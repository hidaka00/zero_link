#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo_root"

venv_dir="${1:-.venv-zerolink}"
python3 -m venv "$venv_dir"
source "$venv_dir/bin/activate"

python -m pip install -e bindings/python

echo "Python dev environment is ready: $venv_dir"
echo "Activate with: source $venv_dir/bin/activate"
