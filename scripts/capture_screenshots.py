#!/usr/bin/env python3
"""Capture the real app with a temporary demo library. Uses the Linux smoke-test
dependencies and downloads three demo photos from Unsplash. No user data is read.
"""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import urllib.request
import uuid

from smoke_x11 import ROOT, LOCAL, executable
from PIL import ImageGrab

PHOTOS = {
    "forest": "https://images.unsplash.com/photo-1441974231531-c6227db76b6e?w=1000&q=90",
    "lake": "https://images.unsplash.com/photo-1493246507139-91e8fad9978e?w=1200&q=90",
    "mountain": "https://images.unsplash.com/photo-1519681393784-d120267933ba?w=1000&q=90",
}


def main():
    output = ROOT / "docs/screenshots"
    output.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ)
    env.pop("WAYLAND_DISPLAY", None)
    env["LD_LIBRARY_PATH"] = str(LOCAL / "lib/x86_64-linux-gnu") + ":" + env.get("LD_LIBRARY_PATH", "")
    read_fd, write_fd = os.pipe()
    server = subprocess.Popen([executable("Xvfb"), "-displayfd", str(write_fd), "-screen", "0", "1280x820x24", "-ac", "-nolisten", "tcp"], env=env, pass_fds=[write_fd], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    os.close(write_fd)
    app = None
    try:
        with os.fdopen(read_fd) as pipe:
            env["DISPLAY"] = ":" + pipe.readline().strip()
        with tempfile.TemporaryDirectory(prefix="magpie-screenshots-") as temporary:
            work = Path(temporary)
            env["MAGPIE_DATA_DIR"] = str(work / "library")
            binary = os.environ.get("MAGPIE_BINARY", str(ROOT / "target/debug/magpie"))

            def x(*args):
                subprocess.run([executable("xdotool"), *map(str, args)], env=env, check=True, stdout=subprocess.DEVNULL)
                time.sleep(0.15)

            def start(paths=()):
                process = subprocess.Popen([binary, *map(str, paths)], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                for _ in range(100):
                    if process.poll() is not None:
                        raise RuntimeError("Magpie exited before opening its window")
                    result = subprocess.run([executable("xdotool"), "search", "--name", "^Magpie$"], env=env, text=True, capture_output=True)
                    if result.stdout.strip():
                        window = int(result.stdout.splitlines()[0])
                        x("windowmap", window, "windowsize", window, 1280, 820, "windowmove", window, 0, 0, "windowfocus", window)
                        return process
                    time.sleep(0.1)
                raise RuntimeError("Magpie window did not appear")

            paths = []
            for name, url in PHOTOS.items():
                path = work / f"{name}.jpg"
                with urllib.request.urlopen(url, timeout=30) as response:
                    path.write_bytes(response.read())
                paths.append(path)
            app = start(paths)
            manifest = work / "library/boards.json"
            for _ in range(100):
                if manifest.exists() and len(json.loads(manifest.read_text())["boards"][0]["images"]) == 3:
                    break
                time.sleep(0.2)
            else:
                raise RuntimeError("Demo import did not finish")
            x("key", "ctrl+q")
            app.wait(timeout=5)
            library = json.loads(manifest.read_text())
            board = library["boards"][0]
            board["name"] = "Field notes"
            layout = {"forest.jpg": (155, 175, 390), "lake.jpg": (585, 145, 505), "mountain.jpg": (555, 505, 360)}
            for item in board["images"]:
                x_pos, y_pos, width = layout[item["name"]]
                item.update(x=x_pos, y=y_pos, width=width, height=width * item["height"] / item["width"])
            for name in ["Architecture", "Next collection"]:
                library["boards"].append({"id": str(uuid.uuid4()), "name": name, "camera": {"x": 0.0, "y": 0.0, "zoom": 1.0}, "images": []})
            manifest.write_text(json.dumps(library))
            app = start()
            time.sleep(3)
            x("mousemove", 20, 400)
            ImageGrab.grab(xdisplay=env["DISPLAY"]).save(output / "canvas.png", optimize=True)
            x("mousemove", 656, 773, "click", 1)
            x("mousemove", 20, 400)
            time.sleep(0.4)
            ImageGrab.grab(xdisplay=env["DISPLAY"]).save(output / "zoom.png", optimize=True)
            x("key", "Escape")
            x("mousemove", 200, 42, "click", 1)
            x("mousemove", 20, 400)
            time.sleep(0.4)
            ImageGrab.grab(xdisplay=env["DISPLAY"]).save(output / "boards.png", optimize=True)
            x("key", "Escape", "ctrl+n")
            time.sleep(0.6)
            ImageGrab.grab(xdisplay=env["DISPLAY"]).save(output / "empty.png", optimize=True)
            x("key", "ctrl+q")
            app.wait(timeout=5)
            print(f"Captured native screenshots in {output}")
    finally:
        if app and app.poll() is None:
            app.terminate()
            app.wait(timeout=5)
        server.terminate()
        server.wait(timeout=5)


if __name__ == "__main__":
    main()
