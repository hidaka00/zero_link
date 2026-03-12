# Benchmarks

This directory provides a scenario-driven benchmark harness for IPC comparisons.

Current transports:
- `zerolink` (implemented)
- `zeromq` (optional; requires `pyzmq`)

## Layout

- `scenarios/*.json`: benchmark scenario definitions.
- `runners/run_one.sh`: run one scenario for one transport.
- `runners/run_all.sh`: run all scenarios for configured transports.
- `scripts/run_benchmark.py`: main runner.
- `scripts/summarize.py`: aggregate raw JSON results into CSV.
- `results/raw/`: per-run raw metrics.
- `results/summary/`: aggregated CSV.

## Prerequisites

From repository root:

```bash
cargo build -p connectord -p zl-cli -p zl-ffi
bash ./scripts/bindings/build_native.sh
python3 -m venv .venv-bench
source .venv-bench/bin/activate
python -m pip install -e bindings/python
```

For `zeromq` transport:

```bash
python -m pip install pyzmq
```

## Run

Single scenario:

```bash
bash benchmarks/runners/run_one.sh zerolink benchmarks/scenarios/p2p_small.json
```

All scenarios for default transports:

```bash
bash benchmarks/runners/run_all.sh
```

Results:

- Raw: `benchmarks/results/raw/*.json`
- Summary CSV: `benchmarks/results/summary/latest.csv`

## Metrics

Each run records:
- `p50_us`
- `p95_us`
- `throughput_msg_s`
- `received_messages`
- `dropped_messages`
- `transport`, `scenario`, `timestamp`

Scenario knobs:
- `max_inflight`: upper bound of in-flight messages (stabilizes daemon comparisons for large payloads).
