# Still

A keyboard-oriented Navidrome music player built with Rust and GPUI. The interface
and documentation are in English. macOS is the currently tested platform.

## Features

- Browse Artists, Albums, Songs, Folders and Liked songs.
- Albums appear as a vertical list, sorted from oldest to newest.
- Browse server folders in parent, current and preview columns. Compilations stay
  together regardless of artist tags. Folder selection and Find are restored.
- Stream audio with automatic queue advancement, shuffle, seeking, volume and mute.
- Save likes to Navidrome, with rollback and Retry when a request fails.
- Control playback through macOS media commands and Now Playing.
- Use a compact or expanded sidebar; hold Command to reveal shortcut hints.
- Find within loaded records, with a match count and a clear button.
- Open keyboard help over the library while keeping playback controls available.
- Store an optional connection in the system credential store. Passwords are not
  saved in the preferences file.

There is no offline mode, playlist management, repeat mode or persistent playback
queue. Find is local to loaded records, not a server-wide search. Metadata is shown
as supplied by the server; fix incorrectly encoded source tags in the music library.

## Build and run

Install [Rust through rustup](https://rustup.rs/) and the Xcode command-line tools
on macOS. The project uses Rust 2024 and pins Rust 1.99.1 in `rust-toolchain.toml`. `Cargo.lock` records dependency versions.

```sh
cargo run --locked --release
```

Connect using the server URL, username and password in the form. Enable Remember
connection only when you want credentials saved. Disconnect & forget removes a
saved connection. To inspect the interface without a server:

```sh
cargo run --locked --release -- --demo
```

For automated environments, `NAVIDROME_URL`, `NAVIDROME_USERNAME` and
`NAVIDROME_PASSWORD` can supply a connection. Do not put actual credentials into
source files, shell examples or committed `.env` files. Environment-supplied
connections default to not being remembered.

Build an ad-hoc-signed app and architecture-specific ZIP:

```sh
./scripts/build-macos.sh
```

Outputs are `target/release/Still.app` and `target/release/Still-macOS-arm64.zip`
(or `Still-macOS-x86_64.zip`). This is a local build, not a notarized distribution.
Linux and Windows are not validated by the current native UI suite. GPUI has
additional platform dependencies; consult its upstream build instructions before
attempting another platform.

## Keyboard and mouse

| Shortcut | Action |
| --- | --- |
| `j` / `k`, Up / Down | Select the next / previous record |
| `h` / `l` | Go back / open or play; in Folders, parent / selected child |
| Enter / Backspace | Open or play / go back |
| Shift+Enter | Play a folder and its subfolders |
| `gg` / `G`, Home / End | First / last record |
| Ctrl+D / Ctrl+U | Half a page down / up |
| Ctrl+F / Ctrl+B, PageDown / PageUp | Page down / up |
| `/`, Command+F | Find in the loaded list |
| Enter / Escape in Find | Return to the list, keeping the query |
| Escape in the list | Clear the query |
| Space | Play / pause; open the selected record if the queue is empty |
| `[` / `]` | Previous / next song; previous restarts after the first 3 seconds |
| Left / Right, `H` / `L` | Seek backward / forward 10 seconds |
| `-` / `+` (or `=`) | Decrease / increase volume by 5% |
| `s` | Toggle shuffle without restarting the current song |
| `f` / `F` | Toggle like for the selected / current song |
| Command+1 through Command+5 | Artists, Albums, Songs, Folders, Liked songs |
| Tab / Shift+Tab | Next / previous section |
| `r` / `e` | Refresh / load more records |
| `?`, F1 | Open / close keyboard help |
| Escape in help | Close help |
| Command+B | Switch compact / expanded sidebar |
| Command+Q | Quit, including while editing a field |

Text fields retain normal text-editing behavior. Left / Right moves the caret
while Find is focused. Section shortcuts and Command+B remain available there.
Playback shortcuts work while help is open; browsing commands do not act on the
covered list. Help also closes through Close or a click outside the panel.

A green selection background and blue outline identify the keyboard cursor.
The play/pause indicator identifies the current song independently. Clearing Find
with the × button keeps focus in the field. Drag the timeline to preview a position
and release to seek. Drag volume for continuous adjustment; click the speaker to
mute and restore the previous level within the session.

## Server folders

Enable **Report Real Path** for the Still player in Navidrome's **Players** settings,
then open Folders or use Refresh folders. The tree is built from absolute `path`
fields returned by `search3`. Paths belong to the server, or to its container when
Navidrome runs in Docker. The app does not synthesize filesystem paths from tags.

Folder browsing loads metadata for the accessible music library and caches the tree
until refresh or disconnect. It includes indexed audio and its parent folders, not
empty folders or unrelated files. Recursive playback orders files by filename,
then visits child folders by name. Album and song lists load 200 records per page.

For path diagnostics, run `python3 scripts/check-paths.py`. It prompts for credentials
without echoing the password, prints up to 20 matching server paths, and does not
save credentials or change server settings. Its output may contain private music
paths: inspect it before sharing.

## Preferences and diagnostics

Volume, shuffle, sidebar mode, last section, window size and the last folder with
its selection and query are saved. Restoring a folder is scoped to the server and
account; a missing folder falls back to an existing parent.

On macOS, preferences are in `~/Library/Application Support/Still/preferences.json`
and diagnostics in `~/Library/Logs/Still/still.log`. `STILL_CONFIG_DIR` and
`STILL_LOG_DIR` override those directories for isolated checks. Credentials are
stored separately through GPUI's system credential API.

Preferences failures are logged and shown in a separate notification. Pending
changes stay in memory and saving is retried with an increasing delay, up to
30 seconds. Invalid preferences fall back to defaults with an explanation. Before
the next save, the damaged file is preserved as `preferences.corrupt-*.json` in
the same directory; a failed backup prevents overwriting the original.

Audio preparation has a 30-second deadline covering both opening and decoding.
Switching or stopping a song cancels its transport and wakes a blocked decoder.
Slow seeking does not block Stop or Next. Repeated relative seeks accumulate,
and pending seek targets are coalesced. Audio errors keep their underlying causes
in diagnostics with request URLs redacted.
If the audio worker stops unexpectedly, playback reports a recoverable error;
retrying or pressing Play recreates the worker and keeps the queue and volume.

The panic hook records a backtrace before native unwinding can abort. A log already
at least 5 MiB is rotated on startup to `still.previous.log`. Packaging retains a
matching executable under `target/release/symbols/<UUID>/still` for symbolication.
Build outputs, local reports and credentials must stay outside Git.

## Development

Run `./scripts/check.sh` for source privacy checks, Python syntax, formatting,
Clippy and tests. The same command is configured for macOS in GitHub Actions.
The privacy guard detects common token patterns, private keys and personal paths;
it complements manual review and does not guarantee that every secret is detected. See [native checks](scripts/README.md) for the isolated
macOS UI suite and [architecture](docs/architecture.md) for module responsibilities.
The UI suite uses a local fixture server and never replaces the installed app.

GPUI is pinned to its published 0.2.2 release. Its transitive `block` and
`proc-macro-error2` dependencies currently produce future-compatibility warnings.
The `libc` pin works around GPUI's old `xattr` dependency. These constraints should
be reviewed when an upstream GPUI update becomes available.

## Third-party notices

SVG icons are derived from [Phosphor Icons](https://github.com/phosphor-icons/core).
The text input is adapted from GPUI's input example. Their attribution notices are
kept in `licenses/` and bundled with the app. Still does not currently declare a
project license.
