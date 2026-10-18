#!/usr/bin/env bash
# Build dependencies for Ubuntu/Debian; runtime libraries come from these packages.
set -euo pipefail
if [[ "$EUID" -eq 0 ]]; then
    elevate=()
else
    elevate=(sudo)
fi
"${elevate[@]}" apt-get update
"${elevate[@]}" apt-get install -y --no-install-recommends \
    build-essential cmake pkg-config \
    libasound2-dev libfontconfig-dev libfreetype-dev \
    libxkbcommon-dev libxkbcommon-x11-dev libxcb1-dev libxcb-xkb-dev \
    libwayland-dev libvulkan-dev
