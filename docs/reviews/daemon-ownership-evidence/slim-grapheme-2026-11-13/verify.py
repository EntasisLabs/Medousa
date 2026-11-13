#!/usr/bin/env python3
"""Verify source/evidence bindings, resolved acceptance and nonzero test receipts."""
import gzip
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]


def check_hashes(base, manifest):
    for path, digest in manifest.items():
        actual = hashlib.sha256((base / path).read_bytes()).hexdigest()
        assert actual == digest, (str(base / path), digest, actual)


def main():
    sources = json.loads((HERE / 'source-fingerprints.json').read_text())
    check_hashes(ROOT, sources)
    historical = json.loads((HERE / 'historical-fingerprints.json').read_text())
    check_hashes(ROOT, historical)
    files = json.loads((HERE / 'evidence-sha256.json').read_text())
    check_hashes(HERE, files)
    g = json.loads((HERE / 'graphs.json').read_text())
    before, after = g['before'], g['after']
    grapheme = lambda inv: {k: v for k, v in inv.items() if k.startswith('grapheme-')}
    nonroot = lambda inv: {k: v for k, v in inv.items() if not k.startswith('medousa v')}
    assert before['embedded'] == after['embedded']
    for label in ('daemon', 'iroh'):
        assert grapheme(after[label]) == grapheme(before['embedded'])
        assert not g['summary'][label]['added']
    assert nonroot(before['tui']) == nonroot(after['tui'])
    assert nonroot(before['daemon']) == nonroot(after['extended'])
    receipts = json.loads((HERE / 'receipts.json').read_text())
    assert all(not r['stopped_at_8GiB'] and r['exit_code'] == 0 for r in receipts)
    assert min(r['min_free_bytes'] for r in receipts) >= 8 * 1024**3
    by_label = {r['label']: r for r in receipts}
    for name in ('daemon-check', 'tui-check', 'test-artifact', 'lifetime', 'slim',
                 'symlink', 'drain', 'identity', 'settings', 'contracts', 'spine',
                 'catalog', 'grapheme', 'recovery', 'boundary', 'shell-workflow',
                 'worker-cancellation', 'secret-dispatch', 'secret-workflow',
                 'http-authority', 'http-engine-authority', 'packaging', 'graph-guard'):
        assert name in by_label, name
    counts = {'slim': 2, 'symlink': 1, 'drain': 4, 'identity': 2, 'settings': 5,
              'contracts': 4, 'spine': 7, 'catalog': 4, 'grapheme': 11,
              'recovery': 1, 'boundary': 9, 'shell-workflow': 1,
              'worker-cancellation': 1, 'secret-dispatch': 1, 'secret-workflow': 1,
              'http-authority': 1, 'http-engine-authority': 1}
    for name, count in counts.items():
        assert any(s.startswith(f'test result: ok. {count} passed; 0 failed; 0 ignored;')
                   for s in by_label[name]['summaries']), name
    assert len(by_label['lifetime']['summaries']) == 3
    assert all(s.startswith('test result: ok. 1 passed; 0 failed; 0 ignored;')
               for s in by_label['lifetime']['summaries'])
    print(f'PASS {len(sources)} source fingerprints, {len(files)} evidence files; '
          f'{len(historical)} historical files unchanged; embedded graph match; '
          f'{sum(counts.values()) + 3} fresh Rust tests')


if __name__ == '__main__':
    main()
