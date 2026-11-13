#!/usr/bin/env python3
"""Fresh bounded checks; compiled application inputs remain baseline-identical."""
import hashlib
import importlib.util
import json
import pathlib
import subprocess

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[3]
spec = importlib.util.spec_from_file_location('bounded', ROOT / 'docs/reviews/daemon-ownership-evidence/harness-correction-2026-11-13/qualify.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
m.HERE = HERE  # Reuse monitor code, NEVER its historical output paths/main.


def main():
    m.run('environment', ['sh', '-c', 'rustc -Vv; cargo -V; uname -a; df -k .; git rev-parse HEAD; command -v rg || true'])
    common = ['cargo', 'check', '--locked', '--offline', '-p', 'medousa', '--no-default-features']
    m.run('daemon-check', common + ['--features', 'full-daemon', '--bin', 'medousa_daemon'])
    m.run('tui-check', common + ['--features', 'tui', '--bin', 'medousa_tui', '--bin', 'medousa'])
    text = m.run('test-artifact', ['cargo', 'test', '--locked', '--offline', '-p', 'medousa', '--no-default-features', '--features', 'full-daemon', '--lib', '--no-run', '--message-format=json'])
    artifacts = [json.loads(l) for l in text.splitlines() if l.startswith('{')]
    candidates = [a for a in artifacts if a.get('reason') == 'compiler-artifact' and a.get('executable') and a.get('target', {}).get('name') == 'medousa']
    assert len(candidates) == 1
    a = candidates[0]
    old = json.loads((ROOT / 'docs/reviews/daemon-ownership-evidence/independent-9026f46/binary.json').read_text())
    binary = a['executable']
    digest = hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest()
    assert a['fresh'] and digest == old['sha256']
    (HERE / 'binary.json').write_text(json.dumps({'artifact': a, 'sha256': digest, 'baseline': '6e64e1d6ac359ea223baf6754d219ca2b2148a82', 'rebuilt': False}, indent=2) + '\n')
    text = m.run('lifetime', ['bash', 'scripts/ci/test-platform-lifetime.sh'])
    assert text.count('test result: ok. 1 passed; 0 failed; 0 ignored;') == 3
    selectors = [
        ('symlink', 'runtime::platform::lifetime_tests::fixture_root_resolves_symlink_ancestor_and_retains_owner', 1),
        ('drain', 'runtime::event_delivery::', 4),
        ('identity', 'ownership_tests::', 2),
        ('settings', 'runtime::settings::', 5),
        ('contracts', 'tool_contract_baseline::', 4),
        ('spine', 'agent_runtime::spine_byte_parity::', 7),
        ('catalog', 'grapheme_host_catalog::tests::', 4),
        ('grapheme', 'grapheme_runtime::tests::', 11),
        ('recovery', 'engine_recovery::tests::recovery_replays_uncommitted_journal_and_marks_committed', 1),
        ('boundary', 'daemon::request_boundary::tests::', 9),
    ]
    for label, selector, count in selectors:
        text = m.run(label, [binary, selector, '--test-threads=1'] + (['--exact'] if count == 1 else []))
        assert f'test result: ok. {count} passed; 0 failed; 0 ignored;' in text
    for label, cmd in [
        ('packaging', ['python3', 'scripts/ci/test-daemon-packaging.py']),
        ('graph-guard', ['python3', 'scripts/ci/check-daemon-dependencies.py', '--offline']),
        ('ownership-python', ['python3', 'docs/reviews/daemon-ownership-evidence/harness-correction-2026-11-13/check-runtime-ownership.py']),
        ('historical-independent', ['python3', 'docs/reviews/daemon-ownership-evidence/independent-9026f46/verify.py']),
        ('historical-harness', ['python3', 'docs/reviews/daemon-ownership-evidence/harness-correction-2026-11-13/verify.py']),
        ('after-graph', ['python3', str(HERE / 'inspect.py'), 'after']),
    ]:
        m.run(label, cmd)
    print('PASS fresh cached qualification: 51 unique Rust tests, 9 packaging checks; no binary rebuild')


if __name__ == '__main__':
    main()
