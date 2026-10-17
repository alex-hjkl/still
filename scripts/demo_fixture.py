"""Synthetic music metadata, original geometric covers and a native demo script."""
import math
import struct
import zlib


def library():
    records = [
        ("First Light", "Mira Sol", 2008, ["First Light", "Paper Skies", "A Place to Begin", "Slow Motion", "Home Again"]),
        ("Atlas of Rain", "North Relay", 2011, ["Rain on Glass", "Night Train", "Small Hours", "Northern Lines", "Coming Home"]),
        ("Quiet Geometry", "Mira Sol", 2013, ["Parallel", "Soft Edges", "Still Life", "Between the Lines", "Circles"]),
        ("Soft Focus", "Juniper Lane", 2015, ["Golden", "Little Things", "Sunday Morning", "Soft Focus", "Stay Awhile"]),
        ("After Hours", "Mira Sol", 2018, ["Blue Hour", "Empty Streets", "After Hours", "City Lights", "Daybreak"]),
        ("Open Water", "North Relay", 2019, ["The Crossing", "Open Water", "Undertow", "Far Shore", "Low Tide"]),
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
    palettes = [((228, 192, 133), (125, 77, 51)), ((128, 166, 172), (32, 62, 79)),
                ((178, 191, 143), (67, 90, 71)), ((214, 173, 159), (120, 73, 87)),
                ((107, 124, 157), (34, 43, 75)), ((120, 169, 164), (35, 83, 91))]
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

    def gesture(value, keys, label, pause=2200, **expected):
        return [hint(keys, label), key(value), expect(**expected), wait(pause)]

    # Prepare a paused track and the complete album list before starting capture.
    steps = [expect(loaded=True, page="albums"), key("enter"), expect(loaded=True, page="album"),
             key("enter"), expect(playback="playing", current_track="song-0-1"),
             dict(action="media", command="pause"), expect(playback="paused"),
             key("backspace"), expect(loaded=True, page="albums"),
             key("cmd-4"), expect(loaded=True, page="folders", path=""),
             key("l"), expect(path="/Music"), key("j"), key("l"), expect(path="/Music/Mira Sol"),
             key("j"), expect(selected="/Music/Mira Sol/First Light"),
             key("cmd-2"), expect(loaded=True, page="albums"), wait(800),
             dict(action="recording", start=True), hint("Still", "Your music. At your fingertips."), wait(3500)]
    steps += gesture("j", "j", "Next album", selected="album-1")
    steps += gesture("enter", "↵", "Open album", pause=3000, loaded=True, page="album")
    for number in range(1, 5):
        steps += [hint("j" if number == 1 else f"j × {number}", "Move down through songs"),
                  key("j"), expect(selected=f"song-1-{number + 1}"), wait(400)]
    steps += [wait(1200)]
    for number in range(1, 3):
        steps += [hint("k" if number == 1 else f"k × {number}", "Move back up"),
                  key("k"), expect(selected=f"song-1-{5 - number}"), wait(400)]
    steps += [wait(1000)]
    steps += gesture("enter", "↵", "Play selected song", playback="playing", current_track="song-1-3")
    steps += gesture("]", "]", "Next song", pause=1700, playback="playing", current_track="song-1-4")
    steps += gesture("]", "]", "Next song", pause=1700, playback="playing", current_track="song-1-5")
    steps += gesture("[", "[", "Previous song", pause=1700, playback="playing", current_track="song-1-4")
    steps += gesture("space", "Space", "Pause", playback="paused")
    for number in range(1, 7):
        steps += [hint("→" if number == 1 else f"→ × {number}", "Seek forward · 10 seconds per press"),
                  key("right"), expect(playback="paused", position_min=number * 10, position_max=number * 10 + 5), wait(500)]
    steps += [wait(2500)]
    for number in range(1, 3):
        target = (6 - number) * 10
        steps += [hint("←" if number == 1 else f"← × {number}", "Seek back · 10 seconds per press"),
                  key("left"), expect(playback="paused", position_min=target, position_max=target + 5), wait(650)]
    steps += [wait(3000)]
    steps += gesture("backspace", "⌫", "Back to albums", pause=2800, page="albums")
    steps += [hint("/", "Find in this list"), key("/"), expect(focus="filter")]
    for character in "light":
        steps += [dict(action="text", text=character), wait(100)]
    steps += [expect(filter="light", find_summary="1 of 6 loaded albums"), wait(2800)]
    steps += [hint("Esc × 2", "Return to the list and clear Find"),
              dict(action="key_burst", keys=["escape", "escape"]), expect(filter="", focus=""), wait(2800)]
    steps += gesture("cmd-4", "⌘4", "Browse folders", pause=3000, loaded=True, page="folders", path="/Music/Mira Sol")
    steps += gesture("l", "l", "Move into album", pause=2500, path="/Music/Mira Sol/First Light")
    steps += gesture("h", "h", "Move to parent column", pause=2500, path="/Music/Mira Sol")
    steps += gesture("j", "j", "Preview another album", pause=1800, selected="/Music/Mira Sol/Quiet Geometry")
    steps += gesture("l", "l", "Open the previewed album", pause=2500, path="/Music/Mira Sol/Quiet Geometry")
    steps += gesture("cmd-b", "⌘B", "Compact sidebar", pause=2800, sidebar_width=64)
    steps += gesture("cmd-b", "⌘B", "Expand sidebar", pause=2800, sidebar_width=208)
    steps += gesture("cmd-2", "⌘2", "Back to albums", pause=2800, loaded=True, page="albums")
    steps += [hint("Still", "Keyboard first. Music always."), wait(3500),
              dict(action="recording", start=False), dict(action="quit")]
    return steps
