#!/usr/bin/env python3
"""Check source files for common secrets and personal paths; never print matched values.

This is a publication guard, not a complete credential detector. Synthetic test
credentials and reserved example domains are intentional and require code review.
"""
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
PATTERNS = {
    "private key": re.compile(r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----"),
    "access token": re.compile(r"\b(?:gh[pousr]_[A-Za-z0-9]{30,}|github_pat_[A-Za-z0-9_]{40,}|sk-[A-Za-z0-9_-]{24,}|AKIA[0-9A-Z]{16})\b"),
    "personal absolute path": re.compile(r"/(?:Users|home)/[A-Za-z0-9_-]+/"),
    "private network address": re.compile(r"(?<![\d.])(?:10\.\d+\.\d+\.\d+|192\.168\.\d+\.\d+|172\.(?:1[6-9]|2\d|3[01])\.\d+\.\d+)(?![\d.])"),
}


def main():
    result = subprocess.run(["git", "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
                            cwd=ROOT, check=True, capture_output=True)
    findings = []
    paths = sorted(set(result.stdout.decode().split("\0")) - {""})
    for name in paths:
        path = ROOT / name
        if not path.is_file():
            continue
        if path.name.startswith(".env") or path.suffix in {".pem", ".key", ".p12", ".pfx"}:
            findings.append((name, 0, "credential file"))
            continue
        try:
            content = path.read_text()
        except UnicodeError:
            findings.append((name, 0, "binary file needs manual privacy review"))
            continue
        for line_number, line in enumerate(content.splitlines(), 1):
            for label, pattern in PATTERNS.items():
                if pattern.search(line):
                    findings.append((name, line_number, label))
    for name, line, label in findings:
        print(f"{name}:{line}: {label}", file=sys.stderr)
    print(f"Publication check: {len(paths)} files, {len(findings)} findings (matched values redacted)")
    return bool(findings)


if __name__ == "__main__":
    sys.exit(main())
