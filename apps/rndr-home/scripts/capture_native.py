#!/usr/bin/env python3
"""Capture ONLY the audited offline fixture; never inspect other apps/chat data.
Usage: python3 scripts/capture_native.py BINARY RNDR_CHECKOUT OUTPUT_PREFIX
Build the binary with --features observe first. Outputs must not exist.
"""
import pathlib
import select
import subprocess
import sys
import time

binary, rndr, prefix = map(pathlib.Path, sys.argv[1:])
# The shared rndr checkout is strictly read-only, including Python caches.
sys.dont_write_bytecode = True
sys.path.insert(0, str(rndr / "tools"))
from observe_client import observe, save_frame

process = subprocess.Popen([str(binary.resolve()), "--observe-offline"], stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
descriptor = None
try:
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        if select.select([process.stderr], [], [], 0.2)[0]:
            line = process.stderr.readline().decode()
            if line.startswith("observation descriptor: "):
                descriptor = pathlib.Path(line.strip().split(": ", 1)[1])
                break
            if not line and process.poll() is not None:
                raise RuntimeError("native app exited before observation became available")
    if descriptor is None:
        raise RuntimeError("native app did not start an observation session")
    for attempt in range(8):
        try:
            metadata, png = observe(descriptor)
            break
        except Exception:
            if attempt == 7:
                raise
            time.sleep(0.35)
    assert metadata["source"] == "live-surface-copy-before-present"
    assert metadata["presented_confirmation"] is False
    prefix.parent.mkdir(parents=True, exist_ok=True)
    save_frame(prefix, metadata, png)
    print(f"Native surface {metadata['width']}x{metadata['height']}, frame {metadata['frame_revision']}; not compositor confirmation")
    process.wait(timeout=20)
    assert process.returncode == 0
    assert not descriptor.exists(), "normal shutdown must remove observation capability"
    print("Normal timed exit and observation cleanup passed")
finally:
    if process.poll() is None:
        process.terminate()
        process.wait(timeout=5)
