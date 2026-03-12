#!/usr/bin/env python3
from __future__ import annotations

import csv
import json
import pathlib


REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
RAW_DIR = REPO_ROOT / "benchmarks" / "results" / "raw"
SUMMARY_DIR = REPO_ROOT / "benchmarks" / "results" / "summary"


def main() -> None:
    SUMMARY_DIR.mkdir(parents=True, exist_ok=True)
    rows = []
    for path in sorted(RAW_DIR.glob("*.json")):
        try:
            obj = json.loads(path.read_text(encoding="utf-8"))
        except Exception:
            continue
        rows.append(
            {
                "timestamp": obj.get("timestamp", ""),
                "transport": obj.get("transport", ""),
                "scenario": obj.get("scenario", ""),
                "payload_bytes": obj.get("payload_bytes", ""),
                "messages": obj.get("messages", ""),
                "sent_messages": obj.get("sent_messages", ""),
                "received_messages": obj.get("received_messages", ""),
                "unsent_messages": obj.get("unsent_messages", ""),
                "delivery_dropped_messages": obj.get("delivery_dropped_messages", ""),
                "dropped_messages": obj.get("dropped_messages", ""),
                "daemon_dropped_messages": obj.get("daemon_dropped_messages", ""),
                "p50_us": obj.get("p50_us", ""),
                "p95_us": obj.get("p95_us", ""),
                "throughput_msg_s": obj.get("throughput_msg_s", ""),
                "file": path.name,
            }
        )

    out_path = SUMMARY_DIR / "latest.csv"
    with out_path.open("w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(
            f,
            fieldnames=[
                "timestamp",
                "transport",
                "scenario",
                "payload_bytes",
                "messages",
                "sent_messages",
                "received_messages",
                "unsent_messages",
                "delivery_dropped_messages",
                "dropped_messages",
                "daemon_dropped_messages",
                "p50_us",
                "p95_us",
                "throughput_msg_s",
                "file",
            ],
        )
        writer.writeheader()
        for row in rows:
            writer.writerow(row)

    print(f"rows={len(rows)}")
    print(f"saved={out_path}")


if __name__ == "__main__":
    main()
