#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import json
import os
import pathlib
import subprocess
import sys
import tempfile
from typing import Any


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
RUN_BENCH = REPO_ROOT / "benchmarks" / "scripts" / "run_benchmark.py"
OUT_CSV = REPO_ROOT / "benchmarks" / "results" / "summary" / "sweep_zerolink_daemon_latest.csv"


def parse_sizes(raw: str) -> list[int]:
    vals = []
    for x in raw.split(","):
        x = x.strip()
        if not x:
            continue
        vals.append(int(x))
    if not vals:
        raise ValueError("payload sizes must not be empty")
    return vals


def parse_json_line(output: str) -> dict[str, Any] | None:
    for line in output.splitlines():
        line = line.strip()
        if not line.startswith("{"):
            continue
        try:
            return json.loads(line)
        except json.JSONDecodeError:
            continue
    return None


def run_one(
    payload_bytes: int,
    messages: int,
    warmup_messages: int,
    timeout_ms: int,
    max_inflight: int,
    per_run_timeout_s: int,
    queue_limit: int,
) -> dict[str, Any]:
    scenario = {
        "name": f"sweep_{payload_bytes}",
        "endpoint": "daemon://local",
        "topic": f"bench/sweep/{payload_bytes}",
        "payload_bytes": payload_bytes,
        "messages": messages,
        "warmup_messages": warmup_messages,
        "timeout_ms": timeout_ms,
        "queue_limit": queue_limit,
        "max_inflight": max_inflight,
    }

    with tempfile.NamedTemporaryFile("w", encoding="utf-8", suffix=".json", delete=False) as tf:
        json.dump(scenario, tf)
        tf.flush()
        scenario_path = tf.name

    env = os.environ.copy()
    env.setdefault("ZL_DAEMON_REQUEST_RETRIES", "1")
    env.setdefault("ZL_DAEMON_REQUEST_BACKOFF_MS", "10")

    try:
        cp = subprocess.run(
            [sys.executable, str(RUN_BENCH), "--transport", "zerolink", "--scenario", scenario_path],
            cwd=str(REPO_ROOT),
            env=env,
            capture_output=True,
            text=True,
            timeout=per_run_timeout_s,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return {
            "payload_bytes": payload_bytes,
            "status": "timeout",
            "received_messages": 0,
            "dropped_messages": messages,
            "p50_us": "",
            "p95_us": "",
            "throughput_msg_s": "",
            "notes": f"run timeout ({per_run_timeout_s}s)",
        }
    finally:
        try:
            os.unlink(scenario_path)
        except OSError:
            pass

    parsed = parse_json_line(cp.stdout)
    if cp.returncode != 0 or parsed is None:
        return {
            "payload_bytes": payload_bytes,
            "status": "error",
            "received_messages": 0,
            "dropped_messages": messages,
            "p50_us": "",
            "p95_us": "",
            "throughput_msg_s": "",
            "notes": cp.stderr.strip() or "failed to parse benchmark output",
        }

    received = int(parsed.get("received_messages", 0))
    dropped = int(parsed.get("dropped_messages", messages))
    if received == messages and dropped == 0:
        status = "ok"
    elif received > 0:
        status = "partial"
    else:
        status = "failed"

    return {
        "payload_bytes": payload_bytes,
        "status": status,
        "received_messages": received,
        "dropped_messages": dropped,
        "p50_us": parsed.get("p50_us", ""),
        "p95_us": parsed.get("p95_us", ""),
        "throughput_msg_s": parsed.get("throughput_msg_s", ""),
        "notes": "",
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--payload-sizes", default="256,512,1024,2048,4096,8192,16384,32768,65536")
    parser.add_argument("--messages", type=int, default=120)
    parser.add_argument("--warmup-messages", type=int, default=20)
    parser.add_argument("--timeout-ms", type=int, default=10000)
    parser.add_argument("--max-inflight", type=int, default=1)
    parser.add_argument("--queue-limit", type=int, default=4000)
    parser.add_argument("--per-run-timeout-s", type=int, default=90)
    args = parser.parse_args()

    sizes = parse_sizes(args.payload_sizes)
    rows = []
    for size in sizes:
        row = run_one(
            payload_bytes=size,
            messages=args.messages,
            warmup_messages=args.warmup_messages,
            timeout_ms=args.timeout_ms,
            max_inflight=args.max_inflight,
            per_run_timeout_s=args.per_run_timeout_s,
            queue_limit=args.queue_limit,
        )
        rows.append(row)
        print(
            f"size={row['payload_bytes']} status={row['status']} "
            f"received={row['received_messages']} dropped={row['dropped_messages']} "
            f"p95_us={row['p95_us']} throughput={row['throughput_msg_s']}"
        )

    OUT_CSV.parent.mkdir(parents=True, exist_ok=True)
    with OUT_CSV.open("w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(
            f,
            fieldnames=[
                "payload_bytes",
                "status",
                "received_messages",
                "dropped_messages",
                "p50_us",
                "p95_us",
                "throughput_msg_s",
                "notes",
            ],
        )
        writer.writeheader()
        writer.writerows(rows)

    print(f"saved={OUT_CSV}")


if __name__ == "__main__":
    main()
