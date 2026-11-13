#!/usr/bin/env python3
"""Independent rerun; no production changes, host temp inherited, serialized cache use."""
import importlib.util, pathlib, json, hashlib, tarfile, os, subprocess
HERE=pathlib.Path(__file__).resolve().parent
ROOT=HERE.parents[3]
spec=importlib.util.spec_from_file_location('bounded', ROOT/'docs/reviews/daemon-ownership-evidence/harness-correction-2026-11-13/qualify.py')
m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
m.HERE=HERE
assert 'RUST_MIN_STACK' not in m.ENV
(HERE/'environment.json').write_text(json.dumps({'HEAD':subprocess.check_output(['git','rev-parse','HEAD']).decode().strip(), 'TMPDIR':m.ENV.get('TMPDIR'),'real_tmp':os.path.realpath(m.ENV['TMPDIR']),'stack':m.ENV.get('RUST_MIN_STACK'),'jobs':m.ENV['CARGO_BUILD_JOBS'],'offline':m.ENV['CARGO_NET_OFFLINE']},indent=2)+'\n')
with tarfile.open(ROOT/'docs/reviews/daemon-ownership-evidence/harness-correction-2026-11-13/evidence.tar.gz') as t:
    member=next(x for x in t.getmembers() if x.name.endswith('/source-fingerprints.json'))
    sources=json.loads(t.extractfile(member).read())
assert len(sources)==1083
for p,h in sources.items(): assert hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h,p
(HERE/'source-fingerprints.json').write_text(json.dumps(sources,indent=2,sort_keys=True)+'\n')
m.run('provenance',['sh','-c','rustc -Vv; cargo -V; rustup show active-toolchain; uname -a; df -k .; git status --short; git log -4 --oneline; cat .cargo/config.toml'])
text=m.run('committed-lifetime',['bash','scripts/ci/test-platform-lifetime.sh'])
assert text.count('test result: ok. 1 passed; 0 failed; 0 ignored;')==3
text=m.run('cargo-artifact',['cargo','test','--locked','--offline','-p','medousa','--no-default-features','--features','full-daemon','--lib','--no-run','--message-format=json'])
artifacts=[json.loads(l) for l in text.splitlines() if l.startswith('{')]
a=[a for a in artifacts if a.get('reason')=='compiler-artifact' and a.get('executable') and a.get('target',{}).get('name')=='medousa']
assert len(a)==1
binary=a[0]['executable']; digest=hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest()
(HERE/'binary.json').write_text(json.dumps({'artifact':a[0],'sha256':digest},indent=2)+'\n')
assert a[0]['fresh'], 'expected cached current artifact'
assert digest=='c54a858343716ae93208de944c7ff66cf8019c88c5f4b0c2f04655d43cd7bc03'
selectors=[('symlink','runtime::platform::lifetime_tests::fixture_root_resolves_symlink_ancestor_and_retains_owner',1),('drain','runtime::event_delivery::',4),('identity','ownership_tests::',2),('settings','runtime::settings::',5),('contracts','tool_contract_baseline::',4),('spine','agent_runtime::spine_byte_parity::',7),('catalog','grapheme_host_catalog::tests::',4),('grapheme','grapheme_runtime::tests::',11),('recovery','engine_recovery::tests::recovery_replays_uncommitted_journal_and_marks_committed',1),('boundary','daemon::request_boundary::tests::',9)]
for label,selector,count in selectors:
    text=m.run(label,[binary,selector,'--test-threads=1']+(['--exact'] if label=='symlink' else []))
    assert f'test result: ok. {count} passed; 0 failed; 0 ignored;' in text
for label,command in [('packaging',['python3','scripts/ci/test-daemon-packaging.py']),('graph',['python3','scripts/ci/check-daemon-dependencies.py','--offline']),('worker-audit',['python3','docs/reviews/daemon-ownership-evidence/harness-correction-2026-11-13/verify.py']),('ownership-python',['python3','docs/reviews/daemon-ownership-evidence/harness-correction-2026-11-13/check-runtime-ownership.py']),('syntax',['bash','-n','scripts/ci/test-platform-lifetime.sh']),('diff-check',['git','diff','--check','71a149b3','9026f46'])]:
    m.run(label,command)
for p,h in sources.items(): assert hashlib.sha256((ROOT/p).read_bytes()).hexdigest()==h,p
print('PASS independent rerun: 51 unique Rust tests, 9 packaging tests; source and binary bound',flush=True)
