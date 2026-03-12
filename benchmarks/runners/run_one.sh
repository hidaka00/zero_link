#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -lt 2 ]; then
  echo "usage: $0 <transport> <scenario.json>"
  exit 1
fi

transport="$1"
scenario="$2"
shift 2

python3 benchmarks/scripts/run_benchmark.py \
  --transport "$transport" \
  --scenario "$scenario" \
  "$@"
