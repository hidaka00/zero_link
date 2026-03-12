#!/usr/bin/env bash
set -euo pipefail

transports="${BENCH_TRANSPORTS:-zerolink}"
scenario_glob="${BENCH_SCENARIOS:-benchmarks/scenarios/*.json}"

for t in ${transports}; do
  for s in ${scenario_glob}; do
    echo "[bench] transport=${t} scenario=${s}"
    bash benchmarks/runners/run_one.sh "${t}" "${s}"
  done
done

python3 benchmarks/scripts/summarize.py
