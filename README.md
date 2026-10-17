# Still

A keyboard-first Navidrome music player for macOS, built with Rust and GPUI.

https://github.com/user-attachments/assets/0da23166-ce0f-49e3-aabb-0a53960abc83

## Features

- Browse artists, albums, songs and liked songs. Albums are sorted oldest first.
- Navigate folders in parent, current and preview columns with `h`, `j`, `k`, `l`.
- Stream music with queue advancement, shuffle, seeking, volume and mute.
- Control playback with macOS media keys and Now Playing.
- Find within loaded lists and switch between compact and expanded sidebars.

## Run

Install [Rust](https://rustup.rs/) and the Xcode command-line tools, then run:

```sh
cargo run --locked --release
```

Enter your Navidrome URL, username and password. **Remember connection** saves the
connection in macOS Keychain; **Disconnect & forget** removes it.

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

Volume, shuffle, sidebar mode, window size, last section and folder selection are
restored between sessions. Connection credentials are stored separately in Keychain.

## Development

```sh
./scripts/check.sh
```

See [development and native UI checks](scripts/README.md) and
[architecture](docs/architecture.md). Third-party notices are in [licenses/](licenses/).
