#!/usr/bin/env python3
"""Verify native previews of a two-page label and an exported design."""

import argparse
import json
import os
from pathlib import Path
import struct
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[2]
EXPECTED = [(102.0, 152.0, 816, 1216), (75.0, 50.0, 600, 400)]


def check_preview(manifest, expected):
    pages = json.loads(manifest.read_text(encoding="utf-8"))
    if len(pages) != len(expected):
        return False  # A restored tab can finish before the newly opened file.
    sizes = [(p["width_mm"], p["height_mm"], p["width"], p["height"]) for p in pages]
    if sizes != expected:
        raise AssertionError(f"Unexpected preview sizes: {sizes}")
    for page, (_, _, width, height) in zip(pages, expected):
        with (manifest.parent / page["file"]).open("rb") as image:
            header = image.read(24)
        if len(header) != 24 or header[:8] != b"\x89PNG\r\n\x1a\n":
            raise AssertionError(f"Invalid preview PNG: {page['file']}")
        if struct.unpack(">II", header[16:24]) != (width, height):
            raise AssertionError(f"Unexpected PNG dimensions: {page['file']}")
    return True


def stop_daemon(cli):
    stopped = subprocess.run([*cli, "stop"], capture_output=True, timeout=5)
    if stopped.returncode != 0:
        return
    # The service exits shortly after acknowledging shutdown. Wait for it to
    # close its files before removing the state directory, especially on Windows.
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        if subprocess.run([*cli, "status"], capture_output=True, timeout=5).returncode != 0:
            return
        time.sleep(0.1)
    raise TimeoutError("Background service did not stop")


def open_and_check(desktop, source, expected, state, cli):
    log_path = state.parent / "desktop.log"
    previous = set(state.glob("previews/document-*/pages.json"))
    env = dict(os.environ, LABELIZE_PRINTER_HOME=str(state))
    with log_path.open("w", encoding="utf-8") as log:
        app = subprocess.Popen([str(desktop), str(source)], env=env, stdout=log, stderr=subprocess.STDOUT)
        try:
            deadline = time.monotonic() + 45
            while time.monotonic() < deadline:
                if app.poll() is not None:
                    raise RuntimeError(f"Desktop exited with status {app.returncode}")
                manifests = set(state.glob("previews/document-*/pages.json")) - previous
                if any(check_preview(manifest, expected) for manifest in manifests):
                    status = subprocess.run([*cli, "status"], capture_output=True, timeout=5)
                    if status.returncode == 0 and json.loads(status.stdout)["raw_listening"]:
                        return
                time.sleep(0.2)
            raise TimeoutError(f"Desktop preview or service did not become ready for {source.name}")
        except Exception:
            print(log_path.read_text(encoding="utf-8", errors="replace")[-6000:])
            raise
        finally:
            if app.poll() is None:
                app.terminate()
                try:
                    app.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    app.kill()
                    app.wait()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, default=ROOT / "target/debug")
    args = parser.parse_args()
    suffix = ".exe" if os.name == "nt" else ""
    desktop = args.bin_dir.resolve() / f"labelize-printer{suffix}"
    daemon = args.bin_dir.resolve() / f"labelize-printer-daemon{suffix}"
    if not desktop.is_file() or not daemon.is_file():
        parser.error(f"Desktop and daemon binaries are required in {args.bin_dir}")

    with tempfile.TemporaryDirectory(prefix="studio-ui-") as directory:
        work = Path(directory).resolve()
        state = work / "state"
        cli = [str(daemon), "--data-dir", str(state)]
        try:
            open_and_check(desktop, ROOT / "examples/shipping-demo.zpl", EXPECTED, state, cli)
            print("PASS GPUI opened the ZPL example and rendered both preview pages")

            settings = json.loads(subprocess.check_output([*cli, "settings"], timeout=5))
            settings.update(dpmm=12, output_dir=str(work / "output"))
            config = work / "settings.json"
            config.write_text(json.dumps(settings), encoding="utf-8")
            subprocess.run([*cli, "settings", "--file", str(config)], check=True, capture_output=True, timeout=10)
            template = ROOT / "examples/designer-showcase.labelize.json"
            job = json.loads(subprocess.check_output(
                [*cli, "submit", str(template), "--format", "zpl", "--wait"], timeout=60
            ))
            source, = job["output_files"]
            stop_daemon(cli)
            open_and_check(desktop, Path(source), [(102.0, 152.0, 1224, 1824)], state, cli)
            print("PASS GPUI reopened the exported design at the saved 12 dpmm on startup")
        finally:
            stop_daemon(cli)


if __name__ == "__main__":
    main()
