"""Deterministic native scenarios shared by the unattended smoke runner."""
from pathlib import PurePosixPath


def scenario(launch, tracks):
    def key(value):
        return dict(action="key", key=value)

    def expect(**values):
        return dict(action="expect", **values)

    def media(command, **values):
        return dict(action="media", command=command, **values)

    def sidebar_icons(height=760):
        # Actual painted centers must stay fixed when labels appear/disappear.
        centers = {"brand": [32, 32]}
        centers.update({name: [32, 96 + row * 48]
                        for row, name in enumerate(["artists", "albums", "songs", "folders", "heart"])})
        centers.update({"connection-status": [32, height - 202],
                        "keyboard-help": [32, height - 162], "disconnect": [32, height - 118]})
        return centers

    cd2 = str(PurePosixPath(tracks[30]["path"]).parent)
    film = str(PurePosixPath(cd2).parent)
    collection = str(PurePosixPath(film).parent)
    music = str(PurePosixPath(collection).parent)
    cd1 = str(PurePosixPath(tracks[0]["path"]).parent)

    def folder(path, selected, **values):
        return expect(connected=True, loaded=True, page="folders", path=path,
                      selected=selected, visible=True, **values)

    if launch == 2:
        return [expect(connected=False), key("tab"), expect(focus="username"),
                key("tab"), expect(focus="password"), dict(action="text", text="fixture"),
                dict(action="quit")]

    restored = folder(cd2, "song-35", filter="035", preview_song="song-35",
                      columns=dict(parent=film, current=cd2))
    if launch == 1:
        return [restored, expect(sidebar_width=208), key("cmd-b"), expect(sidebar_width=64, sidebar_icons=sidebar_icons()), key("escape"), key("h"), key("k"), key("l"),
                folder(cd1, "song-0", filter="", preview_song="song-0"),
                key("enter"), expect(current_track="song-0", playback="playing", media_state="playing", media_title=tracks[0]["title"]),
                media("pause"), expect(playback="paused", media_state="paused"),
                key("f1"), expect(show_help=True, playback="paused", columns=dict(parent=film, current=cd1), preview_song="song-0"),
                key("space"), expect(show_help=True, playback="playing"),
                key("space"), expect(show_help=True, playback="paused"),
                media("seek_to", seconds=12), expect(position_min=12, position_max=12),
                key("left"), expect(show_help=True, playback="paused", position_min=2, position_max=2),
                key("right"), expect(show_help=True, playback="paused", position_min=12, position_max=12),
                key("-"), expect(show_help=True, volume=0.65),
                key("+"), expect(show_help=True, volume=0.7),
                key("s"), expect(show_help=True, shuffle=True),
                key("s"), expect(show_help=True, shuffle=False),
                key("]"), expect(show_help=True, current_track="song-1", playback="playing"),
                media("seek_to", seconds=0), expect(current_track="song-1", position_max=1),
                key("["), expect(show_help=True, current_track="song-0", playback="playing"),
                key("space"), expect(show_help=True, playback="paused"),
                dict(action="stop_audio_worker"), expect(show_help=True, playback="failed", media_state="stopped", current_track="song-0"),
                key("space"), expect(show_help=True, playback="playing", media_state="playing", current_track="song-0"),
                key("space"), expect(show_help=True, playback="paused"),
                key("f1"), expect(show_help=False, playback="paused"),
                media("pause"), expect(playback="paused", media_state="paused"),
                media("play"), expect(playback="playing", media_state="playing"),
                media("play"), expect(playback="playing", media_state="playing"),
                media("next"), expect(current_track="song-1", playback="playing", media_title=tracks[1]["title"]),
                media("previous"), expect(current_track="song-0", playback="playing", media_title=tracks[0]["title"]),
                media("toggle"), expect(playback="paused", media_state="paused"),
                media("seek_to", seconds=12), expect(playback="paused", media_state="paused", position_min=12, position_max=12),
                media("seek_to", seconds=19), expect(playback="paused", position_min=19, position_max=19),
                dict(action="key_burst", keys=["left"] * 3), expect(playback="paused", position_max=0),
                media("seek_to", seconds=19), expect(playback="paused", position_min=19, position_max=19),
                dict(action="media_burst", commands=[dict(command="skip", seconds=-10)] * 3), expect(playback="paused", position_max=0),
                media("seek_to", seconds=12), expect(playback="paused", position_min=12, position_max=12),
                key("left"), folder(cd1, "song-0", playback="paused", position_min=2, position_max=2),
                key("right"), folder(cd1, "song-0", playback="paused", position_min=12, position_max=12),
                key("left"), expect(playback="paused", position_min=2, position_max=2),
                key("left"), expect(playback="paused", position_max=0),
                media("seek_to", seconds=12), expect(playback="paused", position_min=12, position_max=12),
                key("cmd-3"), expect(page="songs", loaded=True),
                key("left"), expect(page="songs", current_track="song-0", playback="paused", position_min=2, position_max=2),
                key("right"), expect(page="songs", current_track="song-0", playback="paused", position_min=12, position_max=12),
                key("cmd-f"), expect(focus="filter"),
                key("left"), key("right"), expect(focus="filter", playback="paused", position_min=12, position_max=12),
                key("escape"), key("cmd-4"), folder(cd1, "song-0"),
                media("skip", seconds=-10), expect(playback="paused", position_min=2, position_max=2),
                media("skip", seconds=10), expect(playback="paused", position_min=12, position_max=12),
                media("stop"), expect(playback="idle", media_state="stopped", position_max=0),
                media("play"), expect(playback="playing", media_state="playing", current_track="song-0"),
                media("stop"), expect(playback="idle", media_state="stopped"),
                key("space"), expect(playback="playing", media_state="playing"),
                key("space"), expect(playback="paused", media_state="paused"),
                dict(action="disconnect"), expect(connected=False, media_title="", media_state="stopped"),
                dict(action="quit")]

    return [
        expect(connected=True, loaded=True, page="artists", sidebar_width=64, sidebar_icons=sidebar_icons()),
        key("cmd-b"), expect(sidebar_width=208, page="artists", sidebar_icons=sidebar_icons()),
        key("cmd-b"), expect(sidebar_width=64, sidebar_icons=sidebar_icons()),
        key("f1"), expect(show_help=True, page="artists", selected="composer", current_track=""),
        key("j"), key("enter"), key("h"), key("r"), key("space"),
        expect(show_help=True, page="artists", selected="composer", current_track=""),
        key("cmd-f"), expect(show_help=False, focus="filter"), key("escape"),
        key("cmd-4"), folder("", music, columns=dict(current="", preview=music)),
        key("l"), folder(music, collection, columns=dict(parent="", current=music, preview=collection)),
        key("l"), expect(path=collection, loaded=True),  # The original UTF-8 crash.
        key("j"), expect(selected=film), key("l"), folder(film, cd1),
        key("j"), expect(selected=cd2), key("l"), folder(cd2, "song-30"),
        key("j"), expect(selected="song-31"), key("j"), expect(selected="song-32"),
        key("k"), expect(selected="song-31"), key("h"), folder(film, cd2),
        key("l"), folder(cd2, "song-31"), key("G"), folder(cd2, "song-59"),
        key("pageup"), expect(visible=True),
        dict(action="resize", size=[780, 640]), expect(size=[780, 640]),
        key("pagedown"), expect(visible=True), key("pageup"), expect(visible=True),
        dict(action="resize", size=[1120, 760]), expect(size=[1120, 760]),
        key("/"), expect(focus="filter"), dict(action="text", text="035"), key("cmd-b"),
        expect(sidebar_width=208, focus="filter", filter="035", find_summary="1 of 30 loaded items"),
        dict(action="resize", size=[780, 640]), expect(size=[780, 640], sidebar_width=208, visible=True, sidebar_icons=sidebar_icons(640)),
        key("cmd-b"), expect(sidebar_width=64, focus="filter", filter="035", sidebar_icons=sidebar_icons(640)),
        key("cmd-b"), expect(sidebar_width=208, focus="filter", filter="035", sidebar_icons=sidebar_icons(640)),
        dict(action="resize", size=[1120, 760]), expect(size=[1120, 760], sidebar_width=208),
        key("enter"), restored,
        key("f1"), expect(show_help=True, filter="035", visible=True, columns=dict(parent=film, current=cd2), preview_song="song-35"),
        key("escape"), expect(show_help=False, filter="035", selected="song-35", visible=True),
        key("h"), folder(film, cd2), key("l"), restored,
        key("cmd-3"), expect(page="songs", loaded=True), key("cmd-4"), restored,
        dict(action="quit"),
    ]
