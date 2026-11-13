#!/usr/bin/env python3
"""Fresh slim-profile checks, serial, offline, same cache, active 8 GiB stop."""
import hashlib
import importlib.util
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
spec = importlib.util.spec_from_file_location('monitor', ROOT / 'docs/reviews/daemon-ownership-evidence/harness-correction-2026-11-13/qualify.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)
m.HERE = HERE
m.ENV['CARGO_BUILD_JOBS'] = '1'


def main():
    m.run('environment', ['sh', '-c', 'rustc -Vv; cargo -V; uname -a; df -k .; git rev-parse HEAD'])
    common = ['cargo', 'check', '--locked', '--offline', '-p', 'medousa', '--no-default-features']
    m.run('daemon-check', common + ['--features', 'full-daemon', '--bin', 'medousa_daemon'])
    m.run('tui-check', common + ['--features', 'tui', '--bin', 'medousa_tui', '--bin', 'medousa'])
    text = m.run('test-artifact', ['cargo', 'test', '--locked', '--offline', '-p', 'medousa', '--no-default-features', '--features', 'full-daemon', '--lib', '--no-run', '--message-format=json'])
    artifacts = [json.loads(l) for l in text.splitlines() if l.startswith('{')]
    binaries = [a for a in artifacts if a.get('reason') == 'compiler-artifact' and a.get('executable') and a.get('target', {}).get('name') == 'medousa']
    assert len(binaries) == 1
    a = binaries[0]
    binary = a['executable']
    (HERE / 'binary.json').write_text(json.dumps({'artifact': a, 'sha256': hashlib.sha256(Path(binary).read_bytes()).hexdigest()}, indent=2) + '\n')
    text = m.run('lifetime', ['bash', 'scripts/ci/test-platform-lifetime.sh'])
    assert text.count('test result: ok. 1 passed; 0 failed; 0 ignored;') == 3
    selectors = [
        ('slim', 'grapheme_lsp_bridge::slim_tests::', 2),
        ('symlink', 'runtime::platform::lifetime_tests::fixture_root_resolves_symlink_ancestor_and_retains_owner', 1),
        ('drain', 'runtime::event_delivery::', 4),
        ('identity', 'ownership_tests::', 2),
        ('settings', 'runtime::settings::', 5),
        ('contracts', 'tool_contract_baseline::', 4),
        ('spine', 'agent_runtime::spine_byte_parity::', 7),
        ('catalog', 'grapheme_host_catalog::tests::', 4),
        ('grapheme', 'grapheme_runtime::tests::', 11),
        ('shell-workflow', 'grapheme_medousa_bridge::tests::workflow_shell_can_run_past_the_former_two_second_default', 1),
        ('worker-cancellation', 'grapheme_medousa_bridge::tests::dropped_engine_future_cancels_its_blocking_worker_but_completion_does_not', 1),
        ('secret-dispatch', 'grapheme_secret_bridge::tests::configured_grapheme_engine_dispatches_secrets_to_the_host_scope', 1),
        ('secret-workflow', 'grapheme_secret_bridge::tests::workflow_engine_consumes_internal_token_and_hides_secret_state', 1),
        ('http-authority', 'grapheme_secret_bridge::tests::authenticated_http_is_denied_before_network_for_unapproved_hosts', 1),
        ('http-engine-authority', 'grapheme_secret_bridge::tests::configured_engine_denies_unapproved_authorized_http_before_network', 1),
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
        ('rustfmt', ['rustfmt', '--edition', '2024', '--check', 'src/grapheme_lsp_bridge.rs']),
        ('diff-check', ['git', 'diff', '--check', '4e85141b32d6c74a25427e7506758b38c8683739']),
    ]:
        m.run(label, cmd)
    print('PASS fresh slim qualification: 59 unique Rust tests and 9 packaging tests')


if __name__ == '__main__':
    main()
