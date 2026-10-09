#!/usr/bin/env python3
"""Exercise the real Linux window on a private X server. Requires Xvfb, xdotool,
python-xlib, Pillow, and a Vulkan driver (Mesa llvmpipe works). No user data is used.
Build first, then run: python3 scripts/smoke_x11.py
"""
import json
import os
from pathlib import Path
import select
import shutil
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
LOCAL = ROOT / ".local/sysroot/usr"
sys.path.insert(0, str(LOCAL / "lib/python3/dist-packages"))
from PIL import Image, ImageGrab  # noqa: E402
from Xlib import X, display, protocol  # noqa: E402


def executable(name):
    local = LOCAL / "bin" / name
    return str(local) if local.exists() else shutil.which(name) or name


class TestDirectory(tempfile.TemporaryDirectory):
    def __init__(self, *args, xdisplay, **kwargs):
        super().__init__(*args, **kwargs)
        self.xdisplay = xdisplay

    def __exit__(self, exc_type, exc_value, traceback):
        if exc_type is not None:
            try:
                ImageGrab.grab(xdisplay=self.xdisplay).save(Path(self.name) / "failure.png")
            except Exception as error:
                print(f"Could not capture failure screenshot: {error}", file=sys.stderr)
            destination = ROOT / "artifacts/smoke-failure"
            shutil.copytree(self.name, destination, dirs_exist_ok=True)
            log = destination / "app.log"
            if log.exists():
                print(log.read_text(), file=sys.stderr)
            print(f"Failure diagnostics saved to {destination}", file=sys.stderr)
        return super().__exit__(exc_type, exc_value, traceback)


def run():
    env = dict(os.environ)
    env.pop("WAYLAND_DISPLAY", None)
    env["LC_ALL"] = "C.UTF-8"
    env["LD_LIBRARY_PATH"] = str(LOCAL / "lib/x86_64-linux-gnu") + ":" + env.get("LD_LIBRARY_PATH", "")
    read_fd, write_fd = os.pipe()
    xvfb = subprocess.Popen(
        [executable("Xvfb"), "-displayfd", str(write_fd), "-screen", "0", "1280x820x24", "-ac", "-nolisten", "tcp"],
        env=env, pass_fds=[write_fd], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    )
    os.close(write_fd)
    app = None
    try:
        with os.fdopen(read_fd) as pipe:
            env["DISPLAY"] = ":" + pipe.readline().strip()
        with TestDirectory(prefix="magpie-smoke-", xdisplay=env["DISPLAY"]) as temporary:
            data = Path(temporary)
            env["MAGPIE_DATA_DIR"] = str(data / "library")
            log = open(data / "app.log", "w+")
            binary = os.environ.get("MAGPIE_BINARY", str(ROOT / "target/debug/magpie"))

            def x(*args):
                subprocess.run([executable("xdotool"), *map(str, args)], env=env, check=True, stdout=subprocess.DEVNULL)
                time.sleep(0.08)

            def start():
                nonlocal app
                process = subprocess.Popen([binary], env=env, stdout=log, stderr=log)
                app = process
                for _ in range(100):
                    if process.poll() is not None:
                        raise AssertionError("App exited during startup")
                    found = subprocess.run([executable("xdotool"), "search", "--onlyvisible", "--name", "^Magpie$"], env=env, capture_output=True, text=True)
                    if found.stdout.strip():
                        window = int(found.stdout.splitlines()[0])
                        x("windowmap", window, "windowsize", window, 1280, 820, "windowmove", window, 0, 0, "windowfocus", window)
                        x("mousemove", 20, 100, "mousemove", 600, 100)
                        # Mapping the OS window precedes GPUI's first rendered frame.
                        # Software Vulkan can take several seconds on a cold CI runner.
                        connection = display.Display(env["DISPLAY"])
                        try:
                            for _ in range(300):
                                if process.poll() is not None:
                                    raise AssertionError("App exited before its first frame")
                                pixel = connection.screen().root.get_image(600, 100, 1, 1, X.ZPixmap, 0xFFFFFFFF)
                                if pixel and any(pixel.data[:3]):
                                    break
                                time.sleep(0.1)
                            else:
                                ImageGrab.grab(xdisplay=env["DISPLAY"]).save(data / "startup.png")
                                raise AssertionError("App did not render its first frame within 30 seconds")
                        finally:
                            connection.close()
                        x("windowfocus", window, "mousemove", 600, 100, "click", 1)
                        return process, window
                    time.sleep(0.1)
                raise AssertionError("Window never appeared")

            def state():
                time.sleep(0.75)  # Include the zoom animation and debounced persistence.
                manifest = data / "library/boards.json"
                for _ in range(50):
                    if manifest.exists():
                        return json.loads(manifest.read_text())
                    time.sleep(0.1)
                raise AssertionError("No saved board after waiting for startup and autosave")

            def board():
                current = state()
                return next(b for b in current["boards"] if b["id"] == current["active"])

            def screen(b, item, rx=0.5, ry=0.5):
                c = b["camera"]
                return [round((item["x"] + item["width"] * rx) * c["zoom"] + c["x"]), round((item["y"] + item["height"] * ry) * c["zoom"] + c["y"])]

            app, window = start()
            x("key", "ctrl+n")
            assert len(state()["boards"]) == 2, "New board shortcut"
            x("key", "F2")
            # A cold software renderer may need time to paint the new dialog
            # and register its text input before native typing can be handled.
            for _ in range(50):
                if ImageGrab.grab(xdisplay=env["DISPLAY"]).getpixel((465, 360))[:3] == (34, 36, 38):
                    break
                time.sleep(0.1)
            else:
                raise AssertionError("Rename dialog did not render")
            x("type", "--clearmodifiers", "Smoke board")
            x("key", "Return")
            assert board()["name"] == "Smoke board", "Rename"
            # Board navigation is docked; switching boards leaves it open.
            x("mousemove", 85, 180, "click", 1)
            assert board()["name"] == "Untitled board", "Sidebar switches boards"
            x("mousemove", 85, 226, "click", 1)
            assert board()["name"] == "Smoke board", "Sidebar stays open after switching"
            x("mousemove", 205, 40, "click", 1)
            assert ImageGrab.grab(xdisplay=env["DISPLAY"]).getpixel((50, 400))[:3] == (23, 25, 27), "Sidebar collapses"
            x("mousemove", 40, 40, "click", 1)
            assert ImageGrab.grab(xdisplay=env["DISPLAY"]).getpixel((50, 400))[:3] == (29, 31, 33), "Sidebar reopens"
            x("mousemove", 100, 98, "click", 1)
            assert len(state()["boards"]) == 3, "New board button"
            x("key", "ctrl+z")
            assert board()["name"] == "Smoke board", "Undo board creation"

            paths = []
            for number, color in enumerate(["#9fae85", "#987552", "#819bad"]):
                path = data / f"image-{number}.png"
                Image.new("RGB", (400, 280), color).save(path)
                paths.append(path)
            d = display.Display(env["DISPLAY"])
            root = d.screen().root
            source = root.create_window(0, 0, 1, 1, 0, d.screen().root_depth, X.InputOutput, X.CopyFromParent)
            target = d.create_resource_object("window", window)
            atom = d.intern_atom
            uri_type = atom("text/uri-list")
            source.set_selection_owner(atom("XdndSelection"), X.CurrentTime)
            root.warp_pointer(160, 180)

            def send(kind, values):
                target.send_event(protocol.event.ClientMessage(window=target, client_type=atom(kind), data=(32, values)), event_mask=0)
                d.flush()

            send("XdndEnter", [source.id, 5 << 24, uri_type, 0, 0])
            send("XdndPosition", [source.id, 0, (160 << 16) | 180, X.CurrentTime, atom("XdndActionCopy")])
            deadline = time.time() + 5
            while time.time() < deadline:
                if not d.pending_events():
                    select.select([d.fileno()], [], [], 0.05)
                    continue
                event = d.next_event()
                if event.type == X.SelectionRequest:
                    payload = "\r\n".join(path.as_uri() for path in paths) + "\r\n"
                    event.requestor.change_property(event.property, uri_type, 8, payload.encode())
                    event.requestor.send_event(protocol.event.SelectionNotify(time=event.time, requestor=event.requestor, selection=event.selection, target=event.target, property=event.property), event_mask=0)
                    d.flush()
                    time.sleep(0.2)
                    send("XdndDrop", [source.id, 0, X.CurrentTime, 0, 0])
                elif event.type == X.ClientMessage and event.client_type == atom("XdndFinished"):
                    break
            for _ in range(15):
                if len(board()["images"]) == 3:
                    break
            else:
                log.flush()
                raise AssertionError(f"OS file drop: {state()}\n{Path(log.name).read_text()}")
            for path in paths:
                path.unlink()
            for item in board()["images"]:
                assert (data / "library/images" / item["asset"]).exists(), "Owned preview"
                assert (data / "library/images" / item["original"]).exists(), "Owned original"

            x("key", "Escape", "f")
            before = board()
            item = before["images"][0]
            p = screen(before, item)
            x("mousemove", *p, "mousedown", 1)
            x("mousemove", p[0] + 70, p[1] + 45)
            x("mouseup", 1)
            moved = board()["images"][0]
            assert abs(moved["x"] - item["x"] - 70 / before["camera"]["zoom"]) < 2, "Move image"
            x("key", "ctrl+z")
            assert board()["images"] == before["images"], "Undo move"
            x("key", "ctrl+shift+z")
            assert board()["images"][0] == moved, "Redo move"

            b = board()
            item = b["images"][0]
            x("mousemove", *screen(b, item), "click", 1)
            p = screen(b, item, 1, 1)
            x("mousemove", *p, "mousedown", 1)
            x("mousemove", p[0] + 60, p[1] + 40)
            x("mouseup", 1)
            resized = board()["images"][0]
            assert resized["width"] > item["width"] + 30, "Resize handle"
            assert abs(resized["height"] / resized["width"] - item["height"] / item["width"]) < 0.0001, "Aspect ratio"
            x("key", "ctrl+d")
            assert len(board()["images"]) == 4, "Duplicate"
            x("key", "Delete")
            assert len(board()["images"]) == 3, "Delete"
            x("key", "ctrl+z")
            assert len(board()["images"]) == 4, "Undo delete"
            x("key", "ctrl+z")
            assert len(board()["images"]) == 3, "Undo duplicate"

            x("key", "Escape")
            b = board()
            x("mousemove", 640, 650, "mousedown", 1)
            x("mousemove", 720, 680)
            x("mouseup", 1)
            camera = board()["camera"]
            assert abs(camera["x"] - b["camera"]["x"] - 80) < 2, "Canvas pan"
            x("mousemove", 640, 410, "click", 4)
            panned = board()["camera"]
            assert panned["zoom"] == camera["zoom"] and panned["y"] != camera["y"], "Wheel pans"
            x("keydown", "shift", "click", 4, "keyup", "shift")
            horizontal = board()["camera"]
            assert horizontal["x"] != panned["x"] and horizontal["y"] == panned["y"], "Shift-wheel pans horizontally"
            panned = horizontal
            x("keydown", "ctrl", "click", 4, "keyup", "ctrl")
            zoomed = board()["camera"]
            assert zoomed["zoom"] > panned["zoom"], "Ctrl-wheel zoom"
            for axis, point in [("x", 640), ("y", 410)]:
                assert abs((point - panned[axis]) / panned["zoom"] - (point - zoomed[axis]) / zoomed["zoom"]) < 0.02, ("Zoom stays anchored to the pointer", axis, panned, zoomed)
            x("key", "shift+0")
            assert abs(board()["camera"]["zoom"] - 1) < 0.000001, "Actual size shortcut"
            x("key", "ctrl+plus")
            assert board()["camera"]["zoom"] == 1.25, "Zoom in shortcut"
            x("key", "ctrl+minus")
            assert board()["camera"]["zoom"] == 1, "Zoom out shortcut"
            x("key", "shift+1")
            fitted = board()
            item = fitted["images"][0]
            center = screen(fitted, item)
            x("mousemove", *center, "click", 1)
            x("key", "shift+2")
            selected = board()
            assert max(abs(a - b) for a, b in zip(screen(selected, selected["images"][0]), [760, 410])) < 2, "Fit selection centers the selected image"

            def enter_zoom(value):
                x("mousemove", 776, 773, "click", 1)
                time.sleep(0.2)
                x("key", "ctrl+a")
                x("type", "--clearmodifiers", value)
                x("key", "Return")
                return board()["camera"]

            assert enter_zoom("125% ")["zoom"] == 1.25, "Editable zoom percentage"
            # Invalid text keeps the input open, without mutating the camera.
            assert enter_zoom("NaN")["zoom"] == 1.25, "Invalid zoom is rejected"
            x("key", "Escape")
            x("key", "shift+2")
            board()
            for value, expected in [("1000000%", 10000), ("1e11%", 1e9)]:
                assert enter_zoom(value)["zoom"] == expected, "Zoom far beyond the old upper cap"
                # The selected solid-color image must still cover the viewport.
                pixel = ImageGrab.grab(xdisplay=env["DISPLAY"]).getpixel((760, 410))[:3]
                assert max(abs(a - b) for a, b in zip(pixel, (159, 174, 133))) <= 2, ("Image renders at extreme zoom", value, pixel)
            assert abs(enter_zoom("0.00001%")["zoom"] - 1e-7) < 1e-16, "Zoom far beyond the old lower cap"
            x("key", "shift+1")
            board()
            x("key", "ctrl+a", "Delete")
            assert not board()["images"], "Delete multiple"
            x("key", "ctrl+z")
            assert len(board()["images"]) == 3, "Undo multiple"

            clipboard_image = data / "clipboard.png"
            Image.new("RGB", (32, 32), "#cccccc").save(clipboard_image)
            source.set_selection_owner(atom("CLIPBOARD"), X.CurrentTime)
            d.flush()
            x("key", "ctrl+v")
            deadline = time.time() + 4
            while time.time() < deadline:
                if not d.pending_events():
                    select.select([d.fileno()], [], [], 0.05)
                    continue
                event = d.next_event()
                if event.type != X.SelectionRequest:
                    continue
                if event.target == atom("TARGETS"):
                    event.requestor.change_property(event.property, atom("ATOM"), 32, [atom("TARGETS"), atom("image/png")])
                else:
                    event.requestor.change_property(event.property, atom("image/png"), 8, clipboard_image.read_bytes())
                event.requestor.send_event(protocol.event.SelectionNotify(time=event.time, requestor=event.requestor, selection=event.selection, target=event.target, property=event.property), event_mask=0)
                d.flush()
                if event.target == atom("image/png"):
                    break
            for _ in range(10):
                if len(board()["images"]) == 4:
                    break
            else:
                log.flush()
                raise AssertionError(f"Clipboard paste: {board()}\n{Path(log.name).read_text()}")
            d.close()

            second = subprocess.run([binary], env=env, capture_output=True, timeout=5)
            assert second.returncode != 0 and b"already open" in second.stderr, "Concurrent library protection"
            saved = state()
            x("key", "ctrl+q")
            app.wait(timeout=5)
            app, window = start()
            reopened = state()
            assert reopened == saved, ("Reopen restores boards and camera positions", saved, reopened)
            x("key", "ctrl+q")
            app.wait(timeout=5)
            print("PASS: native sidebar switching/collapse, board creation/rename, OS file drop, owned imports, move, resize, pan, anchored zoom, zoom menu, extreme scales, fit selection, zoom shortcuts, duplicate, delete, undo/redo, clipboard paste, library locking, restart persistence")
            log.close()
    finally:
        if app and app.poll() is None:
            app.terminate()
            app.wait(timeout=5)
        xvfb.terminate()
        xvfb.wait(timeout=5)


if __name__ == "__main__":
    run()
