#!/usr/bin/env python3
"""Build and package the current host platform without publishing anything."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output-dir", type=Path, default=ROOT / "target/packages")
    args = parser.parse_args()
    metadata = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"], cwd=ROOT, text=True,
    ))
    version = next(package["version"] for package in metadata["packages"] if package["name"] == "still")
    ref = os.environ.get("GITHUB_REF", "")
    if ref.startswith("refs/tags/") and ref != f"refs/tags/v{version}":
        parser.error(f"Release tag must match Cargo.toml: v{version}")
    host = subprocess.check_output(["rustc", "-vV"], cwd=ROOT, text=True)
    host = next(line.removeprefix("host: ") for line in host.splitlines() if line.startswith("host: "))
    architecture = {"aarch64": "arm64", "x86_64": "x86_64"}.get(host.split("-")[0])
    system = {"darwin": "macOS", "linux": "Linux", "win32": "Windows"}.get(sys.platform)
    if system is None or architecture is None:
        parser.error(f"Unsupported host: {host}")
    artifacts = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not artifacts.is_absolute():
        artifacts = ROOT / artifacts
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    name = f"Still-{version}-{system}-{architecture}"
    if system == "macOS":
        subprocess.run(["bash", "scripts/build-macos.sh"], cwd=ROOT, check=True)
        archive = output / f"{name}.zip"
        shutil.copy2(artifacts / f"release/Still-macOS-{architecture}.zip", archive)
    else:
        subprocess.run(["cargo", "build", "--locked", "--release"], cwd=ROOT, check=True)
        with tempfile.TemporaryDirectory(prefix="still-package-") as temporary:
            package = Path(temporary) / name
            package.mkdir()
            executable = "still.exe" if system == "Windows" else "still"
            shutil.copy2(artifacts / "release" / executable, package / executable)
            for source in ("LICENSE", "README.md"):
                if (ROOT / source).is_file():
                    shutil.copy2(ROOT / source, package / source)
            shutil.copytree(ROOT / "licenses", package / "licenses")
            if system == "Linux":
                applications = package / "share/applications"
                icons = package / "share/icons/hicolor/scalable/apps"
                applications.mkdir(parents=True)
                icons.mkdir(parents=True)
                shutil.copy2(ROOT / "assets/linux/com.still.player.desktop", applications)
                shutil.copy2(ROOT / "assets/app-icon/still.svg", icons / "com.still.player.svg")
                shutil.copy2(ROOT / "scripts/install-desktop.py", package)
                # Do not bundle glibc or GPU drivers. List the runtime requirements.
                (package / "RUNNING.txt").write_text(
                    "Run ./still in a Wayland or X11 desktop session with Vulkan drivers.\n"
                    "Run python3 install-desktop.py to add Still and its icon to your app menu.\n"
                    "Keep this extracted folder in place after registering the launcher.\n"
                    "Requires glibc >= 2.35 for the Ubuntu 22.04 CI build, ALSA, Fontconfig,\n"
                    "FreeType, XKBCommon (+ X11), XCB, and Wayland client/cursor libraries.\n"
                    "Linux media-key integration is not implemented yet.\n",
                    encoding="utf-8",
                )
                archive = output / f"{name}.tar.gz"
                with tarfile.open(archive, "w:gz") as bundle:
                    bundle.add(package, arcname=name)
            else:
                archive = output / f"{name}.zip"
                with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as bundle:
                    for source in sorted(package.rglob("*")):
                        if source.is_file():
                            bundle.write(source, source.relative_to(package.parent))
    digest = hashlib.sha256()
    with archive.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    checksum = digest.hexdigest()
    archive.with_name(archive.name + ".sha256").write_text(f"{checksum}  {archive.name}\n", encoding="utf-8")
    print(f"Built {archive}")


if __name__ == "__main__":
    main()
