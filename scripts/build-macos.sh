#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != Darwin ]]; then
    echo "This script requires macOS (codesign and ditto)." >&2
    exit 1
fi

project_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_dir"
cargo build --locked --release

artifact_dir="${CARGO_TARGET_DIR:-target}/release"
staging_dir="$(mktemp -d "$artifact_dir/still-package.XXXXXX")"
trap 'rm -rf "$staging_dir"' EXIT
export STILL_PACKAGE_DIR="$staging_dir"
python3 - <<'PY'
import os
import plistlib
import re
import shutil
import subprocess
from pathlib import Path

staging = Path(os.environ["STILL_PACKAGE_DIR"])
artifacts = Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "release"
bundle = staging / "Still.app"
executable = bundle / "Contents/MacOS/still"
executable.parent.mkdir(parents=True)
shutil.copy2(artifacts / "still", executable)
uuid_output = subprocess.check_output(["dwarfdump", "--uuid", str(executable)], text=True)
image_uuid = re.search(r"UUID: ([0-9A-Fa-f-]+)", uuid_output).group(1)
symbols = artifacts / "symbols" / image_uuid
symbols.mkdir(parents=True, exist_ok=True)
shutil.copy2(executable, symbols / "still")
print(f"Saved symbol image to {symbols / 'still'}")
version = re.search(r'^version\s*=\s*"([^"]+)"', Path("Cargo.toml").read_text(), re.M).group(1)
info = dict(CFBundleDisplayName="Still", CFBundleName="Still", CFBundleExecutable="still",
            CFBundleIdentifier="com.still.player", CFBundlePackageType="APPL",
            CFBundleShortVersionString=version, CFBundleVersion="1",
            CFBundleDevelopmentRegion="en", NSHighResolutionCapable=True,
            CFBundleIconFile="Still.icns")
(bundle / "Contents/Info.plist").write_bytes(plistlib.dumps(info))
licenses = bundle / "Contents/Resources/licenses"
shutil.copytree("licenses", licenses)
shutil.copy2("assets/app-icon/still.icns", bundle / "Contents/Resources/Still.icns")
PY

codesign --force --sign - "$staging_dir/Still.app"
codesign --verify --deep --strict "$staging_dir/Still.app"
archive_name="Still-macOS-$(uname -m).zip"
ditto -c -k --keepParent "$staging_dir/Still.app" "$staging_dir/$archive_name"
export STILL_PACKAGE_ARCHIVE="$archive_name"
python3 - <<'PY'
import os
import zipfile
from pathlib import Path

staging = Path(os.environ["STILL_PACKAGE_DIR"])
artifacts = Path(os.environ.get("CARGO_TARGET_DIR", "target")) / "release"
archive = os.environ["STILL_PACKAGE_ARCHIVE"]
with zipfile.ZipFile(staging / archive) as package:
    if package.testzip() is not None:
        raise RuntimeError("Archive verification failed")
target = artifacts / "Still.app"
backup = staging / "previous.app"
if target.exists():
    target.rename(backup)
try:
    (staging / "Still.app").rename(target)
except OSError:
    if backup.exists():
        backup.rename(target)
    raise
os.replace(staging / archive, artifacts / archive)
print(f"Built and verified {target} and {artifacts / archive}")
PY
