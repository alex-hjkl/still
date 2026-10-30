# Architecture

Still is a single desktop application. Domain models are kept independent of GPUI
where practical; views adapt user input and asynchronous results to those models.

- `browser.rs`, `folders.rs`: loaded records, filtered entries, navigation history,
  server folder tree, selection restoration and queue inputs.
- `models.rs`: shared artist, album and song records, library page size, sorting
  and duration labels. Domain modules do not import these records from the HTTP adapter.
- `api.rs`: Subsonic requests, response decoding and authentication. MD5 is used
  for the protocol's salted token, not as a general-purpose password hash.
- `queue.rs`, `likes.rs`: playback order and optimistic like reconciliation.
- `player.rs`: audio worker, cancelable buffering/decoding and watch-channel snapshots.
- `playback_controller.rs`: queue, playback policy, worker recovery and native media
  integration. Views read its state and send commands through its methods.
- `library_loader.rs`: pending background library task, cooperative cancellation
  and execution of library requests. The view reconciles completion events with
  current navigation.
- `collection_controller.rs`: background collection resolution, cancellation,
  failures and retry state. Playback changes and disconnect cancel pending work;
  a canceled result cannot start a new queue.
- `likes_controller.rs`: optimistic like writes, per-song tasks and retry state.
  Resetting the connection invalidates completions from the previous session.
- `media.rs`, `media_macos.rs`: native media commands and Now Playing metadata.
- `connection.rs`: connection controller owning authentication, credential persistence,
  operation identities and connection failures. It emits connection events for the view.
  `credentials.rs` adapts the system credential store.
- `settings.rs`, `diagnostics.rs`: background preferences persistence and diagnostics.
- `slider.rs`: reusable GPUI slider with pointer state and preview/change events.
- `ui.rs`: application view, navigation and coordination of controllers.
- `ui_components.rs`: shared buttons, list rows, notifications, icons and tooltips.
- `library_view.rs`, `sidebar.rs`: library lists, headers and sidebar rendering.
- `connection_screen.rs`, `folder_columns.rs`, `player_bar.rs`, `keyboard_help.rs`:
  screen rendering and view-specific interactions.
- `navigation.rs`: section ordering, persisted identities, names, icons and shortcuts.
- `shortcuts.rs`: key bindings and descriptions shared by dispatch, keyboard help,
  contextual hints and tooltips. `keyboard.rs` owns editing and Vim prefix rules;
  `input.rs` handles native Unicode/IME text input.
- `requests.rs`, `failure.rs`: stale-request cancellation and operation-specific Retry.
  Library, playback and each song's like have independent failures. Navigation clears
  only the library failure; connection failures belong to the connection controller.
- `ui_tests.rs`: opt-in native regression driver and paced demo captions, excluded
  from normal releases. `scripts/demo_fixture.py` supplies synthetic demo records
  and original covers; `scripts/demo_recording.py` captures the isolated window.
- `test_http.rs`: test-only scripted HTTP fixtures with expected paths, query
  parameters and explicit callbacks for cancellation. Waiting for a request is bounded.

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
Snapshots also carry the decoder's duration. If the server reports zero duration,
playback progress, seeks and native Now Playing use that duration; track changes
reset it. An unknown duration is displayed as `--:--` and disables the seek slider.

Song artwork is suppressed when the album is missing or is Navidrome's Unknown
Album placeholder. Even media-file cover IDs may fall back to shared album art,
which can belong to an unrelated untagged file. Both the folder preview and player
bar use the same artwork policy; identified albums retain the server's cover ID.

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
GPUI layout, keyboard dispatch, restarts and macOS media command dispatchers. They
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
dependency versions. Linux smoke checks cover desktop navigation, playback and
preferences; Windows is not validated by the native UI suite.
