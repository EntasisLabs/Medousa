#!/usr/bin/env python3
"""Reproduce this offline qualification from the repository root; no cache cleanup.
Requires CARGO_TARGET_DIR to name the already selected reusable build cache.
Each command is bounded to two jobs; stop its process group at the 8 GiB floor.
"""
import datetime
import hashlib
import json
import os
from pathlib import Path
import shlex
import shutil
import signal
import subprocess
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
RNDR = Path('/Users/theelevators/rndr')
PIN = '44f5dc7c0c91af56e4f3e29f97283b8e8d1e1139'
FLOOR = 8 * 1024**3
os.chdir(ROOT)
assert os.environ.get('CARGO_TARGET_DIR'), 'choose one existing reusable target first'
TARGET = Path(os.environ['CARGO_TARGET_DIR']).resolve()
assert TARGET.is_dir(), 'reuse an existing cache, do not create a duplicate'
os.environ.update(CARGO_INCREMENTAL='0', CARGO_BUILD_JOBS='2',
                  CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0')

def git(*args):
    return subprocess.check_output(['git', '-C', str(RNDR), *args], text=True).strip()

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

assert git('rev-parse', 'HEAD') == PIN
assert not git('status', '--porcelain'), 'framework must be clean and read-only'
report = {'utc_start': datetime.datetime.now(datetime.timezone.utc).isoformat(),
          'medousa_baseline': subprocess.check_output(['git', 'rev-parse', 'HEAD'], text=True).strip(),
          'rndr_before': PIN, 'rndr_status_before': '', 'target': str(TARGET),
          'environment': {k: os.environ[k] for k in ['CARGO_INCREMENTAL', 'CARGO_BUILD_JOBS',
                          'CARGO_PROFILE_DEV_DEBUG', 'CARGO_PROFILE_TEST_DEBUG']},
          'checks': []}

def run(name, args):
    free = shutil.disk_usage(ROOT).free
    assert free > FLOOR, '8 GiB disk floor before command'
    check = {'name': name, 'command': shlex.join(map(str, args)), 'free_before_bytes': free}
    report['checks'].append(check)
    with (HERE / f'{name}.log').open('w') as log:
        log.write(check['command'] + '\n')
        log.flush()
        process = subprocess.Popen(list(map(str, args)), stdout=log, stderr=subprocess.STDOUT,
                                   start_new_session=True)
        minimum = free
        while process.poll() is None:
            free = shutil.disk_usage(ROOT).free
            minimum = min(minimum, free)
            if free <= FLOOR:
                os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=10)
                raise RuntimeError('stopped process group at 8 GiB disk floor')
            time.sleep(0.25)
        check.update(exit_code=process.returncode, minimum_free_bytes=minimum,
                     free_after_bytes=shutil.disk_usage(ROOT).free)
    (HERE / 'run.json').write_text(json.dumps(report, indent=2) + '\n')
    print(name, check['exit_code'], flush=True)
    assert process.returncode == 0, f'{name} failed; see its log'

cargo = ['cargo']
manifest = ['--manifest-path', 'apps/rndr-home/Cargo.toml']
common = manifest + ['--offline', '--locked', '-j', '2']
run('fmt', cargo + ['fmt'] + manifest + ['--', '--check'])
run('default', cargo + ['test'] + common)
run('observe', cargo + ['test'] + common + ['--features', 'observe'])
run('clippy-default', cargo + ['clippy'] + common + ['--all-targets', '--', '-D', 'warnings'])
run('clippy-observe', cargo + ['clippy'] + common + ['--all-targets', '--all-features', '--', '-D', 'warnings'])
run('build-default', cargo + ['build'] + common)
binary = TARGET / 'debug/medousa-rndr-home'
report['default_binary_sha256'] = sha(binary)
run('build-observe', cargo + ['build'] + common + ['--features', 'observe'])
report['observe_binary_sha256'] = sha(binary)
for appearance, flags in [('dark', []), ('light', ['--light']), ('high-contrast', ['--high-contrast'])]:
    run('export-' + appearance, [binary, '--export', HERE / appearance, *flags])
run('native', ['python3', 'apps/rndr-home/scripts/capture_native.py', binary, RNDR, HERE / 'native-empty'])
run('docs', ['bash', 'scripts/verify-docs.sh', '--strict'])
run('diff', ['git', 'diff', '--check'])
report.update(rndr_after=git('rev-parse', 'HEAD'), rndr_status_after=git('status', '--porcelain'),
              utc_end=datetime.datetime.now(datetime.timezone.utc).isoformat())
assert report['rndr_after'] == PIN and not report['rndr_status_after']
(HERE / 'run.json').write_text(json.dumps(report, indent=2) + '\n')
