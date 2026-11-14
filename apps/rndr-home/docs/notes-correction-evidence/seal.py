#!/usr/bin/env python3
"""Deterministic seals of saved evidence, not a verification or acceptance claim."""
import hashlib
import json
import pathlib
import subprocess

ROOT = pathlib.Path(subprocess.check_output(['git', 'rev-parse', '--show-toplevel'], text=True).strip())
HERE = pathlib.Path(__file__).resolve().parent
SOURCE = '1d438f27420a1c390cd7a9e1d689bcdff83ee0a4'
PRIOR = '465d1e14d6d634fe8e7602c13fd2f5d1fdec2fa3'
BASE = 'a4ea4400482a27183340f738f0d961d571b94157'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def write(name, value):
    (HERE / name).write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def git(*args):
    return subprocess.check_output(['git', *args], cwd=ROOT)


paths = git('ls-tree', '-r', '--name-only', SOURCE, 'apps/rndr-home/src', 'apps/rndr-home/Cargo.toml', 'apps/rndr-home/Cargo.lock', 'apps/rndr-home/docs/CONTENT_SURFACES.md').decode().splitlines()
files = {p: sha(git('show', f'{SOURCE}:{p}')) for p in paths}
for path, digest in files.items():
    assert sha((ROOT / path).read_bytes()) == digest, path
assert not git('diff', PRIOR, '--', 'apps/rndr-home/docs/content-evidence').strip()
write('source-manifest.json', dict(implementation_commit=SOURCE, implementation_tree=git('rev-parse', SOURCE + '^{tree}').decode().strip(), reviewed_prior_candidate=PRIOR, undertaking_baseline=BASE, source_sha256=files, prior_review_is_not_corrected_review=True))
write('artifact-seals.json', {str(p.relative_to(HERE)): sha(p.read_bytes()) for p in sorted(HERE.rglob('*')) if p.is_file() and p.name != 'artifact-seals.json'})
print('Sealed saved evidence and exact source; no approval or integration.')
