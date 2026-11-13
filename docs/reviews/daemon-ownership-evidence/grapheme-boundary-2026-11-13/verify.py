#!/usr/bin/env python3
"""Noncompiling sealed-evidence audit. This is not qualification or approval."""
import ast
import gzip
import hashlib
import io
import json
import pathlib
import re
import subprocess
import tarfile
import tomllib

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[3]
BASE = '6e64e1d6ac359ea223baf6754d219ca2b2148a82'
ARCHIVE_SHA256 = 'ad3df8ea96c4f4c15924649f4bd2f448be3c0b432e9c15a16f5618da476208e3'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    raw = (HERE / 'evidence.tar.gz').read_bytes()
    assert sha(raw) == ARCHIVE_SHA256
    with tarfile.open(fileobj=io.BytesIO(raw), mode='r:gz') as archive:
        data = {m.name: archive.extractfile(m).read() for m in archive if m.isfile()}
    manifest = json.loads(data['file-sha256.json'])
    assert set(data) == set(manifest) | {'file-sha256.json'}
    for p, digest in manifest.items():
        assert sha(data[p]) == digest, p
        current = (HERE / p).read_bytes() if (HERE / p).exists() else gzip.decompress((HERE / (p + '.gz')).read_bytes())
        assert current == data[p], p
    assert data['report.md'] == (ROOT / 'docs/reviews/daemon-grapheme-effective-boundary-2026-11-13.md').read_bytes()
    baseline = json.loads(data['baseline-files.json'])
    assert len(baseline) == 7742
    for p, rec in baseline.items():
        assert sha((ROOT / p).read_bytes()) == rec['working_sha256'], p
    # Independently bind all baseline working hashes to exact Git/attributes.
    proc = subprocess.Popen(['git', 'archive', '--format=tar', BASE], cwd=ROOT, stdout=subprocess.PIPE)
    seen = set()
    with tarfile.open(fileobj=proc.stdout, mode='r|') as archive:
        for member in archive:
            if member.isfile():
                seen.add(member.name)
                assert sha(archive.extractfile(member).read()) == baseline[member.name]['baseline_archive_sha256'], member.name
    assert proc.wait() == 0 and seen == set(baseline)
    before, after = [json.loads(data[p + '.json']) for p in ('before', 'after')]
    assert before == after
    assert set(before) == {'full-daemon', 'tui', 'full-daemon,iroh-transport'}
    for profile, count in [('full-daemon', 917), ('tui', 955), ('full-daemon,iroh-transport', 1002)]:
        record = before[profile]
        assert len(record['packages']) == count
        for phase in ['before', 'after']:
            name = f'{phase}-{profile.replace(",", "-")}.txt.gz'
            text = gzip.decompress(data[name])
            assert sha(text) == record['graph_sha256']
            assert sorted(set(re.findall(rb'^([^ ]+ v[^ ]+) ', text, re.M))) == [p.encode() for p in record['packages']]
        features = record['features']
        modules = {'data', 'image', 'media', 'pdf', 'plot'}
        assert modules | {'host', 'stage-b', 'wasix-runtime'} <= set(features['grapheme-sdk v0.7.1'])
        assert modules | {'host', 'http', 'web', 'net', 'sql', 'surreal', 'email', 'transforms'} <= set(features['grapheme-stdlib v0.7.1'])
        assert modules | {'stage-b', 'wasix-runtime'} <= set(features['grapheme-runtime v0.7.1'])
        assert modules | {'full', 'stage-b'} <= set(features['grapheme-compiler v0.7.1'])
        assert modules | {'full'} <= set(features['grapheme-signatures v0.7.1'])
    upstream = json.loads(data['upstream-manifests.json'])
    source_hashes = json.loads(data['upstream-source-sha256.json'])
    proof = json.loads(data['upstream-registry-checksums.json'])
    locked = {p['name'] + '-' + p['version']: p for p in tomllib.loads((ROOT / 'Cargo.lock').read_text())['package']}
    assert set(proof) == set(upstream) and len(proof) == 12
    assert sum(len(p['checked_files']) for p in proof.values()) == 341
    for name, rec in proof.items():
        assert rec['locked_crate_sha256'] == locked[name]['checksum']
        assert rec['checked_files']['Cargo.toml'] == upstream[name]['sha256']
        assert sha(data[name + '-Cargo.toml']) == upstream[name]['sha256']
        for p, h in rec['checked_files'].items():
            if p != 'Cargo.toml':
                assert source_hashes[name + '/' + p] == h
    receipts = json.loads(data['receipts.json'])
    by = {r['label']: r for r in receipts}
    assert len(receipts) == len(by) == 28
    for r in receipts:
        assert r['exit_code'] == r['expected_exit_code'] == 0 and not r['stopped_at_8GiB'], r['label']
        assert r['min_free_bytes'] == min(s['free_bytes'] for s in r['samples'])
        assert r['min_free_bytes'] >= 8 * 2**30
    assert data['lifetime.log'].count(b'test result: ok. 1 passed; 0 failed; 0 ignored;') == 3
    assert by['lifetime']['command'] == ['bash', 'scripts/ci/test-platform-lifetime.sh']
    counts = {'symlink': 1, 'drain': 4, 'identity': 2, 'settings': 5, 'contracts': 4,
              'spine': 7, 'catalog': 4, 'grapheme': 11, 'recovery': 1, 'boundary': 9}
    assert sum(counts.values()) + 3 == 51
    binary = json.loads(data['binary.json'])
    assert binary['sha256'] == 'c54a858343716ae93208de944c7ff66cf8019c88c5f4b0c2f04655d43cd7bc03'
    assert binary['baseline'] == BASE and binary['rebuilt'] is False
    a = binary['artifact']
    assert a['fresh'] and a['features'] == ['full-daemon'] and a['profile']['test']
    assert a['target']['src_path'] == str(ROOT / 'src/lib.rs')
    for label, count in counts.items():
        assert len(by[label]['summaries']) == 1
        assert f'test result: ok. {count} passed; 0 failed; 0 ignored;' in by[label]['summaries'][0]
        assert by[label]['command'][0] == a['executable']
        assert 'running 0 tests' not in data[label + '.log'].decode()
    assert b'Ran 9 tests' in data['packaging.log'] and data['packaging.log'].rstrip().endswith(b'OK')
    policy = json.loads(data['run-policy.json'])
    assert policy['CARGO_BUILD_JOBS'] == '2' and policy['CARGO_NET_OFFLINE'] == 'true'
    assert policy['RUST_MIN_STACK'] is None and policy['TMPDIR'].startswith('/var/folders/')
    for p in ['inspect.py', 'qualify.py']:
        ast.parse(data[p])
    ast.parse(pathlib.Path(__file__).read_text())
    print(f'PASS sealed evidence: {len(manifest)} archived files; {len(baseline)} baseline working files byte-identical.')
    print('PASS three identical before/after resolved graphs, retained implementation/signature/AOT/WASIX closure; ZERO effective slimming.')
    print('51 unique Rust + 9 packaging tests recorded freshly executed on authenticated unchanged cached binary; no rebuild, approval or integration implied.')
    print('Minimum sampled free GiB:', min(r['min_free_bytes'] for r in receipts) / 2**30)


if __name__ == '__main__':
    main()
