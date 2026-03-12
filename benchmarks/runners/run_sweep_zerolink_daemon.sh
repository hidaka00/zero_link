#!/usr/bin/env bash
set -euo pipefail

python3 benchmarks/scripts/sweep_zerolink_daemon.py "$@"
