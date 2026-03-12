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

Notes:
- Default `zerolink` run includes `p2p_small` and `p2p_large`.
- Use `BENCH_SCENARIOS_ZEROLINK` to override, e.g.:

```bash
BENCH_SCENARIOS_ZEROLINK="benchmarks/scenarios/p2p_small.json benchmarks/scenarios/p2p_xlarge.json" \
bash benchmarks/runners/run_all.sh
```

Daemon payload-size sweep (`zerolink`):

```bash
bash benchmarks/runners/run_sweep_zerolink_daemon.sh \
  --payload-sizes 256,512,1024,2048,4096 \
  --messages 120 \
  --timeout-ms 10000 \
  --max-inflight 1
```

Results:

- Raw: `benchmarks/results/raw/*.json`
- Summary CSV: `benchmarks/results/summary/latest.csv`
- Comparison Markdown table (latest by transport/scenario):

```bash
python3 benchmarks/scripts/render_compare_markdown.py \
  --output benchmarks/results/summary/latest_compare.md
```
Default is common scenarios only (both `zerolink` and `zeromq`). Add `--include-partial` to include one-sided rows.

- Sweep CSV: `benchmarks/results/summary/sweep_zerolink_daemon_latest.csv`

## Metrics

Each run records:
- `p50_us`
- `p95_us`
- `throughput_msg_s`
- `sent_messages`
- `received_messages`
- `unsent_messages` (not published before send deadline)
- `delivery_dropped_messages` (published but not received)
- `dropped_messages` (= unsent + delivery_dropped)
- `transport`, `scenario`, `timestamp`

Scenario knobs:
- `max_inflight`: upper bound of in-flight messages (stabilizes daemon comparisons for large payloads).
