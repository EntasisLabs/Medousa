#!/usr/bin/env python3
"""Verify independent review evidence integrity, not candidate readiness. No builds."""
import hashlib
import io
import json
from pathlib import Path
import re
import tarfile

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
ARCHIVE_SHA256 = '57915460bbebb20444581c9ab482a0043d611bd1a59d7511bb6213041f42b37d'
blob = (HERE / 'evidence.tar.gz').read_bytes()
assert hashlib.sha256(blob).hexdigest() == ARCHIVE_SHA256, 'archive changed'
with tarfile.open(fileobj=io.BytesIO(blob), mode='r:gz') as archive:
    files = {Path(member.name).name: archive.extractfile(member).read()
             for member in archive.getmembers() if member.isfile()}
manifest = json.loads(files['file-sha256.json'])
for name, digest in manifest.items():
    assert hashlib.sha256(files[name]).hexdigest() == digest, name
sources = json.loads(files['source-fingerprints.json'])
for path, digest in sources.items():
    assert hashlib.sha256((ROOT / path).read_bytes()).hexdigest() == digest, path
static = json.loads(files['static-receipts.json'])
assert all(receipt['exit_code'] == 0 for receipt in static.values())
receipts = json.loads(files['current-test-receipts.json'])
focused = [r for r in receipts['results'] if r['label'].startswith('focused-')]
counts = [int(re.search(r'ok\. (\d+) passed;', r['summary'][0])[1]) for r in focused]
assert counts == [4, 2, 5, 4, 7, 4, 11, 1, 9]
assert all(r['exit_code'] == 0 for r in focused)
for name in ('daemon-check', 'tui-check', 'lifetime-canonical-stack'):
    receipt = json.loads(files[name + '.json'])
    assert receipt['head'] == 'f44c405c3e91bbe160ed148b34265beffff25a1b'
    assert receipt['exit_code'] == 0 and not receipt['stopped_at_8GiB']
    assert receipt['min_free_bytes'] > 8 * 1024**3
assert files['lifetime-canonical-stack.log'].count(b'test result: ok. 1 passed; 0 failed; 0 ignored;') == 3
assert json.loads(files['lifetime.json'])['exit_code'] == 101
assert b'overflowed its stack' in files['lifetime.log']
assert json.loads(files['lifetime-large-stack.json'])['exit_code'] == 101
assert b'open vault root /var/' in files['lifetime-large-stack.log']
print(f'PASS evidence integrity: {len(manifest)} files, {len(sources)} unchanged source fingerprints, 47 fresh focused tests, 3 environment-qualified constructor tests.')
print('Default committed runner FAILS; exact candidate verdict remains CHANGES REQUESTED. No build or executable rerun performed by this auditor.')
