#!/usr/bin/env python3
"""Register an extracted Still Linux package in the current user's app menu."""
import argparse
import os
from pathlib import Path
import shutil
import subprocess

APP_ID = "com.still.player"


def exec_value(binary):
    value = str(binary)
    if any(character in value for character in "\n\r="):
        raise ValueError("The package path cannot contain newlines or an equals sign")
    # Desktop-entry string escaping is applied before Exec argument unquoting.
    for character in ("\\", '"', "`", "$"):
        value = value.replace(character, "\\" + character)
    return '"' + value.replace("\\", "\\\\").replace("%", "%%") + '"'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--package-dir", type=Path, default=Path(__file__).resolve().parent)
    args = parser.parse_args()
    package = args.package_dir.resolve()
    binary = package / "still"
    icon = package / f"share/icons/hicolor/scalable/apps/{APP_ID}.svg"
    desktop = package / f"share/applications/{APP_ID}.desktop"
    if not all(path.is_file() for path in (binary, icon, desktop)):
        parser.error("Run this script from an extracted Still Linux release package")
    try:
        # Keep escaped percent signs in an argument: GLib checks the executable
        # path before it expands %% field codes.
        command = f"/usr/bin/env {exec_value(binary)}"
        entry = desktop.read_text(encoding="utf-8").replace("Exec=still\n", f"Exec={command}\n")
    except ValueError as error:
        parser.error(str(error))
    data_home = Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share")
    applications = data_home / "applications"
    icon_theme = data_home / "icons/hicolor"
    icon_dir = icon_theme / "scalable/apps"
    applications.mkdir(parents=True, exist_ok=True)
    icon_dir.mkdir(parents=True, exist_ok=True)
    shutil.copy2(icon, icon_dir / f"{APP_ID}.svg")
    installed_entry = applications / f"{APP_ID}.desktop"
    installed_entry.write_text(entry, encoding="utf-8")
    for command in (
        ["gtk-update-icon-cache", "--force", "--ignore-theme-index", str(icon_theme)],
        ["update-desktop-database", str(applications)],
    ):
        if shutil.which(command[0]):
            subprocess.run(command, check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    print(f"Installed {installed_entry}")
    print("Keep the extracted package in place; run this script again if you move it.")


if __name__ == "__main__":
    main()
