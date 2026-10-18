#!/usr/bin/env python3
"""Run the same source checks on macOS, Linux and Windows."""
import ast
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    subprocess.run([sys.executable, "scripts/check-sensitive.py"], cwd=ROOT, check=True)
    for source in (ROOT / "scripts").glob("*.py"):
        ast.parse(source.read_text(encoding="utf-8"), filename=str(source))
    print("Python script syntax passed", flush=True)
    for command in (
        ["cargo", "fmt", "--check"],
        ["cargo", "clippy", "--locked", "--all-targets", "--all-features", "--", "-D", "warnings"],
        ["cargo", "test", "--locked", "--all-features"],
    ):
        subprocess.run(command, cwd=ROOT, check=True)


if __name__ == "__main__":
    main()
