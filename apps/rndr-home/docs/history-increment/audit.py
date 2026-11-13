#!/usr/bin/env python3
"""Read-only Git/disk audit; writes only this increment's audit receipts."""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
BASE = '15eedfbc66cf4f2713f1ac148a27b7f80f1080e7'
SOURCE = '995fcd5b85ab67708634623f83251fd1ce363d0d'

def git(*args):
    return subprocess.check_output(['git', '-C', str(ROOT), *args])

def tree(ref):
    return {e.split(b'\t', 1)[1].decode(): e.split(b'\t', 1)[0].decode()
            for e in git('ls-tree', '-rz', ref).split(b'\0') if e}

old, new = tree(BASE), tree(SOURCE)
allowed = {'apps/rndr-home/src/' + f + '.rs' for f in
           ['backend_tests', 'composer', 'controller', 'conversation', 'fixtures',
            'history_tests', 'main', 'observation_tests', 'shell', 'ui_tests']}
allowed.add('apps/rndr-home/docs/history-increment/run_checks.py')
changed = sorted(p for p in old.keys() | new.keys() if old.get(p) != new.get(p))
assert set(changed) == allowed, changed
preserved = sorted(p for p in old if p not in allowed)
for p in preserved:
    assert old[p] == new[p], p
disk_diff = git('diff', '--name-only', SOURCE).decode().splitlines()
assert all(p.startswith('apps/rndr-home/docs/history-increment/') and
           not p.endswith('/run_checks.py') for p in disk_diff), disk_diff
for p in changed:
    assert (ROOT / p).read_bytes() == git('show', SOURCE + ':' + p), p

sealed = {}
for name, directory, manifest in [
    ('closure', 'apps/rndr-home/docs', 'closure-evidence/REVIEW_SHA256SUMS'),
    ('qualification', 'apps/rndr-home/docs/qualification-44f5dc7c', 'SHA256SUMS'),
]:
    directory = ROOT / directory
    count = 0
    for line in (directory / manifest).read_text().splitlines():
        if not line.strip():
            continue
        expected, path = line.split(None, 1)
        path = directory / path.lstrip('*')
        assert hashlib.sha256(path.read_bytes()).hexdigest() == expected, str(path)
        count += 1
    sealed[name] = count

runs = [json.loads(line) for line in (HERE / 'runs.jsonl').read_text().splitlines()]
exact = list({r['gate']: r for r in runs if r['head'] == SOURCE}.values())
assert len(exact) == 10 and all(r['exit'] == 0 for r in exact)
source_files = {p: hashlib.sha256((ROOT / p).read_bytes()).hexdigest() for p in changed}
report = dict(base=BASE, source_commit=SOURCE, changed_paths=changed,
              preserved_tree_entries=len(preserved), sealed_files_passed=sealed,
              minimum_free_bytes_all_runs=min(r['minimum_free_bytes'] for r in runs),
              minimum_free_bytes_exact_source=min(r['minimum_free_bytes'] for r in exact),
              final_free_bytes=shutil.disk_usage(ROOT).free,
              exact_source_passing_gates=[r['gate'] for r in exact],
              source_sha256=source_files,
              limits='jobs <=2; debug=0; incremental=0; existing target; sampled 8GiB floor',
              network='Mock HTTP fixtures use loopback only; Cargo offline/locked; no provider requests',
              visual_origin='Synthetic offscreen exports only; no native/live private capture')
assert report['final_free_bytes'] > 8 * 1024**3
(HERE / 'preservation.json').write_text(json.dumps(report, indent=2) + '\n')
(HERE / 'implementation.patch').write_bytes(git('diff', '--binary', BASE, SOURCE))
print(json.dumps({k: v for k, v in report.items() if k != 'source_sha256'}, indent=2))
