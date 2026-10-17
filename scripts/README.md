# Native macOS checks

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
780×640 and 1120×760 windows, filter entry, Cmd+B between compact and expanded sidebars while filtering, restoring the mode
after a restart, fixed painted icon positions in both modes, switching sections, playback, restoring
the exact folder/track/filter in a second process, and Cmd+Q from a password field
in a third process. It also invokes the shared native command dispatchers for
play/pause/toggle/stop/next/previous, checks repeated play/pause and resuming after
stop, verifies arrow-key seeking in folders and songs (and text editing in the filter),
checks playback shortcuts with keyboard help open without navigating the hidden library,
and reads title, position and playback state back from MPNowPlayingInfoCenter.
The Find count and the unchanged library columns behind the help panel are also checked.
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
stays available. In Find, check the result count and clear ×; clearing keeps input
focus. Drag volume, mute/unmute, and verify the previous level returns. Drag the
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
6. Go back to Film Alpha, filter to CD 1, enter it, then return. The filter and
   selection must remain. Click CD 2 in the unfiltered parent column, then h:
   the filter clears if needed so the returned child stays selected.
7. Click a track in the preview column. The middle must move to its folder and
   playback must use that exact track. Check Refresh folders and an empty filter
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
python3 scripts/smoke-macos.py --case filter
```

Press Cmd+F, enter a filter, then Cmd+Q while the caret is still in the input.
The process must exit with code 0.

```sh
python3 scripts/smoke-macos.py --case connection
```

The connection form opens without reading Keychain. Focus the password input,
then press Cmd+Q. The process must exit with code 0.
