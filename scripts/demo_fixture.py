"""Fictional metal bands, original geometric covers and a native demo script."""
import math
import struct
import zlib


def library():
    records = [
        ("Cinderwake", "Ashen Veyr", 2008, ["Cinderwake", "Iron Beneath Ice", "The Hollow Kiln", "Salt of the Furnace", "Ash Without Dawn"]),
        ("The Gloam Engine", "Karnveil Forge", 2011, ["The Gloam Engine", "Blackened Orbit", "Graves of Glass", "A Crown of Static", "The Last Bellows"]),
        ("Thrones of Rust", "Ashen Veyr", 2013, ["Thrones of Rust", "Rivet Psalm", "When Iron Dreams", "The Ninth Foundry", "Teeth of the Sun"]),
        ("Nocturne for Stone", "Ordrake Hollow", 2015, ["Nocturne for Stone", "Below the Basalt", "A Wolf Made of Smoke", "Vault of Embers", "The Obsidian Vow"]),
        ("Wounds in the Anvil", "Ashen Veyr", 2018, ["Wounds in the Anvil", "Hammers at Dusk", "The Cinder Choir", "Furnaceheart", "Vow of the Unmade"]),
        ("Under a Sable Star", "Karnveil Forge", 2019, ["Under a Sable Star", "Chainbound Horizon", "The Silent Smelter", "Sable Meridian", "Engines of the Deep"]),
    ]
    albums, tracks = [], []
    for index, (name, artist, year, titles) in enumerate(records):
        album_id = f"album-{index}"
        albums.append(dict(id=album_id, name=name, artist=artist, year=year,
                           songCount=len(titles), coverArt=album_id))
        for number, title in enumerate(titles, 1):
            tracks.append(dict(id=f"song-{index}-{number}", title=title, artist=artist,
                               album=name, albumId=album_id, duration=180, track=number,
                               coverArt=album_id, path=f"/Music/{artist}/{name}/{number:02} {title}.wav"))
    return albums, tracks


def cover(index, edge=256):
    palettes = [((191, 77, 54), (22, 19, 28)), ((132, 163, 173), (19, 26, 40)),
                ((180, 141, 82), (32, 25, 28)), ((151, 113, 170), (24, 18, 39)),
                ((182, 187, 194), (30, 35, 48)), ((71, 133, 164), (12, 25, 42))]
    light, dark = palettes[index % len(palettes)]
    pixels = bytearray()
    for y in range(edge):
        pixels.append(0)  # PNG scanline: no filter.
        for x in range(edge):
            u, v = x / edge, y / edge
            distance = math.hypot(u - 0.5, v - 0.43)
            t = 0.15 + 0.55 * v
            if index % 3 == 0 and distance < 0.28:
                t = 0.08 if distance < 0.23 else 0.88
            elif index % 3 == 1 and v > 0.4 + 0.1 * math.sin(u * 8 + v * 5):
                t = 0.45 + 0.4 * v
            elif index % 3 == 2 and abs(u - 0.5) + abs(v - 0.43) < 0.35:
                t = 0.78 if u > 0.5 else 0.12
            if 0.84 < v < 0.85 and 0.18 < u < 0.82:
                t = 0.9
            pixels.extend(round(a * (1 - t) + b * t) for a, b in zip(light, dark))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">2I5B", edge, edge, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(pixels)) + chunk(b"IEND", b""))


def scenario():
    def key(value):
        return dict(action="key", key=value)

    def expect(**values):
        return dict(action="expect", **values)

    def wait(milliseconds=1300):
        return dict(action="wait", milliseconds=milliseconds)

    def hint(keys, label):
        return dict(action="demo_hint", keys=keys, label=label)

    def gesture(value, keys, label, pause=1800, **expected):
        return [hint(keys, label), key(value), expect(**expected), wait(pause)]

    albums, _ = library()
    artist_folder = f"/Music/{albums[0]['artist']}"
    first_folder = f"{artist_folder}/{albums[0]['name']}"
    next_folder = f"{artist_folder}/{albums[2]['name']}"

    # Prepare paused playback and a saved folder before recording the artist list.
    steps = [expect(loaded=True, page="albums"), key("enter"), expect(loaded=True, page="album"),
             key("enter"), expect(playback="playing", current_track="song-0-1"),
             dict(action="media", command="pause"), expect(playback="paused"),
             key("backspace"), expect(loaded=True, page="albums"),
             key("cmd-4"), expect(loaded=True, page="folders", path=""),
             key("l"), expect(path="/Music"), key("l"), expect(path=artist_folder, selected=first_folder),
             key("cmd-1"), expect(loaded=True, page="artists", selected="artist-0", preview_loaded=True), wait(800),
             dict(action="recording", start=True), hint("Still", "Your music. At your fingertips."), wait(3500)]
    steps += gesture("l", "l", "Artists, albums and songs in columns", page="artist", selected="album-0", preview_loaded=True)
    steps += gesture("j", "j", "Preview the next album", selected="album-2", preview_loaded=True)
    steps += gesture("l", "l", "Open the previewed album", page="album", selected="song-2-1")
    steps += gesture("h", "h", "Return without losing your place", page="artist", selected="album-2", preview_loaded=True)
    steps += gesture("cmd-2", "⌘2", "Browse all albums", page="albums", selected="album-0", preview_loaded=True)
    steps += gesture("j", "j", "Next album", selected="album-1")
    steps += gesture("enter", "↵", "Open album", loaded=True, page="album")
    for number in range(1, 3):
        steps += [hint("j" if number == 1 else f"j × {number}", "Move down through songs"),
                  key("j"), expect(selected=f"song-1-{number + 1}"), wait(400)]
    steps += [wait(1000)]
    steps += gesture("enter", "↵", "Play selected song", playback="playing", current_track="song-1-3")
    steps += gesture("]", "]", "Next song", pause=1700, playback="playing", current_track="song-1-4")
    steps += gesture("[", "[", "Previous song", pause=1700, playback="playing", current_track="song-1-3")
    steps += gesture("space", "Space", "Pause", playback="paused")
    for number in range(1, 3):
        steps += [hint("→" if number == 1 else f"→ × {number}", "Seek forward · 10 seconds per press"),
                  key("right"), expect(playback="paused", position_min=number * 10, position_max=number * 10 + 5), wait(500)]
    steps += [wait(1500)]
    steps += gesture("left", "←", "Seek back 10 seconds", playback="paused", position_min=10, position_max=15)
    steps += gesture("backspace", "⌫", "Back to albums", page="albums")
    steps += [hint("/", "Search your library"), key("/"), expect(focus="search")]
    for character in "cinder":
        steps += [dict(action="text", text=character), wait(100)]
    steps += [expect(page="search", search="cinder", search_counts=[0, 1, 6]), wait(2200)]
    steps += [hint("Esc × 2", "Return to the album list"),
              dict(action="key_burst", keys=["escape", "escape"]), expect(search="", focus=""), wait(1800)]
    steps += gesture("cmd-4", "⌘4", "Browse folders", loaded=True, page="folders", path=artist_folder)
    steps += gesture("l", "l", "Move into album", path=first_folder, selected="song-0-1")
    steps += gesture("enter", "↵", "Play from Folders", playback="playing", current_track="song-0-1")
    steps += gesture("f", "f", "Like a track in Folders", selected_liked=True, current_liked=True, current_like_pending=False)
    steps += gesture("h", "h", "Move to parent column", path=artist_folder)
    steps += gesture("j", "j", "Preview another album", selected=next_folder)
    steps += gesture("l", "l", "Open the previewed album", path=next_folder)
    steps += gesture("cmd-b", "⌘B", "Compact sidebar", sidebar_width=64)
    steps += gesture("cmd-b", "⌘B", "Expand sidebar", sidebar_width=208)
    steps += [hint("j ↵", "Choose the dark theme"), dict(action="theme_menu"),
              expect(theme_menu=True), wait(1000), key("j"), key("enter"),
              expect(theme="macchiato", theme_menu=False), wait(2200)]
    steps += gesture("cmd-2", "⌘2", "Back to albums", loaded=True, page="albums")
    steps += [hint("Still", "Keyboard first. Music always."), wait(3500),
              dict(action="recording", start=False), dict(action="quit")]
    return steps
