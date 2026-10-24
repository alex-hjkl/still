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
        stress = []
        for index in range(8):
            # No expectation or idle step between focus and text: the driver
            # must synchronize the painted input handler itself.
            stress.extend([key("tab"), key("cmd-a"), dict(action="text", field="username", text=f"focus-{index}"),
                           key("tab"), key("cmd-a"), dict(action="text", field="password", text="fixture"),
                           key("tab")])
        return [expect(theme_preference="system", system_theme_matches=True),
                dict(action="theme_menu"), expect(theme_menu=True, theme_menu_fits=True), key("escape"), expect(focus="server"),
                dict(action="set_theme", preference="latte"),
                dict(action="resize", size=[780, 640]),
                expect(connected=False, connection_fits=True, theme="latte", theme_bg=0xeff1f5),
                *stress, expect(server_unchanged=True, input_values={"username":"focus-7", "password":"fixture"}),
                dict(action="toggle_theme"), expect(theme="macchiato", theme_bg=0x24273a, connection_fits=True),
                key("tab"), expect(focus="username"), key("cmd-a"), dict(action="text", field="username", text="invalid"),
                expect(server_unchanged=True, input_values={"username":"invalid"}),
                key("tab"), expect(focus="password"), key("cmd-a"), dict(action="text", field="password", text="fixture"),
                key("enter"), expect(connected=False, failures=["retry-connection"], connection_fits=True),
                key("shift-tab"), key("cmd-a"), dict(action="text", field="username", text="malformed"), key("tab"),
                key("enter"), expect(connected=False, failures=["retry-connection"], connection_fits=False),
                dict(action="scroll_connection", delta=-10000),
                expect(connection_scrolled_to_bottom=True),
                dict(action="scroll_connection", delta=10000),
                expect(connection_scroll_y=0),
                dict(action="set_theme", preference="system"), expect(theme_preference="system", system_theme_matches=True),
                dict(action="quit")]

    restored = folder(cd2, "song-35", filter="035", preview_song="song-35",
                      columns=dict(parent=film, current=cd2))
    if launch == 1:
        return [restored, expect(theme="macchiato", theme_bg=0x24273a),
                dict(action="toggle_theme"), expect(theme="latte", theme_bg=0xeff1f5),
                expect(sidebar_width=208), key("cmd-b"), expect(sidebar_width=64, sidebar_icons=sidebar_icons()), key("escape"), key("h"), key("k"), key("l"),
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
                dict(action="stop_audio_worker"), expect(show_help=True, playback="failed", media_state="stopped", current_track="song-0", failures=["retry-playback"]),
                key("cmd-3"), expect(page="songs", loaded=True, failures=["retry-playback"]),
                dict(action="retry", operation="playback"), expect(playback="playing", media_state="playing", current_track="song-0", failures=[]),
                key("cmd-4"), expect(page="folders", path=cd1, loaded=True), key("f1"), expect(show_help=True),
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
                dict(action="set_theme", preference="system"), expect(theme_preference="system", system_theme_matches=True),
                dict(action="quit")]

    return [
        expect(theme="latte", theme_bg=0xeff1f5),
        dict(action="set_theme", preference="system"), expect(theme_preference="system", system_theme_matches=True),
        dict(action="system_appearance", dark=True), expect(theme="macchiato", theme_bg=0x24273a),
        dict(action="system_appearance", dark=False), expect(theme="latte", theme_bg=0xeff1f5),
        dict(action="set_theme", preference="latte"),
        dict(action="system_appearance", dark=True), expect(theme="latte", theme_preference="latte"),
        dict(action="toggle_theme"), expect(theme="macchiato", theme_bg=0x24273a),
        expect(connected=True, loaded=True, page="artists", sidebar_width=64, sidebar_icons=sidebar_icons()),
        dict(action="fail_connection_save"), expect(failures=["retry-connection"]),
        key("cmd-3"), expect(page="songs", loaded=False, failures=["retry-connection", "retry-library"]),
        dict(action="retry", operation="library"), expect(page="songs", loaded=True, failures=["retry-connection"]),
        dict(action="credential_store_available"), dict(action="retry", operation="connection"), expect(failures=[]),
        key("cmd-1"), expect(page="artists", loaded=True),
        key("cmd-b"), expect(sidebar_width=208, page="artists", sidebar_icons=sidebar_icons()),
        key("cmd-b"), expect(sidebar_width=64, sidebar_icons=sidebar_icons()),
        key("f1"), expect(show_help=True, page="artists", selected="composer", current_track=""),
        key("j"), key("enter"), key("h"), key("r"), key("space"),
        expect(show_help=True, page="artists", selected="composer", current_track=""),
        key("cmd-f"), expect(show_help=False, focus="filter"), key("escape"),
        expect(footer_hints=["j k Move", "Enter Open", "⇧ Enter Play artist", "/ Find"], footer_fits=True),
        key("shift-enter"), expect(page="artists", failures=["retry-collection"], current_track=""),
        dict(action="retry", operation="collection"),
        expect(page="artists", playback="playing", current_track="song-0", queue_len=120, queue_last="song-119", failures=[]),
        key("enter"), expect(page="artist", loaded=True),
        key("shift-enter"), expect(page="artist", playback="playing", current_track="song-0", queue_len=60, queue_last="song-59"),
        key("j"), key("shift-enter"), expect(page="artist", playback="playing", current_track="song-60", queue_len=60, queue_last="song-119"),
        key("space"), expect(playback="paused"), key("h"), expect(page="artists"),
        key("shift-enter"), key("cmd-3"), expect(page="songs", loaded=True),
        key("j"), key("enter"), expect(current_track="song-1", playback="playing"),
        dict(action="wait", milliseconds=800), expect(page="songs", current_track="song-1", queue_len=120),
        key("space"), expect(playback="paused"),
        key("cmd-4"), folder("", music, columns=dict(current="", preview=music)),
        expect(footer_hints=["j k Move", "Enter Open / play", "h Parent", "⇧ Enter Play folder", "/ Find"], footer_fits=True),
        key("l"), folder(music, collection, columns=dict(parent="", current=music, preview=collection)),
        key("l"), expect(path=collection, loaded=True),  # The original UTF-8 crash.
        key("j"), expect(selected=film),
        key("shift-enter"), expect(path=collection, current_track="song-0", queue_len=60, queue_last="song-59"),
        key("space"), expect(playback="paused"), key("l"), folder(film, cd1),
        key("shift-enter"), expect(page="folders", path=film, current_track="song-0", queue_len=30, queue_last="song-29"),
        key("space"), expect(playback="paused"),
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
        dict(action="theme_menu"), expect(theme_menu=True, theme_menu_fits=True, filter="035"),
        key("down"), key("enter"), expect(theme_menu=False, theme_preference="system", system_theme_matches=True, focus="filter", filter="035"),
        dict(action="theme_menu"), expect(theme_menu=True, theme_menu_fits=True),
        key("down"), key("enter"), expect(theme_menu=False, theme_preference="latte", theme="latte", focus="filter", filter="035"),
        dict(action="theme_menu"), key("down"), key("enter"),
        expect(theme_menu=False, theme_preference="macchiato", theme="macchiato", focus="filter", filter="035"),
        dict(action="theme_menu"), key("escape"), expect(theme_menu=False, focus="filter", filter="035"),
        dict(action="toggle_theme"), expect(theme="latte", focus="filter", filter="035", theme_bg=0xeff1f5),
        dict(action="toggle_theme"), expect(theme="macchiato", focus="filter", filter="035", theme_bg=0x24273a),
        dict(action="resize", size=[780, 640]), expect(size=[780, 640], sidebar_width=208, visible=True, sidebar_icons=sidebar_icons(640), footer_fits=True, header_stacked=True),
        key("cmd-b"), expect(sidebar_width=64, focus="filter", filter="035", sidebar_icons=sidebar_icons(640)),
        key("cmd-b"), expect(sidebar_width=208, focus="filter", filter="035", sidebar_icons=sidebar_icons(640)),
        dict(action="resize", size=[1120, 760]), expect(size=[1120, 760], sidebar_width=208, header_stacked=False),
        key("enter"), restored,
        key("f1"), expect(show_help=True, filter="035", visible=True, columns=dict(parent=film, current=cd2), preview_song="song-35"),
        key("escape"), expect(show_help=False, filter="035", selected="song-35", visible=True),
        key("h"), folder(film, cd2), key("l"), restored,
        key("cmd-3"), expect(page="songs", loaded=True), key("cmd-4"), restored,
        dict(action="quit"),
    ]
