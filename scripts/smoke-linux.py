#!/usr/bin/env python3
"""Exercise the real Linux GPUI window and audio worker using isolated synthetic data.

Requires a running Wayland/X11 desktop, Vulkan drivers and a working audio device.
Does not read saved logins or contact a real Navidrome server.
"""
import argparse
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlsplit
import uuid
import wave

from demo_fixture import cover, library

ROOT = Path(__file__).resolve().parents[1]


def scenario(launch, tracks):
    def key(value):
        return dict(action="key", key=value)

    def expect(**values):
        return dict(action="expect", **values)

    if launch == 1:
        return [expect(connected=True, loaded=True, page="songs", search="", shuffle=True,
                       volume=0.65, sidebar_width=208), dict(action="quit")]
    folder = "/Music/Коллекция 🎵/Очень длинное название альбома"
    return [
        expect(connected=True, loaded=True, page="artists"),
        key("l"), expect(page="artist", loaded=True),
        key("l"), expect(page="album", loaded=True),
        key("j"), expect(selected=tracks[1]["id"]),
        key("k"), expect(selected=tracks[0]["id"]),
        key("enter"), expect(playback="playing", current_track=tracks[0]["id"]),
        dict(action="wait", milliseconds=1800),
        key("space"), expect(playback="paused", audio_position_min=1),
        key("right"), dict(action="wait", milliseconds=200),
        expect(playback="paused", audio_position_min=10),
        key("left"), expect(playback="paused", audio_position_max=4),
        key("space"), expect(playback="playing"),
        key("]"), expect(playback="playing", current_track=tracks[1]["id"]),
        key("space"), expect(playback="paused"),
        key("cmd-4"), expect(page="folders", loaded=True, path=""),
        key("l"), expect(path="/Music"),
        key("G"), key("l"), expect(path="/Music/Коллекция 🎵"),
        key("l"), expect(path=folder, selected=tracks[0]["id"]),
        key("j"), expect(selected=tracks[1]["id"]),
        key("h"), expect(path="/Music/Коллекция 🎵"),
        key("l"), expect(path=folder, selected=tracks[1]["id"]),
        key("f1"), expect(show_help=True), key("escape"), expect(show_help=False),
        dict(action="resize", size=[780, 640]), expect(size=[780, 640]),
        dict(action="resize", size=[1120, 760]), expect(size=[1120, 760]),
        key("cmd-3"), expect(page="songs", loaded=True),
        key("/"), dict(action="text", text="Paper"),
        expect(page="search", loaded=True, search="Paper", search_counts=[0, 0, 1]),
        key("enter"), expect(selected=tracks[1]["id"]),
        key("cmd-b"), expect(sidebar_width=208),
        key("s"), expect(shuffle=True), key("-"), expect(volume=0.65),
        dict(action="quit"),
    ]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--backend", choices=["auto", "wayland", "x11"], default="auto")
    parser.add_argument("--no-build", action="store_true", help="Use an existing release build with ui-tests enabled")
    parser.add_argument("--timeout", type=int, default=120)
    parser.add_argument("--hold-seconds", type=int, default=0, help="Keep the first test window open for visual inspection")
    args = parser.parse_args()
    if sys.platform != "linux":
        parser.error("This check requires Linux")
    if not (os.environ.get("WAYLAND_DISPLAY") or os.environ.get("DISPLAY")):
        parser.error("A real Wayland/X11 desktop session is required")
    if args.backend == "wayland" and not os.environ.get("WAYLAND_DISPLAY"):
        parser.error("--backend wayland requires WAYLAND_DISPLAY")
    if args.backend == "x11" and not os.environ.get("DISPLAY"):
        parser.error("--backend x11 requires DISPLAY")
    if not args.no_build:
        subprocess.run(["cargo", "build", "--locked", "--release", "--features", "ui-tests"], cwd=ROOT, check=True)
    target = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target.is_absolute():
        target = ROOT / target
    binary = target / "release/still"
    albums, tracks = library()
    for track in tracks[:5]:
        track["path"] = f"/Music/Коллекция 🎵/Очень длинное название альбома/{track['track']:02} {track['title']}.wav"
    audio = io.BytesIO()
    with wave.open(audio, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(8000)
        wav.writeframes(b"\0\0" * 8000 * 180)
    image = cover(0)
    requests = []

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *unused):
            pass

        def do_GET(self):
            parsed = urlsplit(self.path)
            method = parsed.path.rsplit("/", 1)[-1].removesuffix(".view")
            params = parse_qs(parsed.query)
            item = params.get("id", [""])[0]
            requests.append(method)
            body = {"status": "ok"}
            if method == "getArtists":
                body["artists"] = {"index": [{"artist": [dict(id="artist-0", name="Mira Sol", albumCount=len(albums))]}]}
            elif method == "getArtist":
                body["artist"] = {"album": albums}
            elif method == "getAlbumList2":
                body["albumList2"] = {"album": albums}
            elif method == "getAlbum":
                album = next(album for album in albums if album["id"] == item)
                body["album"] = dict(album, song=[track for track in tracks if track["albumId"] == item])
            elif method == "search3":
                start = int(params.get("songOffset", ["0"])[0])
                count = int(params.get("songCount", ["200"])[0])
                query = params.get("query", [""])[0].strip().casefold()
                found = [track for track in tracks if query in
                         " ".join(track.get(field, "") for field in ["title", "artist", "album"]).casefold()]
                body["searchResult3"] = {"song": found[start:start + count]}
            elif method == "getStarred2":
                body["starred2"] = {"song": []}
            payload = audio.getvalue() if method == "stream" else image if method == "getCoverArt" else json.dumps({"subsonic-response": body}).encode()
            total = len(payload)
            byte_range = self.headers.get("Range") if method == "stream" else None
            if byte_range:
                start, _, end = byte_range.removeprefix("bytes=").partition("-")
                start, end = int(start or 0), min(int(end) if end else total - 1, total - 1)
                self.send_response(206)
                self.send_header("Content-Range", f"bytes {start}-{end}/{total}")
                payload = payload[start:end + 1]
            else:
                self.send_response(200)
            if method == "stream":
                self.send_header("Accept-Ranges", "bytes")
            self.send_header("Content-Type", "audio/wav" if method == "stream" else "image/png" if method == "getCoverArt" else "application/json")
            self.send_header("Content-Length", str(len(payload)))
            self.end_headers()
            try:
                self.wfile.write(payload)
            except (BrokenPipeError, ConnectionResetError):
                pass  # Playback may cancel a download on Next or quit.

    report = ROOT / ".local/ui-test-results" / f"linux-{args.backend}-{uuid.uuid4().hex}"
    report.mkdir(parents=True)
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    env = {key: value for key, value in os.environ.items() if not key.startswith(("NAVIDROME_", "STILL_UI_TEST_", "STILL_DEMO_"))}
    env.pop("ZED_HEADLESS", None)
    if args.backend == "x11":
        env.pop("WAYLAND_DISPLAY", None)
    elif args.backend == "wayland":
        env.pop("DISPLAY", None)
    env.update(NAVIDROME_URL=f"http://127.0.0.1:{server.server_port}",
               NAVIDROME_USERNAME="fixture", NAVIDROME_PASSWORD="fixture",
               STILL_CONFIG_DIR=str(report / "config"), STILL_LOG_DIR=str(report / "logs"))
    try:
        for launch in range(2):
            steps = scenario(launch, tracks)
            if launch == 0 and args.hold_seconds:
                steps.insert(-1, dict(action="wait", milliseconds=args.hold_seconds * 1000))
            script = report / f"ui-script-{launch}.json"
            result = report / f"ui-result-{launch}.json"
            script.write_text(json.dumps(steps, ensure_ascii=False), encoding="utf-8")
            env.update(STILL_UI_TEST_SCRIPT=str(script), STILL_UI_TEST_RESULT=str(result))
            with (report / f"app-{launch}.log").open("wb") as log:
                process = subprocess.Popen([str(binary)], cwd=ROOT, env=env, stdout=log, stderr=log)
                print(f"Linux {args.backend} launch {launch + 1}: PID {process.pid}; reports: {report}", flush=True)
                try:
                    status = process.wait(timeout=args.timeout + args.hold_seconds)
                finally:
                    if process.poll() is None:
                        process.terminate()
                        try:
                            process.wait(timeout=10)
                        except subprocess.TimeoutExpired:
                            process.kill()
                            process.wait()
            if status != 0:
                raise RuntimeError(f"App exited {status}; see {report}")
            result = json.loads(result.read_text(encoding="utf-8"))
            if not result["success"]:
                raise RuntimeError(result["error"])
            print(f"Launch {launch + 1}: {result['steps']} native UI steps passed", flush=True)
        diagnostic = report / "logs/still.log"
        if "PANIC:" in diagnostic.read_text(encoding="utf-8"):
            raise RuntimeError(f"App logged a panic; see {diagnostic}")
        if "stream" not in requests:
            raise RuntimeError("Playback did not request any audio")
        print(f"Linux UI, playback and preferences checks passed: {report}", flush=True)
    finally:
        (report / "requests.json").write_text(json.dumps(requests), encoding="utf-8")
        server.shutdown()
        server.server_close()


if __name__ == "__main__":
    main()
