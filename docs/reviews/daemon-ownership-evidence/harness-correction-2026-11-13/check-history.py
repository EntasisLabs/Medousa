#!/usr/bin/env python3
"""Version-bound history validation. Does not rewrite or requalify historical tests."""
import hashlib
import json
from pathlib import Path
import subprocess
import tarfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
BASE = '71a149b3c982a828c3ea8bd76967191c40ea9091'
def sha(data):
    return hashlib.sha256(data).hexdigest()
def git_bytes(path):
    return subprocess.check_output(['git', 'cat-file', '--filters', f'{BASE}:{path}'], cwd=ROOT)

# Every previously tracked evidence file and the independent report remains exact.
paths = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', BASE, '--',
    'docs/reviews/daemon-ownership-evidence',
    'docs/reviews/daemon-ownership-independent-rereview-2026-11-13.md'], cwd=ROOT).decode().splitlines()
for path in paths:
    assert (ROOT / path).read_bytes() == git_bytes(path), f'historical evidence changed: {path}'
print(f'PASS immutable historical evidence/report: {len(paths)} files identical to {BASE}')

archive = ROOT / 'docs/reviews/daemon-ownership-evidence/independent-f44c405/evidence.tar.gz'
assert sha(archive.read_bytes()) == '57915460bbebb20444581c9ab482a0043d611bd1a59d7511bb6213041f42b37d'
with tarfile.open(archive) as tar:
    files = {Path(m.name).name: tar.extractfile(m).read() for m in tar.getmembers() if m.isfile()}
manifest = json.loads(files['file-sha256.json'])
for name, digest in manifest.items():
    assert sha(files[name]) == digest, name
sources = json.loads(files['source-fingerprints.json'])
changed = []
for path, digest in sources.items():
    if sha((ROOT / path).read_bytes()) != digest:
        changed.append(path)
        assert sha(git_bytes(path)) == digest, f'historical source version mismatch: {path}'
# The old 1082-file inventory did not include the packaging Python regression.
expected = ['scripts/ci/test-platform-lifetime.sh', 'src/runtime/platform/lifetime_tests.rs']
assert sorted(changed) == expected, changed
print(f'PASS historical independent archive: {len(manifest)} files and {len(sources)} source fingerprints; {len(changed)} resolved against historical Git source, NOT revised qualification')
print('Old current-source auditors are expected to reject these revised harness sources; preserved verbatim. Use verify.py here for fresh source evidence.')
