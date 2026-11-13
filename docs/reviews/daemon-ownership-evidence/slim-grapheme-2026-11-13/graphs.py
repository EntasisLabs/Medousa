#!/usr/bin/env python3
"""Compare standalone normal/build graphs with the actual embedded reference."""
import gzip
import hashlib
import json
from pathlib import Path
import re
import subprocess

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
PROFILES = {'daemon': 'full-daemon', 'embedded': 'embedded-daemon', 'tui': 'tui',
            'extended': 'grapheme-extended', 'iroh': 'full-daemon,iroh-transport'}


def inventory(text):
    result = {}
    for line in text.splitlines():
        m = re.match(r'^(\S+ v\S+) (?:\([^\n]*?\) )?FEATURES=\[([^\]]*)\]', line)
        if m:
            result[m[1]] = sorted(filter(None, m[2].split(',')))
    return result


def grapheme(inv):
    return {k: v for k, v in inv.items() if k.startswith('grapheme-')}


def main():
    summary = {}
    before = {}
    after = {}
    commands = []
    for label, profile in PROFILES.items():
        command = ['cargo', 'tree', '--locked', '--offline', '-p', 'medousa',
                   '--no-default-features', '--features', profile, '-e', 'normal,build',
                   '--prefix', 'none', '--format', '{p} FEATURES=[{f}]']
        commands.append(command)
        text = subprocess.check_output(command, cwd=ROOT, text=True)
        (HERE / f'after-{label}.txt.gz').write_bytes(gzip.compress(text.encode(), mtime=0))
        after[label] = inventory(text)
        old = HERE / f'before-{label}.txt.gz'
        if old.exists():
            before[label] = inventory(gzip.decompress(old.read_bytes()).decode())
            b, a = before[label], after[label]
            summary[label] = {
                'before_package_versions': len(b), 'after_package_versions': len(a),
                'removed': sorted(b.keys() - a.keys()), 'added': sorted(a.keys() - b.keys()),
                'feature_changes': {k: {'before': b[k], 'after': a[k]} for k in b.keys() & a.keys() if b[k] != a[k]},
            }
    # The real embedded profile, unchanged by this patch, not the host-free probe.
    assert before['embedded'] == after['embedded']
    slim = grapheme(before['embedded'])
    for label in ('daemon', 'embedded', 'iroh'):
        assert grapheme(after[label]) == slim, label
    assert grapheme(after['extended']) == grapheme(before['daemon'])
    # All non-root TUI dependency features remain exactly as before.
    nonroot = lambda inv: {k: v for k, v in inv.items() if not k.startswith('medousa v')}
    assert nonroot(before['tui']) == nonroot(after['tui'])
    assert nonroot(before['daemon']) == nonroot(after['extended'])
    for label in ('daemon', 'iroh'):
        assert not summary[label]['added']
    (HERE / 'graphs.json').write_text(json.dumps({
        'baseline': '4e85141b32d6c74a25427e7506758b38c8683739',
        'commands': commands, 'summary': summary, 'slim_grapheme': slim,
        'before': before, 'after': after,
        'assertions': ['embedded graph unchanged', 'daemon/Iroh Grapheme package/features equal embedded baseline',
                       'extended graph restores previous daemon dependencies/features', 'TUI dependencies/features unchanged'],
    }, indent=2, sort_keys=True) + '\n')
    for label, row in summary.items():
        print(label, row['before_package_versions'], '->', row['after_package_versions'],
              'removed', len(row['removed']), 'added', len(row['added']))
    print('PASS exact embedded Grapheme match; unchanged TUI and restored extended graphs')


if __name__ == '__main__':
    main()
