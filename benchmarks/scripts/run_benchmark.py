#!/usr/bin/env python3
from __future__ import annotations

import argparse
import json
import os
import pathlib
import statistics
import subprocess
import sys
import threading
import time
from dataclasses import dataclass
from typing import Any


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
PY_BINDING_SRC = REPO_ROOT / "bindings" / "python" / "src"
if str(PY_BINDING_SRC) not in sys.path:
    sys.path.insert(0, str(PY_BINDING_SRC))


@dataclass
class Scenario:
    name: str
    endpoint: str
    topic: str
    payload_bytes: int
    messages: int
    warmup_messages: int
    timeout_ms: int
    queue_limit: int


def load_scenario(path: pathlib.Path) -> Scenario:
    obj = json.loads(path.read_text(encoding="utf-8"))
    return Scenario(
        name=str(obj["name"]),
        endpoint=str(obj.get("endpoint", "daemon://local")),
        topic=str(obj.get("topic", "bench/p2p/default")),
        payload_bytes=int(obj["payload_bytes"]),
        messages=int(obj["messages"]),
        warmup_messages=int(obj.get("warmup_messages", 0)),
        timeout_ms=int(obj.get("timeout_ms", 5000)),
        queue_limit=int(obj.get("queue_limit", 2000)),
    )


def percentile_us(values: list[float], p: float) -> float:
    if not values:
        return 0.0
    if len(values) == 1:
        return values[0]
    k = (len(values) - 1) * p
    f = int(k)
    c = min(f + 1, len(values) - 1)
    if f == c:
        return values[f]
    return values[f] + (values[c] - values[f]) * (k - f)


def ensure_results_dir() -> pathlib.Path:
    out = REPO_ROOT / "benchmarks" / "results" / "raw"
    out.mkdir(parents=True, exist_ok=True)
    return out


def start_connectord(queue_limit: int) -> tuple[subprocess.Popen[Any], pathlib.Path]:
    log_path = REPO_ROOT / "benchmarks" / "results" / "raw" / "connectord-bench.log"
    log_file = log_path.open("w", encoding="utf-8")
    proc = subprocess.Popen(
        [
            str(REPO_ROOT / "target" / "debug" / "connectord"),
            "serve",
            "--control-endpoint",
            "daemon://local",
            "--tick-ms",
            "50",
            "--queue-limit",
            str(queue_limit),
        ],
        stdout=log_file,
        stderr=subprocess.STDOUT,
        cwd=str(REPO_ROOT),
    )
    time.sleep(0.4)
    return proc, log_path


def run_zerolink(s: Scenario) -> dict[str, Any]:
    from zerolink import Client, MsgHeader

    proc = None
    try:
        if s.endpoint.startswith("daemon://"):
            proc, _ = start_connectord(s.queue_limit)

        payload = ("x" * s.payload_bytes).encode("utf-8", errors="ignore")
        latencies_us: list[float] = []
        sent_at_ns: dict[int, int] = {}
        lock = threading.Lock()
        done = threading.Event()
        total_needed = s.messages
        received_count = 0

        def on_msg(_topic: str, header: Any, _payload: bytes) -> None:
            nonlocal received_count
            now_ns = time.perf_counter_ns()
            with lock:
                start_ns = sent_at_ns.pop(int(header.trace_id), None)
                if start_ns is not None:
                    latencies_us.append((now_ns - start_ns) / 1000.0)
                    received_count += 1
                    if received_count >= total_needed:
                        done.set()

        with Client(s.endpoint) as client:
            client.subscribe(s.topic, on_msg)
            # Warmup
            for i in range(s.warmup_messages):
                trace = 1_000_000 + i
                client.publish(s.topic, payload, header=MsgHeader(trace_id=trace))

            start_ns = time.perf_counter_ns()
            for i in range(s.messages):
                trace_id = i + 1
                with lock:
                    sent_at_ns[trace_id] = time.perf_counter_ns()
                client.publish(s.topic, payload, header=MsgHeader(trace_id=trace_id))

            if not done.wait(timeout=s.timeout_ms / 1000.0):
                # continue to compute with partial result
                pass
            end_ns = time.perf_counter_ns()
            client.unsubscribe(s.topic)

        latencies_us.sort()
        received = len(latencies_us)
        dropped = max(0, s.messages - received)
        duration_s = max(1e-9, (end_ns - start_ns) / 1_000_000_000.0)
        throughput = received / duration_s
        return {
            "transport": "zerolink",
            "scenario": s.name,
            "payload_bytes": s.payload_bytes,
            "messages": s.messages,
            "received_messages": received,
            "dropped_messages": dropped,
            "p50_us": round(percentile_us(latencies_us, 0.50), 2),
            "p95_us": round(percentile_us(latencies_us, 0.95), 2),
            "throughput_msg_s": round(throughput, 2),
        }
    finally:
        if proc is not None and proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                proc.kill()


def run_zeromq(s: Scenario) -> dict[str, Any]:
    try:
        import zmq  # type: ignore
    except Exception as exc:
        raise RuntimeError(
            "zeromq transport requires pyzmq (`python -m pip install pyzmq`)"
        ) from exc

    addr = os.environ.get("BENCH_ZMQ_ADDR", "tcp://127.0.0.1:29555")
    ctx = zmq.Context.instance()
    pub = ctx.socket(zmq.PUB)
    sub = ctx.socket(zmq.SUB)
    sub.setsockopt_string(zmq.SUBSCRIBE, "")
    pub.bind(addr)
    sub.connect(addr)
    time.sleep(0.2)

    payload = b"x" * s.payload_bytes
    sent_at_ns: dict[int, int] = {}
    latencies_us: list[float] = []

    # Warmup
    for i in range(s.warmup_messages):
        pub.send((1_000_000 + i).to_bytes(8, "little", signed=False) + payload)
    time.sleep(0.05)

    start_ns = time.perf_counter_ns()
    for i in range(s.messages):
        trace = i + 1
        sent_at_ns[trace] = time.perf_counter_ns()
        pub.send(trace.to_bytes(8, "little", signed=False) + payload)

    deadline = time.time() + (s.timeout_ms / 1000.0)
    while time.time() < deadline and len(latencies_us) < s.messages:
        try:
            msg = sub.recv(flags=zmq.NOBLOCK)
        except zmq.Again:
            time.sleep(0.001)
            continue
        if len(msg) < 8:
            continue
        trace = int.from_bytes(msg[:8], "little", signed=False)
        sent = sent_at_ns.pop(trace, None)
        if sent is None:
            continue
        latencies_us.append((time.perf_counter_ns() - sent) / 1000.0)
    end_ns = time.perf_counter_ns()

    pub.close(0)
    sub.close(0)

    latencies_us.sort()
    received = len(latencies_us)
    dropped = max(0, s.messages - received)
    duration_s = max(1e-9, (end_ns - start_ns) / 1_000_000_000.0)
    throughput = received / duration_s
    return {
        "transport": "zeromq",
        "scenario": s.name,
        "payload_bytes": s.payload_bytes,
        "messages": s.messages,
        "received_messages": received,
        "dropped_messages": dropped,
        "p50_us": round(percentile_us(latencies_us, 0.50), 2),
        "p95_us": round(percentile_us(latencies_us, 0.95), 2),
        "throughput_msg_s": round(throughput, 2),
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--transport", required=True, choices=["zerolink", "zeromq"])
    parser.add_argument("--scenario", required=True)
    args = parser.parse_args()

    scenario = load_scenario(pathlib.Path(args.scenario))
    started = time.time()
    if args.transport == "zerolink":
        result = run_zerolink(scenario)
    else:
        result = run_zeromq(scenario)
    result["timestamp"] = int(started)

    out_dir = ensure_results_dir()
    out_path = out_dir / f"{result['timestamp']}-{result['transport']}-{result['scenario']}.json"
    out_path.write_text(json.dumps(result, ensure_ascii=True, indent=2), encoding="utf-8")
    print(json.dumps(result, ensure_ascii=True))
    print(f"saved={out_path}")


if __name__ == "__main__":
    main()
