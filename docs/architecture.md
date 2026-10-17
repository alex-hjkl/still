# Architecture

Still is a single desktop application. Domain models are kept independent of GPUI
where practical; views adapt user input and asynchronous results to those models.

- `browser.rs`, `folders.rs`: loaded records, filtered entries, navigation history,
  server folder tree, selection restoration and queue inputs.
- `api.rs`: Subsonic requests, response decoding and authentication. MD5 is used
  for the protocol's salted token, not as a general-purpose password hash.
- `queue.rs`, `likes.rs`: playback order and optimistic like reconciliation.
- `player.rs`: audio worker, stream buffering, decoding and playback snapshots.
- `media.rs`, `media_macos.rs`: native media commands and Now Playing metadata.
- `connection.rs`, `credentials.rs`: connection state and system credential storage.
- `settings.rs`, `diagnostics.rs`: background preferences persistence and diagnostics.
- `slider.rs`: reusable GPUI slider with pointer state and preview/change events.
- `ui.rs`: application view, event coordination and the library layout.
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
entity, and the audio worker publishes a playback snapshot. Text labels use
`truncate()` for single-line content: wrapped ellipsis in GPUI 0.2.2 previously
caused a Unicode layout crash.

## Verification boundaries

Unit tests cover domain behavior and local HTTP fixtures. Native tests cover real
GPUI layout, keyboard dispatch, restarts and macOS media command dispatchers. They
do not prove that every hardware media keyboard works, that every audio codec can
seek, or that untested platforms run successfully. Mouse controls also need the
manual checks documented in `scripts/README.md`.

Cyrillic and emoji in fixtures are intentional regression data, not untranslated
interface copy or real library records. Test credentials and hosts are synthetic.
