#!/usr/bin/env bash
# Build the educational Python bindings (maturin + pyo3).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/python"

if ! command -v python3 >/dev/null; then
  echo "setup-python-edu: python3 not found" >&2
  exit 1
fi

if [[ ! -d .venv ]]; then
  python3 -m venv .venv
fi
# shellcheck disable=SC1091
source .venv/bin/activate
python -m pip install -U pip maturin
# Python newer than PyO3's pinned max: still build via abi3.
export PYO3_USE_ABI3_FORWARD_COMPATIBILITY="${PYO3_USE_ABI3_FORWARD_COMPATIBILITY:-1}"
maturin develop --release

echo
echo "OK — activate with:  source python/.venv/bin/activate"
echo "Try:  python -m room ../mods/doom1-python-hello/hello_mod.py --skill 4"
