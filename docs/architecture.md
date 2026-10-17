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
- `connection.rs`, `credentials.rs`: connection state and system credential storage.
- `settings.rs`, `diagnostics.rs`: background preferences persistence and diagnostics.
- `slider.rs`: reusable GPUI slider with pointer state and preview/change events.
- `ui.rs`: application view, navigation and coordination of controllers.
- `library_view.rs`, `sidebar.rs`: library lists, headers and sidebar rendering.
- `connection_screen.rs`, `folder_columns.rs`, `player_bar.rs`, `keyboard_help.rs`:
  screen rendering and view-specific interactions.
- `keyboard.rs`, `input.rs`: contextual commands and native Unicode/IME text input.
- `requests.rs`, `failure.rs`: stale-request cancellation and operation-specific Retry.
- `ui_tests.rs`: opt-in native regression driver, excluded from normal releases.

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
GPUI layout, keyboard dispatch, restarts and macOS media command dispatchers. They
do not prove that every hardware media keyboard works, that every audio codec can
seek, or that untested platforms run successfully. Mouse controls also need the
manual checks documented in `scripts/README.md`.

Cyrillic and emoji in fixtures are intentional regression data, not untranslated
interface copy or real library records. Test credentials and hosts are synthetic.
