# Architecture

Still is a single desktop application. Domain models are kept independent of GPUI
where practical; views adapt user input and asynchronous results to those models.

- `browser.rs`, `folders.rs`: loaded records, filtered entries, navigation history,
  server folder tree, selection restoration and queue inputs.
- `api.rs`: Subsonic requests, response decoding and authentication. MD5 is used
  for the protocol's salted token, not as a general-purpose password hash.
- `queue.rs`, `likes.rs`: playback order and optimistic like reconciliation.
- `player.rs`: audio worker, cancelable buffering/decoding and watch-channel snapshots.
- `playback_controller.rs`: queue, playback policy, worker recovery and native media
  integration. Views read its state and send commands through its methods.
- `library_loader.rs`: pending background library task, cooperative cancellation
  and completion events. The view reconciles results with current navigation.
- `media.rs`, `media_macos.rs`: native media commands and Now Playing metadata.
- `connection.rs`: connection controller owning authentication, credential persistence,
  operation identities and connection failures. It emits connection events for the view.
  `credentials.rs` adapts the system credential store.
- `theme.rs`: Catppuccin Latte / Macchiato palettes and a separate saved
  `ThemePreference` (System / Latte / Macchiato). New preferences follow the OS;
  existing explicit palette choices are preserved. The active GPUI global contains
  only the resolved palette. System mode observes window appearance changes;
  manual modes ignore them. Appearance callbacks read the current platform value
  rather than GPUI’s cached window value; activation also rechecks background changes.
  On macOS, System clears the native AppKit appearance
  override while manual choices set Aqua / Dark Aqua. Refreshes include independent
  inputs, sliders and tooltips.
- `settings.rs`, `diagnostics.rs`: background preferences persistence and diagnostics.
- `slider.rs`: reusable GPUI slider with pointer state and preview/change events.
- `ui.rs`: application view, navigation and coordination of controllers.
- `ui_components.rs`: shared buttons, icons, tooltips, rows and empty states.
- `demo.rs`: local demo records and loading without a Navidrome server.
- `theme_picker.rs`: mouse and keyboard appearance selection, restoring the
  previous focus when its menu closes.
- `library_view.rs`, `sidebar.rs`: library lists, headers and sidebar rendering.
- `connection_screen.rs`, `folder_columns.rs`, `player_bar.rs`, `keyboard_help.rs`:
  screen rendering and view-specific interactions.
- `keyboard.rs`, `input.rs`: contextual commands and native Unicode/IME text input.
- `requests.rs`, `failure.rs`: stale-request cancellation and operation-specific Retry.
  Library, playback and each song's like have independent failures. Navigation clears
  only the library failure; connection failures belong to the connection controller.
- `ui_tests.rs`: opt-in native regression driver and paced demo captions, excluded
  from normal releases. `scripts/demo_fixture.py` supplies synthetic demo records
  and original covers; `scripts/demo_recording.py` captures the isolated window.

Authentication checks the server with `ping`; the view then loads the saved section
through the normal library loader. An unrelated Artists failure cannot prevent login.
Credential persistence failures keep their retry payload until success, dismissal or
disconnect, independently of library navigation and playback failures.

API requests and image loading run on GPUI's background executor. Audio uses a
separate command worker and a Tokio runtime; blocking decoder work stays off the
UI and async runtime threads. Request identities and playback generations prevent
results from an old operation from replacing current state.

`Arc` shares immutable loaded collections. Mutable UI state belongs to the GPUI
entity or its controllers. The audio worker publishes snapshots through a Tokio
watch channel; the UI does not lock a shared audio-state mutex. A closed publisher
becomes a recoverable playback failure. Retrying recreates the worker while
preserving the queue, shuffle and volume. Text labels use
`truncate()` for single-line content: wrapped ellipsis in GPUI 0.2.2 previously
caused a Unicode layout crash.

Audio preparation has one deadline for opening and decoding. A per-track token
cancels the underlying downloader even when its decoder is already running on
a blocking thread. A guard keeps that cancellation link alive for the source
lifetime and removes it when the source is dropped. Relative seeks are resolved
against the audio worker's position or its latest requested target, rather than a
UI snapshot. One blocking seek runs at a time; subsequent targets are coalesced.
Stop and Play cancel the old download without waiting for seek acknowledgement.
Late seek results are ignored by generation. Audio failures retain their causes
for diagnostics, with authenticated request URLs redacted and short UI messages.

Preferences writes are coalesced in a worker. Failed writes keep the pending value,
report a deduplicated error and retry after 1, 2, 5, 15 and then 30 seconds.
New values replace the pending value without resetting failure backoff; flush and
shutdown attempt an immediate save. A successful write clears the warning.
Before overwriting malformed preferences, the worker preserves the original bytes
in a unique `preferences.corrupt-*.json` file beside the preferences file. If the
backup cannot be written, the save fails and the original remains in place.
Preference warnings are rendered separately from library/playback failures.

## Verification boundaries

Unit tests cover domain behavior and local HTTP fixtures. Native tests cover real
GPUI layout, keyboard dispatch, restarts and macOS media command dispatchers.
Text steps wait for two stable completed paint frames with the intended input
focused before using the platform input handler. They reject unhandled input and
changes to any non-target field. The connection scenario exercises consecutive
Tab / text steps and verifies fixture input values without recording actual field
contents. System-theme notifications are simulated through the same resolver used
by the window observer; tests do not change the host OS theme. They
do not prove that every hardware media keyboard works, that every audio codec can
seek, or that untested platforms run successfully. Mouse controls also need the
manual checks documented in `scripts/README.md`.

Cyrillic and emoji in fixtures are intentional regression data, not untranslated
interface copy or real library records. Test credentials and hosts are synthetic.

## Runtime and dependency notes

Audio preparation has a 30-second deadline covering opening and decoding. A failed
or closed audio worker is recoverable without losing the queue, shuffle or volume.
Diagnostics record underlying audio errors with request URLs redacted. Panic logs
include a backtrace. Logs at least 5 MiB rotate to `still.previous.log` on startup;
packaging keeps a matching executable under `target/release/symbols/<UUID>/still`.

`NAVIDROME_URL`, `NAVIDROME_USERNAME` and `NAVIDROME_PASSWORD` can supply a connection
for automation. Such connections default to not being remembered. `STILL_CONFIG_DIR`
and `STILL_LOG_DIR` override preferences and diagnostics directories for isolated runs.
Preferences preserve malformed JSON before replacement and keep failed saves pending.

Folder trees are cached until refresh or disconnect. Recursive playback orders files
by filename, then visits child folders by name. Album and song lists load 200 records
per page. Metadata is displayed as supplied by Navidrome; fix incorrectly encoded tags
in the source library. `scripts/check-paths.py` provides read-only path diagnostics.

GPUI is pinned to published version 0.2.2. Its transitive `block` and
`proc-macro-error2` dependencies produce future-compatibility warnings. The `libc`
pin works around GPUI's old `xattr` dependency. Revisit these constraints after an
upstream GPUI update. Rust is pinned by `rust-toolchain.toml`; `Cargo.lock` records
dependency versions. Linux and Windows are not validated by the native UI suite.
