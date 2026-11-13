#!/usr/bin/env python3
"""Offline increment gates. From repo root; reuses admitted target; no native capture.
Optional arguments select individual named gates, e.g. `default` or `export-dark`.
Separate invocation receipts are appended, never overwrite historical evidence.
"""
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
os.chdir(ROOT)
TARGET = Path(os.environ['CARGO_TARGET_DIR']).resolve()
assert TARGET.is_dir(), 'existing reusable target required'
os.environ.update(CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2',
                  CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0')
FLOOR = 8 * 1024**3
common = ['--manifest-path', 'apps/rndr-home/Cargo.toml', '--offline', '--locked', '-j', '2']
binary = TARGET / 'debug/medousa-rndr-home'
gates = {
    'fmt': ['cargo', 'fmt', '--manifest-path', 'apps/rndr-home/Cargo.toml', '--', '--check'],
    'default': ['cargo', 'test', *common],
    'observe': ['cargo', 'test', *common, '--features', 'observe'],
    'clippy': ['cargo', 'clippy', *common, '--all-targets', '--all-features', '--', '-D', 'warnings'],
    'build': ['cargo', 'build', *common],
    'build-observe': ['cargo', 'build', *common, '--features', 'observe'],
    'export-dark': [str(binary), '--export', str(HERE / 'dark')],
    'export-light': [str(binary), '--export', str(HERE / 'light'), '--light'],
    'export-contrast': [str(binary), '--export', str(HERE / 'high-contrast'), '--high-contrast'],
    'diff': ['git', 'diff', '--check'],
}
for name in (sys.argv[1:] or list(gates)):
    args = gates[name]
    assert shutil.disk_usage(ROOT).free > FLOOR
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%S%fZ')
    minimum = shutil.disk_usage(ROOT).free
    logname = name + '-' + stamp + '.log'
    with (HERE / logname).open('w') as log:
        p = subprocess.Popen(args, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        while p.poll() is None:
            minimum = min(minimum, shutil.disk_usage(ROOT).free)
            if minimum <= FLOOR:
                os.killpg(p.pid, signal.SIGTERM)
                p.wait(timeout=10)
                raise RuntimeError('Stopped at sampled 8 GiB floor')
            time.sleep(.25)
    record = dict(gate=name, utc=stamp, command=args, exit=p.returncode,
                  minimum_free_bytes=minimum, free_after_bytes=shutil.disk_usage(ROOT).free,
                  head=subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
                  target=str(TARGET), environment={k: os.environ[k] for k in
                  ['CARGO_INCREMENTAL', 'CARGO_BUILD_JOBS', 'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG']},
                  log=logname)
    if binary.is_file():
        record['binary_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
    with (HERE / 'runs.jsonl').open('a') as f:
        f.write(json.dumps(record) + '\n')
    print(name, p.returncode, 'min-free-GiB', round(minimum / 1024**3, 2), flush=True)
    assert p.returncode == 0, logname
