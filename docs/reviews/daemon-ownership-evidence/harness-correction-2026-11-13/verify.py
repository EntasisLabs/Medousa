#!/usr/bin/env python3
"""Noncompiling audit of fresh observations, revised sources and immutable history."""
import ast
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import tarfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
ARCHIVE_SHA256 = 'c7b520ea3d09f53641d88a40ea980ab269f03e9d937d54108ceb1052196c0b0b'
BASE = '71a149b3c982a828c3ea8bd76967191c40ea9091'
SOURCE = 'c0d47829f11f643044b278a8694c4c9023e867ca'
def sha(data):
    return hashlib.sha256(data).hexdigest()
blob = (HERE / 'evidence.tar.gz').read_bytes()
assert sha(blob) == ARCHIVE_SHA256, 'fresh archive changed'
with tarfile.open(fileobj=io.BytesIO(blob), mode='r:gz') as archive:
    files = {Path(m.name).name: archive.extractfile(m).read() for m in archive.getmembers() if m.isfile()}
manifest = json.loads(files['file-sha256.json'])
assert set(manifest) == set(files) - {'file-sha256.json'}
for name, digest in manifest.items():
    assert sha(files[name]) == digest, name
for script in ('qualify.py', 'check-history.py', 'check-runtime-ownership.py'):
    assert (HERE / script).read_bytes() == files[script], script
    ast.parse(files[script], filename=script)
ast.parse(Path(__file__).read_text(), filename=__file__)
assert (ROOT / 'docs/reviews/daemon-ownership-harness-corrections-2026-11-13.md').read_bytes() == files['report.md']
source = json.loads(files['source-version.json'])
assert source['base'] == BASE and source['implementation_commit'] == SOURCE
sources = json.loads(files['source-fingerprints.json'])
assert len(sources) == source['source_count'] == 1083
for path, digest in sources.items():
    assert sha((ROOT / path).read_bytes()) == digest, f'revised source mismatch: {path}'
changed = subprocess.check_output(['git', 'diff', '--name-only', BASE, SOURCE], cwd=ROOT).decode().splitlines()
assert sorted(changed) == sorted(source['changed_sources']), changed
assert files['source.patch'] == subprocess.check_output(['git', 'diff', '--binary', BASE, SOURCE], cwd=ROOT)
for path, digest in source['changed_sources'].items():
    assert sha(subprocess.check_output(['git', 'cat-file', '--filters', f'{SOURCE}:{path}'], cwd=ROOT)) == digest

env = json.loads(files['environment.json'])
assert env['stack'] is None and env['TMPDIR'].startswith('/var/folders/')
assert env['temp_realpath'].startswith('/private/var/folders/')
assert env['CARGO_BUILD_JOBS'] == '2' and env['CARGO_NET_OFFLINE'] == 'true'
receipts = json.loads(files['receipts.json'])
by_label = {r['label']: r for r in receipts}
assert len(by_label) == len(receipts)
for receipt in receipts:
    assert not receipt['stopped_at_8GiB']
    assert receipt['min_free_bytes'] > 8 * 1024**3
    assert min(s['free_bytes'] for s in receipt['samples']) == receipt['min_free_bytes']
required = ['provenance', 'lifetime', 'committed-lifetime', 'test-executable', 'fixture-symlink',
    *[f'focused-{i}' for i in range(1, 10)], 'packaging', 'packaging-final', 'dependency-guard',
    'runtime-ownership-python', 'rustfmt', 'bash-syntax', 'workflow-yaml', 'diff-check',
    'final-diff-check', 'history-final', 'docs-strict']
for label in required:
    assert by_label[label]['exit_code'] == 0, label
    assert by_label[label]['expected_exit_code'] == 0
for label in ('lifetime', 'committed-lifetime'):
    assert by_label[label]['command'] == ['bash', 'scripts/ci/test-platform-lifetime.sh']
    assert len(by_label[label]['summaries']) == 3
    assert files[label+'.log'].count(b'test result: ok. 1 passed; 0 failed; 0 ignored;') == 3
    assert files[label+'.log'].count(b'running 1 test') == 3
counts = []
for i in range(1, 10):
    summaries = by_label[f'focused-{i}']['summaries']
    assert len(summaries) == 1
    counts.append(int(re.search(r'ok\. (\d+) passed; 0 failed; 0 ignored;', summaries[0])[1]))
assert counts == [4, 2, 5, 4, 7, 4, 11, 1, 9]
assert 'ok. 1 passed; 0 failed; 0 ignored;' in by_label['fixture-symlink']['summaries'][0]
assert b'Ran 9 tests' in files['packaging-final.log'] and files['packaging-final.log'].rstrip().endswith(b'OK')
# Preserve non-green observations rather than counting a misleading zero exit.
assert 'UNAVAILABLE' in by_label['runtime-ownership']['qualification']
assert b'rg: command not found' in files['runtime-ownership.log']
assert by_label['history']['exit_code'] == 1  # corrected inventory assumption, not Rust failure
assert by_label['historical-current-auditor-rejection']['exit_code'] == 1
assert b'AssertionError: scripts/ci/test-platform-lifetime.sh' in files['historical-current-auditor-rejection.log']
assert b'781 Rust files' in files['runtime-ownership-python.log']
assert json.loads(files['binary.json'])['sha256'] == 'c54a858343716ae93208de944c7ff66cf8019c88c5f4b0c2f04655d43cd7bc03'
subprocess.run(['python3', str(HERE/'check-history.py')], cwd=ROOT, check=True)
print(f'PASS fresh evidence: {len(manifest)} archived files, {len(sources)} revised source fingerprints; 51 unique Rust tests and 9 Python checks recorded passing.')
print('Committed runner: 3 separate exact tests PASS with no caller stack or TMPDIR workaround. No Rust rerun, production/native certification, acceptance or integration implied.')
