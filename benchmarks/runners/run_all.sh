#!/usr/bin/env bash
set -euo pipefail

transports="${BENCH_TRANSPORTS:-zerolink}"
default_glob="${BENCH_SCENARIOS:-benchmarks/scenarios/*.json}"

for t in ${transports}; do
  upper_t="$(echo "${t}" | tr '[:lower:]' '[:upper:]')"
  scoped_var="BENCH_SCENARIOS_${upper_t}"
  scoped_glob="${!scoped_var:-}"
  if [ -n "${scoped_glob}" ]; then
    scenario_glob="${scoped_glob}"
  elif [ "${t}" = "zerolink" ]; then
    # 1MB daemon path is currently too slow for default quick runs.
    scenario_glob="benchmarks/scenarios/p2p_small.json benchmarks/scenarios/p2p_large.json"
  else
    scenario_glob="${default_glob}"
  fi
  for s in ${scenario_glob}; do
    echo "[bench] transport=${t} scenario=${s}"
    bash benchmarks/runners/run_one.sh "${t}" "${s}"
  done
done

python3 benchmarks/scripts/summarize.py
