# Architecture

Still is a single desktop application. Domain models are kept independent of GPUI
where practical; views adapt user input and asynchronous results to those models.

- `main.rs`: application startup, bundled fonts, initial theme, window creation
  and the native quit action.
- `browser.rs`, `folders.rs`: loaded records, visible entries, navigation history,
  server folder tree, selection restoration and queue inputs. Browser does not own
  active requests or cancellation. Each page holds `Option<PageData>`: artists,
  albums, songs, mixed search results or a cached folder marker. `loaded()` derives
  from that option and a freshness flag, distinguishing an empty loaded page from
  an unloaded or invalidated page. Refresh retains old rows and selection IDs,
  reloads the previously loaded extent and restores selection by record identity.
  LibraryPage carries the last server response's pagination flag when a refresh
  combines multiple pages.
  The ancestor history supplies the visible parent, preserving the actual entry
  route (including search). A separate preview PageState is promoted on entry.
  Up to 32 recently loaded pages retain child selections and share records on Arc;
  disconnect clears the cache. PageId separates stable identity from display metadata.
- `models.rs`: shared artist, album and song records, library page size, sorting
  and time labels. Unknown total durations use `--:--` consistently in lists,
  the folder preview and player bar; a zero playback position still uses `0:00`.
  Domain modules do not import these records from the HTTP adapter.
- `api.rs`: Subsonic requests, response decoding and authentication. MD5 is used
  for the protocol's salted token, not as a general-purpose password hash.
- `queue.rs`, `likes.rs`: playback order and optimistic like reconciliation.
- `player.rs`: audio worker, cancelable buffering/decoding and watch-channel snapshots.
- `playback_controller.rs`: queue, playback policy, worker recovery and native media
  integration. Views read its state and send commands through its methods.
- `library_controller.rs`: owns Browser and the library loader, coordinating
  navigation, cached pages, section selection, folder restoration, search changes,
  pagination and library-specific failures and retries. Its Browser accessor is
  read-only; selection and navigation changes use explicit controller methods.
- `library_loader.rs`: owns one library request, task, cancellation
  and loading status. It rejects outdated and duplicate completions before emitting
  an event. LibraryController cancels it on navigation and applies accepted results.
  The view reconciles likes and playback selection and handles focus and scrolling.
  A second LibraryLoader handles previews, starting after 150 ms. Entering a child
  whose request is pending swaps the loader roles; subscribers resolve those roles
  at delivery, so no request restarts. Other route/selection changes cancel stale
  work. Preview failures and Retry stay in that column and do not block the active
  list. Refresh invalidates cached data, retaining positions, and starts the preview
  after the current page has refreshed. Cached fresh previews open without another request.
- `collection.rs`: resolves an artist or album into an ordered song collection
  without changing the current browser page.
- `collection_controller.rs`: background collection resolution, cancellation,
  failures and retry state. Playback changes and disconnect cancel pending work;
  a canceled result cannot start a new queue. Completion checks request identity
  once, stores the queue or failure and returns the event to emit; outdated or
  duplicate completions emit nothing.
- `likes_controller.rs`: optimistic like writes, per-song tasks and retry state.
  Actions require an explicit Session; Disconnected cannot act as a local demo.
  Resetting the connection invalidates completions from the previous generation.
- `media.rs`, `media_macos.rs`: native media commands and Now Playing metadata.
- `connection.rs`: connection controller owning authentication, credential persistence,
  operation identities, Session and connection failures. It emits connection events for the view.
  `credentials.rs` adapts the system credential store.
- `session.rs`: explicit Disconnected / Demo / Connected modes. It supplies the
  optional network client, library identity and connection label. The application
  view does not maintain an independent demo flag. Session deliberately has no Debug
  implementation because its connected client holds authentication material.
- `theme.rs`: Catppuccin Latte / Macchiato palettes and a separate saved
  `ThemePreference` (System / Latte / Macchiato). New preferences follow the OS;
  existing explicit palette choices are preserved. The active GPUI global contains
  only the resolved palette. System mode observes window appearance changes;
  manual modes ignore them. Appearance callbacks read the current platform value
  on the next frame, after platform event borrows are released, rather than GPUI’s
  cached window value; activation also rechecks background changes.
  On macOS, System clears the native AppKit appearance
  override while manual choices set Aqua / Dark Aqua. Refreshes include independent
  inputs, sliders and tooltips.
- `settings.rs`, `diagnostics.rs`: background preferences persistence and diagnostics.
- `slider.rs`: reusable GPUI slider with pointer state and preview/change events.
- `ui.rs`: application view and coordination of controllers and visual state.
  Construction creates the view, attaches its subscriptions, then initializes
  playback controls, polling and the initial connection or demo page.
- `ui_components.rs`: shared buttons, rows, notifications, icons, tooltips and empty states.
  Live tooltips subscribe to controller events, so a pending like's label updates
  even while the pointer stays over the button. `ui.rs` owns the shared like button
  used by library rows, folder columns, track details and the player.
- `icons.rs`: embedded SVG asset lookup for Phosphor icons and the Still mark.
  `artwork.rs` loads and caches covers, crops nonsquare images to a centered square
  before GPUI painting, and renders placeholders and the themed app mark.
- `demo.rs`: pure local demo records and loading without GPUI or a Navidrome server.
- `theme_picker.rs`: mouse and keyboard appearance selection, restoring the
  previous focus when its menu closes.
- `library_view.rs`, `sidebar.rs`: library header and sidebar rendering.
- `columns.rs`: shared parent/current/preview layout and column headers.
  `library_columns.rs` adapts artists, albums, songs and mixed search results to
  a typed LibraryRow model and one row renderer, with independently retained scroll
  handles keyed by PageId. Navigation changes schedule one deferred reconciliation
  of the final state per GPUI effect cycle; transient intermediate pages do not
  start previews or reset another page's scroll handle.
  Parent and preview rows use the same navigation/playback actions as the active
  list. `track_details.rs` renders the shared song details for all browsing modes.
- `search_view.rs`: library-wide search input, debounce and mixed artist, album
  and song results, using the same navigation and playback actions as library lists.
- `connection_screen.rs`, `folder_columns.rs`, `player_bar.rs`, `keyboard_help.rs`:
  screen rendering and view-specific interactions. `status_bar.rs` renders
  contextual shortcuts, collection loading status and Load more.
- `layout.rs`: shared row, player and sidebar dimensions, plus keyboard page sizing.
- `navigation.rs`: section ordering, persisted identities, names, icons and shortcuts.
- `shortcuts.rs`: key bindings and descriptions shared by dispatch, keyboard help,
  contextual hints and tooltips. `keyboard.rs` owns editing, help availability and
  Vim prefix rules; `input.rs` handles native Unicode/IME text input.
  Player commands target the playback queue independently of browser selection.
  With an empty queue, Space and `f` fall back to the visible selection, except under help.
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

Library search uses [search3](https://opensubsonic.netlify.app/docs/endpoints/search3/)
with a nonempty query and up to 200 results per category. Editing waits 250 ms
before loading, invalidates the previous request immediately and clears its results;
Enter submits without waiting. Empty input makes no server request. Further pages
advance all three offsets by the largest loaded category, preserving the selected
entry as categories grow. Navigation history retains the query and loaded results.
One header field handles all search. Typing starts a search from the current page;
clearing returns to that page and selection while retaining input focus. `/` and
Command+F focus the same field. Search does not change the saved last section.
Legacy saved folder filters are ignored when reading preferences and are never
written back. Song rows use source indices and share their song collection with
playback; directory, artist and album rows do not enter the queue.
PageData keeps shared collections on Arc, so cached history and active playback
queues retain their records when another page is loaded or pagination appends data.

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
The seek slider is synchronized at initialization, disconnect and playback updates;
the volume slider is synchronized at initialization and volume changes. Rendering
the player bar only reads state. All track changes share one handler: explicit
local playback actions dismiss collection failures, while transitions observed
by polling preserve them so Retry remains available.

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
Refreshing Folders discards the tree and its saved directory selections and reloads
the root; library-list refresh instead retains the current list's loaded extent.

GPUI is pinned to published version 0.2.2. Its transitive `block` and
`proc-macro-error2` dependencies produce future-compatibility warnings. The `libc`
pin works around GPUI's old `xattr` dependency. Revisit these constraints after an
upstream GPUI update. Rust is pinned by `rust-toolchain.toml`; `Cargo.lock` records
dependency versions. Linux smoke checks cover desktop navigation, playback and
preferences; Windows is not validated by the native UI suite.

The `libc` pin also prevents `stream-download` 0.24.5: its new `parking_lot` 0.13
dependency requires `libc` 0.2.191 or newer. Keep 0.24.4 until the GPUI / `xattr`
constraint can be removed.

The RustSec CI audit fails on vulnerabilities and unsoundness advisories, and
reports maintenance advisories without suppressing them. GPUI still brings
several unmaintained dependencies; reassess these when upgrading the framework.
RustSec is not an exhaustive list of security issues. A separate OSV review on
2026-11-01 found
[`GHSA-38c5-483c-4qqp`](https://github.com/becheran/grid/security/advisories/GHSA-38c5-483c-4qqp)
in `grid` 0.18 through GPUI's pinned `taffy` 0.9.0. The issue requires calling
`Grid::expand_rows`; Taffy's source does not call it, and Still uses flex layouts.
Leave the dependency unchanged for now and reassess this finding when upgrading
GPUI / Taffy or introducing direct `grid` use. No audit exception is configured.
