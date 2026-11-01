"""Synthetic artist/album hierarchy and native column-navigation regression steps."""
from demo_fixture import library as demo_library


def library():
    albums, tracks = demo_library()
    artists = [dict(id=f"artist-{i}", name=name, albumCount=0)
               for i, name in enumerate(["Mira Sol", "North Relay", "Juniper Lane"])]
    extras = [("album-slow", "Slow preview", 2021),
              ("album-fail", "Retry preview", 2022),
              ("album-empty", "Empty album", 2023)]
    extras += [(f"extra-{i}", f"More music {i:02}", 2024) for i in range(28)]
    for item_id, name, year in extras:
        songs = [] if item_id == "album-empty" else [
            dict(id=f"{item_id}-song-{i}", title=f"{name} / Track {i:02}", artist="Mira Sol",
                 album=name, albumId=item_id, duration=180, track=i+1, coverArt=item_id,
                 path=f"/Music/Mira Sol/{name}/{i:02}.wav") for i in range(24)]
        albums.append(dict(id=item_id, name=name, artist="Mira Sol", year=year,
                           songCount=len(songs), coverArt=item_id))
        tracks.extend(songs)
    for i in range(24):
        name = f"Artist {i:02}"
        artists.append(dict(id=f"more-artist-{i}", name=name, albumCount=1))
        albums.append(dict(id=f"more-album-{i}", name=f"Album {i:02}", artist=name,
                           year=2025, songCount=0))
    for artist in artists:
        artist["albumCount"] = sum(a["artist"] == artist["name"] for a in albums)
    return artists, albums, tracks


def scenario():
    def key(value): return dict(action="key", key=value)
    def expect(**values): return dict(action="expect", **values)
    def columns(current, parent=None, preview=None):
        return dict(current=current, **({"parent": parent} if parent else {}),
                    **({"preview": preview} if preview else {}))
    def artist(value): return f'Artist("{value}")'
    def album(value): return f'Album("{value}")'
    def wait(milliseconds): return dict(action="wait", milliseconds=milliseconds)

    steps = [expect(page="artists", loaded=True, selected="artist-0", preview_loaded=True),
             expect(columns=columns("Artists", preview=artist("artist-0"))),
             key("l"), expect(page="artist", loaded=True, selected="album-0", preview_loaded=True),
             expect(columns=columns(artist("artist-0"), parent="Artists", preview=album("album-0"))),
             key("j"), expect(selected="album-2", preview_loaded=True),
             key("l"), expect(page="album", loaded=True, section="artists", selected="song-2-1", preview_song="song-2-1"),
             expect(columns=columns(album("album-2"), parent=artist("artist-0"))),
             key("j"), key("h"), expect(page="artist", selected="album-2", preview_loaded=True),
             key("l"), expect(page="album", selected="song-2-2", preview_song="song-2-2"),
             key("h"), key("j"), key("j"), expect(selected="album-slow"), wait(250),
             key("j"), expect(selected="album-fail", preview_failed=True, loaded=True),
             dict(action="retry", operation="preview"), expect(preview_loaded=True, preview_failed=False),
             wait(900), expect(selected="album-fail", preview_loaded=True, preview_failed=False),
             key("j"), expect(selected="album-empty", preview_loaded=True),
             key("l"), expect(page="album", loaded=True, cursor=0), key("h"),
             expect(page="artist", selected="album-empty"),
             key("G"), expect(visible=True, preview_loaded=True), key("l"),
             expect(page="album", loaded=True, preview_song="extra-27-song-0"),
             key("G"), expect(visible=True, selected="extra-27-song-23"),
             key("h"), expect(page="artist", visible=True, selected="extra-27"),
             key("l"), expect(page="album", visible=True, selected="extra-27-song-23"),
             key("h"), expect(page="artist", visible=True, selected="extra-27"),
             key("r"), expect(page="artist", loaded=True, preview_loaded=True, selected="extra-27", visible=True),
             key("l"), expect(page="album", loaded=True, selected="extra-27-song-23", search_counts=[0,0,25], visible=True),
             key("h"), expect(page="artist", selected="extra-27", visible=True),
             dict(action="activate_column", column="preview", row=23),
             expect(page="album", playback="playing", current_track="extra-27-song-23"),
             key("space"), expect(playback="paused"), key("h"),
             expect(page="artist", selected="extra-27", visible=True),
             key("h"), expect(page="artists", selected="artist-0"),
             key("G"), expect(visible=True, preview_loaded=True, selected="more-artist-23"),
             key("l"), expect(page="artist", loaded=True), key("h"),
             expect(page="artists", visible=True, selected="more-artist-23"),
             key("home"), expect(selected="artist-0", preview_loaded=True),
             dict(action="activate_column", column="preview", row=1),
             expect(page="album", selected="song-2-2", columns=columns(album("album-2"), parent=artist("artist-0"))),
             dict(action="activate_column", column="parent", row=0),
             expect(page="album", selected="song-0-1", columns=columns(album("album-0"), parent=artist("artist-0"))),
             key("cmd-2"), expect(page="albums", loaded=True, preview_loaded=True),
             key("j"), expect(selected="album-1", preview_loaded=False), wait(250),
             key("l"), expect(page="album", library_loading=True),
             expect(loaded=True, selected="song-1-1"), key("h"),
             key("j"), key("j"), key("j"), key("j"), expect(selected="album-5", preview_loaded=False), wait(250),
             key("l"), expect(page="album", loaded=False, failures=["retry-library"]),
             dict(action="retry", operation="library"), expect(loaded=True, selected="song-5-1", failures=[]),
             key("h"), key("home"), expect(page="albums", selected="album-0", preview_loaded=True),
             key("l"), expect(page="album", section="albums", columns=columns(album("album-0"), parent="Albums")),
             key("h"), expect(page="albums", selected="album-0"),
             key("/"), dict(action="text", field="search", text="Quiet Geometry"),
             expect(page="search", loaded=True, search_counts=[0, 1, 5], preview_loaded=True),
             key("escape"), key("l"), expect(page="album", loaded=True),
             expect(columns=columns(album("album-2"), parent='Search("Quiet Geometry")')),
             key("h"), expect(page="search", selected="album-2", search="Quiet Geometry"),
             key("/"), key("cmd-a"), key("backspace"), expect(page="albums", focus="search"),
             key("escape"), key("cmd-3"), expect(page="songs", loaded=True, preview_song="song-0-1"),
             key("j"), expect(preview_song="song-0-2"), key("enter"),
             expect(playback="playing", current_track="song-0-2"), key("space"), expect(playback="paused"),
             key("cmd-1"), expect(page="artists", loaded=True), key("l"), expect(page="artist", preview_loaded=True),
             dict(action="resize", size=[780, 640]), expect(size=[780, 640], visible=True),
             key("j"), expect(visible=True, preview_loaded=True), key("l"), expect(page="album", visible=True),
             dict(action="set_theme", preference="macchiato"), expect(theme="macchiato", visible=True),
             key("h"), expect(page="artist", selected="album-2", visible=True),
             key("f1"), expect(show_help=True), key("j"), key("l"),
             expect(show_help=True, page="artist", selected="album-2"), key("escape"),
             dict(action="disconnect"), expect(connected=False), dict(action="quit")]
    return steps


def verify_result(result, requests):
    for page, selected in [("artist", "extra-27"), ("album", "extra-27-song-23"),
                           ("artists", "more-artist-23")]:
        offsets = [s["list_scroll_y"] for s in result["snapshots"]
                   if s["page"] == page and s["selected"] == selected]
        if len(offsets) < 2 or offsets[0] >= 0 or any(abs(y - offsets[0]) > 0.1 for y in offsets):
            raise RuntimeError(f"Scroll was not restored for {page}/{selected}: {offsets}")
    for item_id, expected in [("album-1", 1), ("album-5", 2)]:
        observed = sum(r["method"] == "getAlbum" and r["id"] == item_id for r in requests)
        if observed != expected:
            raise RuntimeError(f"Preview transfer restarted {item_id}: {observed} requests, expected {expected}")
