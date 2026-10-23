# Still

A keyboard-first Navidrome music player built with Rust and GPUI.

macOS is the primary platform. CI builds macOS (Apple Silicon and Intel), Linux
(x86_64), and Windows (x86_64). Linux and Windows do not yet integrate with system
media keys / Now Playing. Windows runtime validation is pending.

Navigation uses Vim-style motions (`h`, `j`, `k`, `l`), with keyboard controls inspired
by [Yazi](https://yazi-rs.github.io/),
[cmux](https://cmux.com/), [T3 Code](https://t3.codes/), and other keyboard-driven apps.

https://github.com/user-attachments/assets/0da23166-ce0f-49e3-aabb-0a53960abc83

## Features

- Browse artists, albums, songs and liked songs. Albums are sorted oldest first.
- Navigate folders in parent, current and preview columns with `h`, `j`, `k`, `l`.
- Stream music with queue advancement, shuffle, seeking, volume and mute.
- Control playback with macOS media keys and Now Playing.
- Find within loaded lists and switch between compact and expanded sidebars.
- Switch between [Catppuccin](https://catppuccin.com/palette/) Latte (light) and
  Macchiato (dark) with the sun / moon button; your choice is saved.

## Run

Install [Rust](https://rustup.rs/). On macOS, install Xcode including its Metal
tools. On Ubuntu / Debian, install build dependencies with
`bash scripts/install-linux-deps.sh`. On Windows, install the Visual Studio C++
Build Tools, Windows SDK and CMake, then run `scripts/setup-windows.ps1` in
PowerShell (dot-source it for a local build: `. ./scripts/setup-windows.ps1`).
Then run:

```sh
cargo run --locked --release
```

Enter your Navidrome URL, username and password. **Remember connection** saves the
connection in the platform credential store (macOS Keychain, Windows Credential
Manager, or a Linux Secret Service provider); **Disconnect & forget** removes it.

To preview the interface without a server or audio playback:

```sh
cargo run --locked --release -- --demo
```

## Build the macOS app

```sh
./scripts/build-macos.sh
```

The app and architecture-specific ZIP are written to `target/release/`.
Builds are ad hoc signed and not notarized.

## CI and release packages

GitHub Actions checks the source and builds optimized packages for all four
OS/architecture combinations on pushes, pull requests and manual CI runs.
Download them from the run's **Artifacts**: macOS `.app.zip`, Linux `.tar.gz`,
or Windows portable `.zip`, with a SHA-256 checksum for each archive.

To build the current platform locally:

```sh
python3 scripts/build-release.py
```

Packages are written to `target/packages/`. A tag matching `v<version>` in
`Cargo.toml` also creates a **draft** GitHub Release after every build passes.
Publishing that draft is a separate manual action. No signing certificates or
personal access token are needed for these initial builds. See
[CI setup and Linux runtime checks](scripts/README.md#ci-and-release-builds).

The Linux CI package is built on Ubuntu 22.04 (glibc 2.35). Run it in a Wayland
or X11 desktop with Vulkan drivers and the runtime libraries described in its
`RUNNING.txt`. Windows packages are unsigned; macOS packages use ad hoc signing.
To add the Linux app and its icon to your application menu, run
`python3 install-desktop.py` from the extracted package and keep that folder in place.

## Keyboard controls

| Shortcut | Action |
| --- | --- |
| `j` / `k`, Up / Down | Move selection |
| `h` / `l`, Backspace / Enter | Go back / open or play |
| `/`, Command+F | Find in the loaded list |
| Space | Play / pause |
| `[` / `]` | Previous / next song |
| Left / Right | Seek backward / forward 10 seconds |
| Command+1 through Command+5 | Switch sections |
| Command+B | Switch compact / expanded sidebar |
| `?`, F1 | Keyboard help |
| Command+Q | Quit |

[All keyboard and mouse controls](docs/shortcuts.md).

## Folder browsing

Enable **Report Real Path** for the Still player in Navidrome's **Players** settings,
then refresh Folders. These are paths on the Navidrome server, not your Mac.
Folder browsing includes indexed audio files and their parent folders.

Find searches only the currently loaded records. There is no offline mode,
playlist management, repeat mode or persistent playback queue.

## Preferences and logs

- Preferences: `~/Library/Application Support/Still/preferences.json`
- Diagnostic log: `~/Library/Logs/Still/still.log`

On Linux, preferences use `$XDG_CONFIG_HOME/still` (default `~/.config/still`)
and logs use `$XDG_STATE_HOME/still/logs` (default `~/.local/state/still/logs`).
On Windows, they use `%APPDATA%\Still` and `%LOCALAPPDATA%\Still\logs` respectively.

Volume, shuffle, sidebar mode, window size, last section and folder selection are
restored between sessions. Connection credentials are stored separately in Keychain.

## Development

```sh
./scripts/check.sh
```

See [development and native UI checks](scripts/README.md) and
[architecture](docs/architecture.md). Third-party notices are in [licenses/](licenses/).
