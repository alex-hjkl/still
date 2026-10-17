#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.."
python3 scripts/check-sensitive.py
python3 - <<'PY'
import ast
from pathlib import Path
for source in Path("scripts").glob("*.py"):
    ast.parse(source.read_text(), filename=str(source))
print("Python script syntax passed")
PY
cargo fmt --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
