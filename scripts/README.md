# Development checks

Run `./scripts/check.sh` for the publication guard, Python syntax, formatting,
Clippy and all unit tests. Rust is pinned by `rust-toolchain.toml`; builds use
`Cargo.lock` with `--locked`. The GitHub Actions workflow runs the same checks.
The cross-platform entry point is `python scripts/check.py` (Python 3.10+).

## CI and release builds

`.github/workflows/check.yml` runs on pushes, pull requests and manual dispatch.
It checks and packages macOS arm64, macOS x86_64, Linux x86_64 (Ubuntu 22.04),
and Windows x86_64 (MSVC). Actions are pinned to commit hashes. Only the tag
release job has write access, and checkout does not persist credentials.

Each job uploads its versioned archive and SHA-256 checksum as an Actions
artifact, retained for 14 days. Download **Still-Windows-x86_64** to test Windows:
unpack both the Actions artifact and the contained release ZIP, then run
`still.exe`. Keep the accompanying license files. Windows runtime verification
is currently manual; a successful CI build does not establish that it runs.

To prepare a native release locally:

```sh
python3 scripts/build-release.py
```

Output: `target/packages/Still-<version>-<OS>-<arch>.<zip|tar.gz>` and a matching
`.sha256` file. `CARGO_TARGET_DIR` may redirect the build directory; use
`--output-dir` to redirect packages. The macOS packager reuses `build-macos.sh`.
The other packages include the executable, README and licenses. The Linux
package relies on system libraries rather than bundling glibc or GPU drivers.

Application icons are checked-in exports of [the selected SVG](../assets/app-icon/still.svg).
macOS bundles include `Still.icns`; Windows builds embed the ICO as resource ID 1
for Explorer, the taskbar and GPUI windows. Linux packages contain a desktop entry
and an SVG in the hicolor theme, matching the window's `com.still.player` app ID.
From the extracted Linux package, run `python3 install-desktop.py` to register the
launcher and icon for the current user. Keep that folder in place; register again
after moving it. `XDG_DATA_HOME` is respected, and no root access is needed.
See [icon export and review notes](../assets/app-icon/README.md) before updating assets.

Prerequisites:

- Linux: `bash scripts/install-linux-deps.sh` on Ubuntu / Debian (requires root
  or sudo). A desktop session with working Vulkan drivers is needed to run.
- Windows: Rust MSVC, Visual Studio C++ Build Tools, CMake and a Windows SDK.
  Dot-source `. ./scripts/setup-windows.ps1` in PowerShell before building.
  The script finds `fxc.exe` across installed SDK versions and configures
  `GPUI_FXC_PATH`; in Actions it exports that value to later steps as well.
- macOS: Xcode with Metal tools, the selected command-line tools and Python 3.

To make a release, update `Cargo.toml` and `Cargo.lock`, commit the desired
source, and push a matching version tag (for example `v0.1.0`). Pushing is an
explicit remote action; do it only with the repository owner's approval.
The workflow rejects tags that do not match the package version. After all
four jobs pass, it verifies the archive checksums and creates a **draft**
GitHub Release with those files. Re-running can update the draft, but cannot
replace an already-published release. Review the draft and publish it manually.
Manual dispatch and branch/PR builds only produce Actions artifacts.

`GITHUB_TOKEN` is supplied by Actions; no personal token or signing secrets are
required. macOS bundles are ad hoc signed, Windows binaries are unsigned.
Developer ID signing/notarization and Windows signing can be added separately.

## Linux native smoke checks

Run inside a real desktop session with a working audio output:

```sh
python3 scripts/smoke-linux.py --backend wayland
python3 scripts/smoke-linux.py --backend x11
```

The runner builds the release with `ui-tests`, opens the real GPUI window,
starts a loopback fixture server, streams synthetic silent WAV audio, and
checks navigation, Unicode folders, help, resizing, library search, playback,
pause/resume, seeking, Next and restoring preferences on a second launch.
It verifies audio position advancement and seek results, with no dependency
on the macOS Now Playing bridge. Reports and logs stay in ignored
`.local/ui-test-results/linux-*` directories. No real saved connection or
library is used. The scenario does not exercise credential-store persistence
or native system media keys. It is a local interactive-desktop check, not a
headless CI job. `--no-build` reuses a binary already built with `ui-tests`;
`--hold-seconds 10` leaves time for visual inspection before the first quit.

After testing, run `python3 scripts/build-release.py` again to restore and
package the ordinary binary without the test driver.

Unit tests include preferences write failure/recovery, retry backoff, damaged-file
preservation, a panicked audio-state publisher, canceled/stalled HTTP bodies and
the complete audio preparation deadline. Device-free Rodio tests hold a seek open
while checking accumulated relative seeks, target replacement, Stop, Next and
stale completion rejection. HTTP error tests check retained causes and URL redaction.

## Record the README demo

On macOS with FFmpeg and the Xcode command-line tools installed:

```sh
python3 scripts/smoke-macos.py --record-demo .local/demo
```

This builds the `ui-tests` feature, launches an isolated app and local fixture
server, and records only that process's window using macOS `screencapture`.
The native driver displays keystroke captions and checks the resulting selection,
navigation, playback and sidebar state. Original geometric covers, fictional
metadata and silent generated audio come from `demo_fixture.py`. No saved login
or real music library is used. The captions are excluded from normal releases.

The output contains a looping GIF and a silent H.264 MP4. The native title bar is
cropped out; no desktop, microphone or system audio is recorded. macOS may request
Screen Recording access for the host that runs the script. Exported media has its
metadata removed. Assertions and state snapshots are saved under `.local/ui-test-results/`.

Before replacing `docs/demo/still.mp4`, inspect the entire
recording for private content and check that captions match the actions. Then
update its SHA-256 hash in `docs/demo/manifest.json`. The publication guard
accepts only that exact reviewed file; changed or other binary files require
another review. Keep raw captures and intermediate frames in `.local/`.

## Native macOS checks

Build the binary, app bundle and architecture-specific ZIP with one command:

```sh
./scripts/build-macos.sh
```

The script creates `target/release/Still.app` and `Still-macOS-arm64.zip` (or
`Still-macOS-x86_64.zip`), signs the app ad hoc, and verifies its signature and ZIP.
It also works with `CARGO_TARGET_DIR`. Python 3 and the macOS command-line tools are required.

Run the unattended native UI suite:

```sh
python3 scripts/smoke-macos.py --automated
```

The runner builds with the `ui-tests` feature and puts that binary into an isolated
copy of the app. It sends keystrokes through GPUI and checks state after real native
GPUI layout and paint frames, including Core Text layout. A test-only timer drives
these frames even when macOS suspends display callbacks for a covered window. It covers long Cyrillic paths and
emoji, j/k/h/l, parent/current/preview column paths, visible selection, page movement,
780×640 and 1120×760 windows, search entry, Cmd+B between compact and expanded sidebars while searching, restoring the mode
after a restart, fixed painted icon positions in both modes, switching sections, playback, restoring
the exact folder and track in a second process, ignoring legacy saved filters without writing them back, and Cmd+Q from a password field
in a third process. The connection form is checked at 780×640 after a rejected
login, including the painted bounds of its controls. A long malformed response
also checks scrolling to the bottom and back to the fields.
Artist and album queues are started with Shift+Enter without changing pages,
including a failed request and retry. Folder queues include nested directories.
The footer is checked for context-specific shortcuts and bounds at narrow widths.
It also invokes the shared native command dispatchers for
play/pause/toggle/stop/next/previous, checks repeated play/pause and resuming after
stop, verifies recovery after the audio worker stops, arrow-key seeking in folders and songs (and text editing in search),
three arrow keys in one frame and a batch of three native Skip commands,
checks playback shortcuts with keyboard help open without navigating the hidden library,
and reads title, position and playback state back from MPNowPlayingInfoCenter.
The unchanged library columns behind the help panel are also checked.
Library search checks server results beyond the first loaded album page, all three
result categories, pagination, opening results and returning to the saved query,
Unicode song search and playback, clearing back to the previous list with input
focus intact, editing in the middle of a query without moving the caret,
request failure / retry, and rejecting a delayed response after the query changes
or the field is cleared.
Theme checks cover Latte / Macchiato switching, preserving search focus and text,
restoring the theme after restart, System appearance changes and manual overrides,
keyboard selection / cancellation in the appearance menu, and both library and
connection-screen palettes. Resizing checks keep search focused while it moves
between the wide header and a second row. Text steps wait for stable painted focus,
assert the intended field and reject changes to other inputs; the connection case
repeats rapid Tab / text sequences without idle steps. To stress that case across
fresh processes without replaying library and audio scenarios:

```sh
python3 scripts/smoke-macos.py --automated --connection-only --repeat 5
```
The suite simulates an unavailable credential store without reading or writing saved
logins, navigates to a failing library request, and retries each operation separately.
It also checks playback Retry across navigation and verifies that restoring Folders
makes no Artists request.
These checks do not send global media keys to unrelated players. No Accessibility
permission or UI automation library is needed.

Every launch must exit 0, produce successful assertions and contain no panic in
its log. Assertion failures include the expected and observed state. Successful
scripts and snapshots are saved under `.local/ui-test-results/<run-id>/`; failures
and logs go to `.local/smoke-failures/<run-id>/`. Run `./scripts/build-macos.sh`
afterward to build the normal release without the test driver. The installed
`Still.app` is not replaced by the test suite.

Run a manual native smoke scenario on a local fixture server:

For the keyboard help overlay, use `--size 780 640`, `--size 1030 640`, or
`--size 1120 760`. Open ? and scroll to the final `Quit Still` row;
key labels and descriptions should stay separate and readable, and Close
should remain visible above the scrolling content. The library stays visible behind the
panel. Clicking outside closes help without activating the row beneath it; the player
stays available. In search, check the result count and clear ×; clearing returns to the previous
list and selection while retaining input focus. Drag volume, mute/unmute, and verify the previous level returns. Drag the
seek bar while paused, including releasing outside its bounds; the preview time
should match the committed position and the track should stay paused.

```sh
python3 scripts/smoke-macos.py --case library --relaunch
```

Each run copies the built app to a temporary directory, assigns a unique bundle ID,
and isolates preferences with `STILL_CONFIG_DIR`. Fixture logins have Remember
connection disabled. No saved connection is read. Keep it disabled during these checks.
The server only logs endpoint names, IDs and offsets. Close the test app with Cmd+Q;
the harness must print `"exitCode": 0`. The temporary directory is removed on exit. On failure, stdout/stderr and diagnostic logs
are copied to `.local/smoke-failures/<run-id>/` before cleanup.

## Library and saved preferences

1. Open Albums with Cmd+2. Check that each album occupies one full-width row.
2. Press j, j, k and check that the selection moves down, down, up. Check the
   arrow keys, PageDown and PageUp, including after resizing the window.
3. Press e to load another page. The fixture fails once; click Retry. The count
   must grow from 200 to 400, with two requests at offset 200 in the log.
4. Press G, Enter, then Backspace. All 400 albums and the last selection remain.
5. Change volume, turn on shuffle, resize the window, and leave Albums selected.
6. Press Cmd+Q. With `--relaunch`, the second launch must restore volume, shuffle,
   window size and the Albums section. Press Cmd+Q again. Both exits must be 0.

## Folder columns

```sh
python3 scripts/smoke-macos.py --case folders
```

1. Press Cmd+4. Check the three columns: Parent folder, Current folder, Folder preview.
2. Press l twice to enter `/music/Collections`. Moving j/k between Film Alpha
   and Film Beta must update the right column without entering either folder.
3. Enter Film Alpha, select CD 2 with j, then press l. The left column highlights
   CD 2; the middle lists its tracks; the right shows the selected track details.
4. Press h, l. CD 2 and its selected track must be restored. Left/Right seek the playing track by −10/+10 seconds without changing folders.
5. In CD 2, press G, PageUp, PageDown; selection stays visible in the middle.
   Resize the window and repeat. Enter plays the selected track; Shift+Enter
   plays the folder including subfolders.
6. In CD 2, select a track, focus search and enter its title. Results must come
   from the whole library. Clear × to restore CD 2 and the selected track while
   keeping input focus; Escape returns keyboard focus to the folder list.
7. Click a track in the preview column. The middle must move to its folder and
   playback must use that exact track. Check Refresh folders and an empty search
   result. Cmd+Q must print exitCode 0.

## Unicode crash regression

```sh
python3 scripts/smoke-macos.py --case unicode
```

This fixture includes long Cyrillic directory names, filenames, artist names,
albums and emoji. The previous build aborted when entering the long collection
name with l. Press Cmd+4, then l twice. Continue into both film folders, move
between rows, go back, play a track and resize the window. Long labels should
occupy a single line and stay within their column; track details in the preview
can wrap.
Also check Songs and Albums. Quit with Cmd+Q: exitCode must be 0 and panics 0.
Each test uses its own STILL_LOG_DIR. Production app logs are described in the
main README; symbol images are retained under `target/release/symbols/<UUID>/`.

## Quit while editing

```sh
python3 scripts/smoke-macos.py --case search
```

Press Cmd+F, enter a search query, then Cmd+Q while the caret is still in the input.
The process must exit with code 0.

```sh
python3 scripts/smoke-macos.py --case connection
```

The connection form opens without reading Keychain. Focus the password input,
then press Cmd+Q. The process must exit with code 0.
