#!/usr/bin/env python3
"""Bounded serialized qualification. Reuses configured cache; never deletes it."""
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
BASE = '71a149b3c982a828c3ea8bd76967191c40ea9091'
os.chdir(ROOT)
ENV = dict(os.environ, CARGO_BUILD_JOBS='2', CARGO_NET_OFFLINE='true', MEDOUSA_TEST_HERMETIC='1', CARGO_TERM_COLOR='never')
ENV.pop('RUST_MIN_STACK', None)  # Runner must own its stack, without caller assistance.
# TMPDIR is deliberately inherited unchanged, not canonicalized or replaced.
RECEIPTS = []

def run(label, command, expected=0):
    log = HERE / (label + '.log')
    samples = []
    stopped = False
    started = time.time()
    free = lambda: os.statvfs(ROOT).f_bavail * os.statvfs(ROOT).f_frsize
    assert free() >= 8 * 1024**3, '8 GiB stop threshold before start'
    with log.open('w') as output:
        process = subprocess.Popen(command, env=ENV, stdout=output, stderr=subprocess.STDOUT, start_new_session=True)
        while True:
            available = free()
            samples.append({'elapsed_seconds': round(time.time() - started, 3), 'free_bytes': available})
            if available < 8 * 1024**3 and process.poll() is None:
                stopped = True
                os.killpg(process.pid, signal.SIGINT)
                try:
                    process.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGTERM)
                process.wait()
            if process.poll() is not None:
                break
            time.sleep(2)
    text = log.read_text()
    receipt = dict(label=label, command=command, exit_code=process.returncode,
                   expected_exit_code=expected, stopped_at_8GiB=stopped,
                   min_free_bytes=min(s['free_bytes'] for s in samples), samples=samples,
                   seconds=round(time.time() - started, 3),
                   summaries=re.findall(r'^test result:.*$', text, re.M))
    RECEIPTS.append(receipt)
    (HERE / 'receipts.json').write_text(json.dumps(RECEIPTS, indent=2) + '\n')
    print(label, receipt['exit_code'], receipt['summaries'], 'min GiB', receipt['min_free_bytes']/1024**3, flush=True)
    assert not stopped and process.returncode == expected, label
    return text

if __name__ == '__main__':
    import tarfile
    with tarfile.open(ROOT / 'docs/reviews/daemon-ownership-evidence/independent-f44c405/evidence.tar.gz') as archive:
        member = next(m for m in archive.getmembers() if m.name.endswith('/source-fingerprints.json'))
        paths = json.loads(archive.extractfile(member).read())
    fingerprints = {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest() for p in paths}
    (HERE / 'source-fingerprints.json').write_text(json.dumps(fingerprints, indent=2, sort_keys=True) + '\n')
    (HERE / 'environment.json').write_text(json.dumps(dict(base=BASE, stack=ENV.get('RUST_MIN_STACK'),
        TMPDIR=ENV.get('TMPDIR'), temp_realpath=os.path.realpath(ENV.get('TMPDIR', '/tmp')),
        CARGO_BUILD_JOBS=ENV['CARGO_BUILD_JOBS'], CARGO_NET_OFFLINE=ENV['CARGO_NET_OFFLINE'],
        MEDOUSA_TEST_HERMETIC=ENV['MEDOUSA_TEST_HERMETIC']), indent=2) + '\n')
    run('provenance', ['sh', '-c', 'rustc -Vv; cargo -V; rustup show active-toolchain; uname -a; df -k .; git rev-parse HEAD; git diff --stat; cat .cargo/config.toml'])
    log = run('lifetime', ['bash', 'scripts/ci/test-platform-lifetime.sh'])
    assert log.count('test result: ok. 1 passed; 0 failed; 0 ignored;') == 3
    text = run('test-executable', ['cargo', 'test', '--locked', '--offline', '-p', 'medousa', '--no-default-features', '--features', 'full-daemon', '--lib', '--no-run', '--message-format=json'])
    artifacts = [json.loads(line) for line in text.splitlines() if line.startswith('{')]
    binaries = [a['executable'] for a in artifacts if a.get('reason') == 'compiler-artifact' and a.get('executable') and a.get('target', {}).get('name') == 'medousa']
    assert len(binaries) == 1, binaries
    binary = binaries[0]
    (HERE / 'binary.json').write_text(json.dumps(dict(path=binary, sha256=hashlib.sha256(Path(binary).read_bytes()).hexdigest()), indent=2) + '\n')
    selectors = [
        ('fixture-symlink', 'runtime::platform::lifetime_tests::fixture_root_resolves_symlink_ancestor_and_retains_owner', 1),
        ('focused-1', 'runtime::event_delivery::', 4),
        ('focused-2', 'ownership_tests::', 2),
        ('focused-3', 'runtime::settings::', 5),
        ('focused-4', 'tool_contract_baseline::', 4),
        ('focused-5', 'agent_runtime::spine_byte_parity::', 7),
        ('focused-6', 'grapheme_host_catalog::tests::', 4),
        ('focused-7', 'grapheme_runtime::tests::', 11),
        ('focused-8', 'engine_recovery::tests::recovery_replays_uncommitted_journal_and_marks_committed', 1),
        ('focused-9', 'daemon::request_boundary::tests::', 9),
    ]
    for label, selector, count in selectors:
        text = run(label, [binary, selector, '--test-threads=1'] + (['--exact'] if label == 'fixture-symlink' else []))
        assert f'test result: ok. {count} passed; 0 failed; 0 ignored;' in text
    for label, command in [
        ('packaging', ['python3', 'scripts/ci/test-daemon-packaging.py']),
        ('dependency-guard', ['python3', 'scripts/ci/check-daemon-dependencies.py', '--offline']),
        ('runtime-ownership', ['bash', 'scripts/ci/check-runtime-context.sh']),
        ('rustfmt', ['rustfmt', '--edition', '2024', '--check', 'src/runtime/platform.rs', 'src/runtime/event_delivery.rs']),
        ('bash-syntax', ['bash', '-n', 'scripts/ci/test-platform-lifetime.sh', 'scripts/release/build.sh', 'scripts/mobile-dev-daemon.sh']),
        ('workflow-yaml', ['ruby', '-e', 'require "yaml"; ARGV.each { |f| YAML.parse_file(f); puts "PASS: #{f}" }', '.github/workflows/ci.yml', '.github/workflows/release.yml']),
        ('diff-check', ['git', 'diff', '--check', BASE]),
    ]:
        run(label, command)
    assert all(hashlib.sha256((ROOT / p).read_bytes()).hexdigest() == h for p, h in fingerprints.items())
    print('PASS all qualification; source fingerprints unchanged:', len(fingerprints), flush=True)
