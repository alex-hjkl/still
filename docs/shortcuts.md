# Keyboard and mouse

`Command` / `⌘` means Command on macOS, Super on Linux and Win on Windows.
`Ctrl` always means Control.

| Shortcut | Action |
| --- | --- |
| `j` / `k`, Up / Down | Select the next / previous record |
| `h` / `l` | Go back / open or play; in Folders, parent / selected child |
| Enter / Backspace | Open or play / go back |
| Shift+Enter | Play selected artist, album, folder or song |
| `gg` / `G`, Home / End | First / last record |
| Ctrl+D / Ctrl+U | Half a page down / up |
| Ctrl+F / Ctrl+B, PageDown / PageUp | Page down / up |
| `/`, Command+F | Focus library search |
| Enter / Escape in search | Focus the results, keeping the query |
| Escape in search results | Return to the previous page |
| Space | Play / pause; open the selected record if the queue is empty |
| `[` / `]` | Previous / next song; previous restarts after the first 3 seconds |
| Left / Right, `H` / `L` | Seek backward / forward 10 seconds |
| `-` / `+` (or `=`) | Decrease / increase volume by 5% |
| `s` | Toggle shuffle without restarting the current song |
| `f` / `F` | Toggle like for the current song; with an empty queue, `f` uses the selected song |
| Command+1 through Command+5 | Artists, Albums, Songs, Folders, Liked songs |
| Tab / Shift+Tab | Next / previous section |
| `r` / `e` | Refresh / load more records |
| `?`, F1 | Open / close keyboard help |
| Escape in help | Close help |
| `j` / `k`, Up / Down in help | Scroll keyboard help |
| Command+B | Switch compact / expanded sidebar |
| Command+Q | Quit, including while editing a field |

Artists and albums use parent, current and preview columns. Move with j / k to
update the preview; l enters the selection and h returns to its parent. Returning
restores the selection and scroll position. Clicking a row in a side column opens
it in that context. Albums opened from search keep the results as their parent;
albums opened from Albums keep the complete album list. Songs and Liked songs show
track details alongside the list. In library lists, Refresh (`r`) reloads all
currently loaded records and updates the preview, retaining the selected record
and scroll position. Other cached pages reload when revisited. In Folders,
Refresh rebuilds the folder tree and returns to its root.

Shift+Enter includes subfolders when playing a folder. With a song selected in
Folders, it plays the current folder and its subfolders. In an open album or
Liked songs, it starts the entire list from the first song.

Text fields retain normal text-editing behavior. Left / Right moves the caret
while search is focused. Section shortcuts and Command+B remain available there.
Playback shortcuts work while help is open; browsing commands do not act on the
covered list. With an empty queue, Space and `f` do not act on the covered selection.
Help also closes through Close or a click outside the panel.

The current song remains the target of `f` when paused or browsing another section.
To like another song during playback, click its heart in the list or preview,
including rows in the Folders columns. Its tooltip shows Saving… while the write
is pending. A failed write can be retried from its error message.

The header search field searches Navidrome for artists, albums and songs across
the entire library. Results update as you type. With the results focused, Enter
opens an artist or album, or plays a song; Shift+Enter plays the selected item.
Use `e` or Load more for further results. Clear the field to return to the previous
list and selection, keeping focus in search. Enter or Escape leaves the field;
Escape again returns to the previous page. Going back from an artist or album
restores the search results.

A highlighted row and blue outline identify the keyboard cursor.
The play/pause indicator identifies the current song independently. Clearing search
with the × button keeps focus in the field. Drag the timeline to preview a position
and release to seek. Drag volume for continuous adjustment; click the speaker to
mute and restore the previous level within the session.

The appearance button opens System, Latte and Macchiato choices. Use j / k,
Up / Down or Tab / Shift+Tab to move, Enter to apply and Esc to cancel. Closing
this menu restores the previously focused field.
