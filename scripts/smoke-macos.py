#!/usr/bin/env python3
"""Run an isolated native smoke scenario against a local Navidrome fixture.

Manual runs follow scripts/README.md and end with Cmd+Q. With --automated, a
test-only driver dispatches GPUI keystrokes and checks rendered UI state.
Each run has its own app ID, preferences and diagnostic logs.
"""
import argparse
import io
import json
import os
from pathlib import Path
import plistlib
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlsplit
import uuid
import wave


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=Path, default=Path(__file__).resolve().parents[1] / "target/release/Still.app")
    parser.add_argument("--case", choices=["library", "filter", "connection", "folders", "unicode"], default="library")
    parser.add_argument("--relaunch", action="store_true", help="Reopen once to check saved preferences")
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument("--automated", action="store_true", help="Run native UI assertions, restart restoration, and connection form checks without manual input")
    mode.add_argument("--record-demo", type=Path, metavar="DIRECTORY", help="Record a synthetic keyboard demo as GIF and MP4")
    parser.add_argument("--connection-only", action="store_true", help="With --automated, run only the connection / input scenario")
    parser.add_argument("--repeat", type=int, choices=range(1, 11), default=1, help="Number of isolated connection launches with --connection-only")
    parser.add_argument("--timeout", type=int, default=300)
    parser.add_argument("--size", type=int, nargs=2, metavar=("WIDTH", "HEIGHT"), help="Initial isolated window size")
    args = parser.parse_args()
    if (args.connection_only or args.repeat != 1) and not args.automated:
        parser.error("Connection stress checks require --automated")
    if args.repeat != 1 and not args.connection_only:
        parser.error("--repeat requires --connection-only")
    if sys.platform != "darwin":
        parser.error("This native smoke check requires macOS")
    if not args.app.is_dir():
        parser.error("App bundle not found; run scripts/build-macos.sh first")
    test_binary = None
    scripted = args.automated or args.record_demo is not None
    if scripted:
        project = Path(__file__).resolve().parents[1]
        subprocess.run(["cargo", "build", "--locked", "--release", "--features", "ui-tests"], cwd=project, check=True)
        artifacts = Path(os.environ.get("CARGO_TARGET_DIR", project / "target"))
        if not artifacts.is_absolute():
            artifacts = project / artifacts
        test_binary = artifacts / "release/still"
        if args.automated:
            args.case = "unicode"
            args.relaunch = True
        else:
            args.size = [1120, 760]

    albums = [dict(id=f"album-{i}", name=f"Album {i:03}", artist="Composers", year=2000 + i % 25, songCount=120) for i in range(401)]
    tracks = [dict(id=f"song-{i}", title=f"Track {i:03}", artist="Composer", album="Soundtrack", duration=30,
                   path=f"/music/Film/{i:03} Theme.wav") for i in range(120)]
    if args.case in ("folders", "unicode"):
        directories = ["Collections/Film Alpha/CD 1", "Collections/Film Alpha/CD 2",
                       "Collections/Film Beta", "Jazz"]
        if args.case == "unicode":
            collection = "Коллекция саундтреков к фильмам и сериалам — очень длинное название каталога"
            film = "Музыка из замечательного фильма — расширенное издание 🎵"
            directories = [f"{collection}/{film}/CD 1", f"{collection}/{film}/CD 2",
                           f"{collection}/Другой фильм", "Электронная музыка"]
        for i, track in enumerate(tracks):
            track["path"] = f"/music/{directories[i // 30]}/{i:03} Theme.wav"
            if args.case == "unicode":
                track.update(title=f"Очень длинное название музыкальной композиции {i:03} 🎵",
                             artist="Композитор саундтрека к замечательному фильму",
                             album="Музыка из фильма — полное собрание")
                track["path"] = f"/music/{directories[i // 30]}/Очень длинное название композиции {i:03} 🎵.wav"
    covers = {}
    if args.record_demo is not None:
        from demo_fixture import cover, library
        albums, tracks = library()
        covers = {album["id"]: cover(index) for index, album in enumerate(albums)}
    liked = {"song-2"}
    fail_more = True
    fail_songs = args.automated
    fail_artist = args.automated
    methods = []
    audio = io.BytesIO()
    with wave.open(audio, "wb") as wav:
        wav.setnchannels(1)
        wav.setsampwidth(2)
        wav.setframerate(8000)
        wav.writeframes(b"\0\0" * 8000 * (180 if args.record_demo is not None else 30))

    class Handler(BaseHTTPRequestHandler):
        def log_message(self, *unused):
            pass

        def do_GET(self):
            nonlocal fail_more, fail_songs, fail_artist
            url = urlsplit(self.path)
            method = url.path.rsplit("/", 1)[-1].removesuffix(".view")
            methods.append(method)
            params = parse_qs(url.query)
            item_id = params.get("id", [""])[0]
            offset = int(params.get("offset", params.get("songOffset", ["0"]))[0])
            print(json.dumps(dict(method=method, id=item_id, offset=offset)), flush=True)
            body = {"status": "ok"}
            if method == "ping" and params.get("u") == ["invalid"]:
                body = {"status": "failed", "error": {"code": 40, "message": "Wrong username or password."}}
            elif method == "ping" and params.get("u") == ["malformed"]:
                body = {"status": "Unexpected response from the server. " * 40}
            elif method == "getArtists":
                artists = [dict(id="composer", name="Composers", albumCount=401)]
                if args.record_demo is not None:
                    artists = [dict(id=f"artist-{index}", name=name, albumCount=sum(a["artist"] == name for a in albums))
                               for index, name in enumerate(sorted({album["artist"] for album in albums}))]
                body["artists"] = {"index": [{"artist": artists}]}
            elif method == "getArtist":
                if fail_artist:
                    fail_artist = False
                    body = {"status": "failed", "error": {"code": 50}}
                else:
                    if args.automated:
                        time.sleep(0.5)
                    body["artist"] = {"album": albums[:2] if args.automated else albums}
            elif method == "getAlbumList2":
                if offset == 200 and fail_more:
                    fail_more = False
                    body = {"status": "failed", "error": {"code": 50}}
                else:
                    ordered = albums
                    if params.get("type") == ["byYear"]:
                        first = int(params["fromYear"][0])
                        last = int(params["toYear"][0])
                        ordered = sorted((album for album in albums if min(first, last) <= album["year"] <= max(first, last)),
                                         key=lambda album: (album["year"], album["name"]), reverse=first > last)
                    body["albumList2"] = {"album": ordered[offset:offset + 200]}
            elif method == "getAlbum":
                album = next((album for album in albums if album["id"] == item_id), albums[0])
                songs = [track for track in tracks if track["albumId"] == album["id"]] if args.record_demo is not None else tracks
                if args.automated and album["id"] in {"album-0", "album-1"}:
                    start = 0 if album["id"] == "album-0" else 60
                    songs = tracks[start:start + 60]
                body["album"] = dict(album, song=songs)
            elif method == "search3":
                size = int(params.get("songCount", ["200"])[0])
                if fail_songs and size == 200:
                    fail_songs = False
                    body = {"status": "failed", "error": {"code": 50}}
                else:
                    body["searchResult3"] = {"song": tracks[offset:offset + size]}
            elif method == "getStarred2":
                body["starred2"] = {"song": [dict(track, starred="2026-10-16T00:00:00Z") for track in tracks if track["id"] in liked]}
            elif method == "star":
                liked.add(item_id)
            elif method == "unstar":
                liked.discard(item_id)
            payload = (audio.getvalue() if method == "stream" else covers.get(item_id, b"") if method == "getCoverArt"
                       else json.dumps({"subsonic-response": body}).encode())
            total = len(payload)
            byte_range = self.headers.get("Range") if method == "stream" else None
            if byte_range:
                start_text, _, end_text = byte_range.removeprefix("bytes=").partition("-")
                start = int(start_text or "0")
                end = min(int(end_text) if end_text else total - 1, total - 1)
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
            self.wfile.write(payload)

    with tempfile.TemporaryDirectory(prefix="still-smoke-") as directory:
        root = Path(directory)
        bundle = root / "Still Smoke.app"
        shutil.copytree(args.app, bundle)
        if test_binary is not None:
            shutil.copy2(test_binary, bundle / "Contents/MacOS/still")
        info_path = bundle / "Contents/Info.plist"
        info = plistlib.loads(info_path.read_bytes())
        info.update(CFBundleIdentifier=f"com.still.player.smoke-{uuid.uuid4().hex}",
                    CFBundleName="Still Smoke", CFBundleDisplayName="Still Smoke")
        info_path.write_bytes(plistlib.dumps(info))
        subprocess.run(["codesign", "--force", "--sign", "-", str(bundle)], check=True)
        server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        env = {key: value for key, value in os.environ.items() if not key.startswith("NAVIDROME_")}
        for key in ("STILL_UI_TEST_SCRIPT", "STILL_UI_TEST_RESULT", "STILL_DEMO_CAPTURE"):
            env.pop(key, None)
        env.update(NAVIDROME_URL=f"http://127.0.0.1:{server.server_port}",
                   STILL_CONFIG_DIR=str(root / "config"), STILL_LOG_DIR=str(root / "logs"))
        if args.size or args.automated:
            (root / "config").mkdir(exist_ok=True)
            preferences = dict(theme="system" if args.connection_only else "latte") if scripted else {}
            if args.size:
                preferences["window_size"] = args.size
            if args.record_demo is not None:
                preferences.update(sidebar_compact=False, last_section="albums")
            (root / "config/preferences.json").write_text(json.dumps(preferences))
        if args.case != "connection":
            env.update(NAVIDROME_USERNAME="fixture", NAVIDROME_PASSWORD="fixture")
        try:
            launches = args.repeat if args.connection_only else (3 if args.automated else (2 if args.relaunch else 1))
            for launch in range(launches):
                scenario_index = 2 if args.connection_only else launch
                methods.clear()
                if scripted:
                    script = root / f"ui-script-{launch}.json"
                    if args.record_demo is not None:
                        from demo_fixture import scenario
                        steps = scenario()
                        env["STILL_DEMO_CAPTURE"] = str(root)
                    else:
                        from ui_scenarios import scenario
                        steps = scenario(scenario_index, tracks)
                    script.write_text(json.dumps(steps, ensure_ascii=False))
                    env.update(STILL_UI_TEST_SCRIPT=str(script), STILL_UI_TEST_RESULT=str(root / f"ui-result-{launch}.json"))
                    if scenario_index == 2:
                        env.pop("NAVIDROME_USERNAME", None)
                        env.pop("NAVIDROME_PASSWORD", None)
                with (root / "app.log").open("ab") as log:
                    process = subprocess.Popen([str(bundle / "Contents/MacOS/still")], env=env, stdout=log, stderr=log)
                    print(json.dumps(dict(app=str(bundle), pid=process.pid, case=args.case, launch=launch + 1,
                                          config=str(root / "config/preferences.json"),
                                          log=str(root / "logs/still.log"))), flush=True)
                    try:
                        if args.record_demo is not None:
                            from demo_recording import capture
                            capture(process, root, args.record_demo)
                        status = process.wait(timeout=args.timeout)
                    except BaseException:
                        if process.poll() is None:
                            process.terminate()
                            process.wait(timeout=10)
                        raise
                print(json.dumps(dict(exitCode=status)), flush=True)
                if status != 0:
                    raise RuntimeError(f"App exited with status {status}")
                if scripted:
                    result = json.loads((root / f"ui-result-{launch}.json").read_text())
                    if not result["success"]:
                        raise RuntimeError(result["error"])
                    if args.automated and not args.connection_only and launch == 1:
                        if methods[:2] != ["ping", "search3"] or "getArtists" in methods:
                            raise RuntimeError("Restoring Folders must not load Artists")
                    if args.automated and scenario_index == 2 and methods.count("ping") != 2:
                        raise RuntimeError("Connection inputs must reach both fixture login responses")
                    print(json.dumps(dict(uiAssertions="passed", steps=result["steps"], launch=launch + 1)), flush=True)
                diagnostic_log = root / "logs/still.log"
                if diagnostic_log.exists():
                    contents = diagnostic_log.read_text()
                    print(json.dumps(dict(logBytes=diagnostic_log.stat().st_size,
                                          panics=contents.count("PANIC:"))), flush=True)
                    if "PANIC:" in contents:
                        raise RuntimeError("App logged a panic during the smoke check")
                preferences = root / "config/preferences.json"
                if preferences.exists():
                    print(json.dumps(dict(preferences=json.loads(preferences.read_text()))), flush=True)
            if scripted:
                saved = Path(__file__).resolve().parents[1] / ".local/ui-test-results" / uuid.uuid4().hex
                saved.mkdir(parents=True)
                for result in root.glob("ui-*.json"):
                    shutil.copy2(result, saved / result.name)
                print(f"Native UI checks passed; reports saved to {saved}", flush=True)
        except BaseException:
            saved = Path(__file__).resolve().parents[1] / ".local/smoke-failures" / uuid.uuid4().hex
            saved.mkdir(parents=True)
            if (root / "app.log").exists():
                shutil.copy2(root / "app.log", saved / "app.log")
            for name in ("capture.log", "capture.mov"):
                if (root / name).exists():
                    shutil.copy2(root / name, saved / name)
            if (root / "logs").exists():
                shutil.copytree(root / "logs", saved / "logs")
            for result in root.glob("ui-*.json"):
                shutil.copy2(result, saved / result.name)
            print(f"Smoke failure logs saved to {saved}", file=sys.stderr, flush=True)
            raise
        finally:
            server.shutdown()
            server.server_close()


if __name__ == "__main__":
    main()
