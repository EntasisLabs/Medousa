#!/usr/bin/env python3
"""Equivalent source scan when the existing rg-based guard cannot run."""
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[4]
script = (ROOT / 'scripts/ci/check-runtime-context.sh').read_text()
pattern = re.search(r"^forbidden='([^']+)'$", script, re.M)[1]
count = 0
matches = []
for directory in ('src', 'apps/medousa-home/src-tauri/src'):
    root = ROOT / directory
    assert root.is_dir(), root
    for path in root.rglob('*.rs'):
        count += 1
        for number, line in enumerate(path.read_text().splitlines(), 1):
            if re.search(pattern, line):
                matches.append(f'{path.relative_to(ROOT)}:{number}:{line}')
assert not matches, '\n'.join(matches)
print(f'PASS Python equivalent forbidden-symbol scan: {count} Rust files; same pattern and roots as unavailable rg guard')
