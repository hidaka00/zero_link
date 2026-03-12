#!/usr/bin/env python3
from __future__ import annotations

import argparse
import csv
import pathlib
from collections import defaultdict

REPO_ROOT = pathlib.Path(__file__).resolve().parents[2]
SUMMARY_CSV = REPO_ROOT / "benchmarks" / "results" / "summary" / "latest.csv"


def to_int(v: str, default: int = 0) -> int:
    try:
        return int(v)
    except Exception:
        return default


def to_float(v: str, default: float = 0.0) -> float:
    try:
        return float(v)
    except Exception:
        return default


def fmt_ratio(a: float, b: float) -> str:
    if b <= 0:
        return "n/a"
    return f"{a / b:.2f}x"


def load_latest_by_transport_scenario(path: pathlib.Path) -> dict[tuple[str, str], dict[str, str]]:
    latest: dict[tuple[str, str], dict[str, str]] = {}
    with path.open("r", encoding="utf-8", newline="") as f:
        reader = csv.DictReader(f)
        for row in reader:
            transport = row.get("transport", "")
            scenario = row.get("scenario", "")
            key = (transport, scenario)
            ts = to_int(row.get("timestamp", "0"), 0)
            prev = latest.get(key)
            if prev is None or ts >= to_int(prev.get("timestamp", "0"), 0):
                latest[key] = row
    return latest


def render_markdown(
    latest: dict[tuple[str, str], dict[str, str]], include_partial: bool
) -> str:
    scenarios = sorted({scenario for (_, scenario) in latest.keys()})
    lines: list[str] = []
    lines.append("# Benchmark Comparison (Latest by Transport/Scenario)")
    lines.append("")
    lines.append("| Scenario | Payload(B) | ZeroLink p50(us) | ZeroMQ p50(us) | p50 Ratio (ZL/ZMQ) | ZeroLink p95(us) | ZeroMQ p95(us) | p95 Ratio (ZL/ZMQ) | ZeroLink Throughput(msg/s) | ZeroMQ Throughput(msg/s) | Throughput Ratio (ZMQ/ZL) | ZL Drops | ZMQ Drops |")
    lines.append("|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|")

    for scenario in scenarios:
        row_zl = latest.get(("zerolink", scenario), {})
        row_zmq = latest.get(("zeromq", scenario), {})
        if not include_partial and (not row_zl or not row_zmq):
            continue

        payload = row_zl.get("payload_bytes") or row_zmq.get("payload_bytes") or "-"

        zl_p50 = to_float(row_zl.get("p50_us", "0"), 0.0)
        zmq_p50 = to_float(row_zmq.get("p50_us", "0"), 0.0)
        zl_p95 = to_float(row_zl.get("p95_us", "0"), 0.0)
        zmq_p95 = to_float(row_zmq.get("p95_us", "0"), 0.0)
        zl_tp = to_float(row_zl.get("throughput_msg_s", "0"), 0.0)
        zmq_tp = to_float(row_zmq.get("throughput_msg_s", "0"), 0.0)

        zl_drops = to_int(row_zl.get("dropped_messages", "0"), 0)
        zmq_drops = to_int(row_zmq.get("dropped_messages", "0"), 0)

        zl_p50_s = f"{zl_p50:.2f}" if row_zl else "-"
        zmq_p50_s = f"{zmq_p50:.2f}" if row_zmq else "-"
        zl_p95_s = f"{zl_p95:.2f}" if row_zl else "-"
        zmq_p95_s = f"{zmq_p95:.2f}" if row_zmq else "-"
        zl_tp_s = f"{zl_tp:.2f}" if row_zl else "-"
        zmq_tp_s = f"{zmq_tp:.2f}" if row_zmq else "-"

        p50_ratio = fmt_ratio(zl_p50, zmq_p50) if row_zl and row_zmq else "-"
        p95_ratio = fmt_ratio(zl_p95, zmq_p95) if row_zl and row_zmq else "-"
        tp_ratio = fmt_ratio(zmq_tp, zl_tp) if row_zl and row_zmq else "-"

        lines.append(
            f"| {scenario} | {payload} | {zl_p50_s} | {zmq_p50_s} | {p50_ratio} | {zl_p95_s} | {zmq_p95_s} | {p95_ratio} | {zl_tp_s} | {zmq_tp_s} | {tp_ratio} | {zl_drops if row_zl else '-'} | {zmq_drops if row_zmq else '-'} |"
        )

    if len(lines) == 4:
        lines.append("| - | - | - | - | - | - | - | - | - | - | - | - | - |")

    return "\n".join(lines) + "\n"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--input", default=str(SUMMARY_CSV), help="Path to summary CSV")
    parser.add_argument("--output", default="", help="Optional output markdown file path")
    parser.add_argument(
        "--include-partial",
        action="store_true",
        help="Include scenarios that have only one transport result",
    )
    args = parser.parse_args()

    csv_path = pathlib.Path(args.input)
    if not csv_path.exists():
        raise SystemExit(f"summary csv not found: {csv_path}")

    latest = load_latest_by_transport_scenario(csv_path)
    markdown = render_markdown(latest, args.include_partial)

    if args.output:
        out = pathlib.Path(args.output)
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(markdown, encoding="utf-8")
        print(f"saved={out}")
    else:
        print(markdown)


if __name__ == "__main__":
    main()
