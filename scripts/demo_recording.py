"""Capture only the isolated fixture window and export GIF and silent H.264 video."""
import json
import math
from pathlib import Path
import shutil
import subprocess
import time


def wait_for(path, process, timeout):
    deadline = time.monotonic() + timeout
    while not path.exists():
        if process.poll() is not None:
            raise RuntimeError("Demo app exited before the recording handshake completed")
        if time.monotonic() >= deadline:
            raise TimeoutError(f"Timed out waiting for recorder signal: {path.name}")
        time.sleep(0.05)


def capture(process, directory, output):
    for tool in ("swift", "ffmpeg", "ffprobe", "screencapture"):
        if not shutil.which(tool):
            raise RuntimeError(f"Recording requires {tool}")
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=True)
    wait_for(directory / "ready", process, 45)
    # Query only this process's main window. Never capture the desktop or another app.
    query = '''import CoreGraphics
import Foundation
let pid = Int(CommandLine.arguments[1])!
let windows = CGWindowListCopyWindowInfo(.optionAll, kCGNullWindowID) as! [[String: Any]]
let own = windows.filter { ($0[kCGWindowOwnerPID as String] as? Int) == pid && ($0[kCGWindowName as String] as? String) == "Still — Navidrome" }
guard let window = own.first, let number = window[kCGWindowNumber as String] as? Int else { exit(1) }
print(number)
'''
    window_id = subprocess.check_output(["swift", "-e", query, str(process.pid)], text=True).strip()
    if not window_id.isdigit():
        raise RuntimeError("Could not identify the isolated demo window")
    steps = json.loads((directory / "ui-script-0.json").read_text())
    # Allow for native frames, assertions and server responses in addition to
    # the deliberately paced scene holds. Export ends at the actual done signal.
    planned_seconds = sum(step.get("milliseconds", 0) for step in steps) / 1000
    recording_seconds = math.ceil(planned_seconds + len(steps) * 0.08 + 12)
    raw = directory / "capture.mov"
    with (directory / "capture.log").open("wb") as log:
        recorded_at = time.monotonic()
        recorder = subprocess.Popen(["screencapture", "-x", "-o", f"-l{window_id}", "-v", f"-V{recording_seconds}", str(raw)], stdout=log, stderr=log)
        try:
            time.sleep(1)
            if recorder.poll() is not None:
                raise RuntimeError("Window capture failed; inspect capture.log")
            (directory / "started").touch()
            wait_for(directory / "done", process, recording_seconds + 15)
            clip_duration = time.monotonic() - recorded_at + 0.25
            if clip_duration >= recording_seconds:
                raise RuntimeError("Demo exceeded the capture budget; increase the recording margin")
            # screencapture finalizes its movie only on its own timed stop.
            # Keep the app on the final frame until that completes.
            recorder.wait(timeout=recording_seconds + 15)
        finally:
            if recorder.poll() is None:
                recorder.terminate()
            try:
                recorder.wait(timeout=10)
            except subprocess.TimeoutExpired:
                recorder.kill()
                recorder.wait()
        if recorder.returncode != 0 or not raw.is_file():
            raise RuntimeError("Window capture did not produce a complete movie")
    (directory / "finished").touch()
    probe = json.loads(subprocess.check_output(["ffprobe", "-v", "error", "-show_streams", "-show_format", "-of", "json", str(raw)]))
    stream = probe["streams"][0]
    scale = stream["width"] / 1120
    content_height = round(760 * scale)
    title_height = stream["height"] - content_height
    if title_height < 0 or title_height > 100 * scale:
        raise RuntimeError("Unexpected capture geometry; refusing to export")
    crop = f"crop=iw:{content_height}:0:{title_height}"
    subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-i", str(raw),
                    "-t", str(clip_duration), "-vf", crop + ",scale=1680:-2:flags=lanczos,fps=30", "-an", "-c:v", "libx264", "-preset", "slow", "-crf", "20",
                    "-pix_fmt", "yuv420p", "-movflags", "+faststart", "-map_metadata", "-1", str(output / "still.mp4")], check=True)
    palette = directory / "palette.png"
    gif_filter = crop + ",fps=12,scale=1008:-2:flags=lanczos"
    # Two passes avoid retaining a whole recording's uncompressed frames in RAM.
    subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-i", str(raw),
                    "-t", str(clip_duration), "-vf", gif_filter + ",palettegen=stats_mode=diff", "-frames:v", "1", str(palette)], check=True)
    subprocess.run(["ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-i", str(raw), "-i", str(palette),
                    "-t", str(clip_duration), "-filter_complex", f"[0:v]{gif_filter}[v];[v][1:v]paletteuse=dither=bayer:bayer_scale=4:diff_mode=rectangle",
                    "-an", "-loop", "0", "-map_metadata", "-1", str(output / "still.gif")], check=True)
    print(json.dumps({"demoOutput": str(output), "duration": clip_duration,
                      "gifBytes": (output / "still.gif").stat().st_size, "mp4Bytes": (output / "still.mp4").stat().st_size}), flush=True)
