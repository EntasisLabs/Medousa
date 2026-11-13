import ast, hashlib, importlib.util, json, os
from pathlib import Path
ROOT=Path.cwd()
HERE=ROOT/'docs/reviews/daemon-ownership-evidence/independent-ceb0f82'
spec=importlib.util.spec_from_file_location('monitor',ROOT/'docs/reviews/daemon-ownership-evidence/harness-correction-2026-11-13/qualify.py')
m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
m.HERE=HERE; m.ENV['CARGO_BUILD_JOBS']='1'
a=json.loads((HERE.parent/'slim-grapheme-2026-11-13/binary.json').read_text())
binary=a['artifact']['executable']
assert hashlib.sha256(Path(binary).read_bytes()).hexdigest()==a['sha256']
(HERE/'binary.json').write_text(json.dumps(a,indent=2)+'\n')
m.run('provenance',['sh','-c','git rev-parse HEAD; git status --short; rustc -Vv; df -k .'])
mod=ast.parse((HERE.parent/'slim-grapheme-2026-11-13/qualify.py').read_text())
selectors=next(ast.literal_eval(n.value) for n in ast.walk(mod) if isinstance(n,ast.Assign) and any(isinstance(t,ast.Name) and t.id=='selectors' for t in n.targets))
for label,selector,count in selectors:
    text=m.run(label,[binary,selector,'--test-threads=1']+(['--exact'] if count==1 else []))
    assert f'test result: ok. {count} passed; 0 failed; 0 ignored;' in text
m.ENV['RUST_MIN_STACK']='33554432'
for i,name in enumerate(['retained_agent_survives_final_platform_drop_without_observation_failure','late_actual_constructor_failure_closes_observations_after_handler_registration','polled_actual_constructor_cancellation_closes_observations_after_handler_registration']):
    text=m.run('lifetime-'+str(i),[binary,'runtime::platform::lifetime_tests::'+name,'--ignored','--exact','--test-threads=1'])
    assert 'test result: ok. 1 passed; 0 failed; 0 ignored;' in text
m.run('route-policy',[binary,'daemon::route_policy::tests::','--test-threads=1'])
m.run('route-inventory',[binary,'daemon::router::tests::workflow_and_grapheme_inventory_separates_execution_from_authoring','--exact','--test-threads=1'])
for label,cmd in [('packaging',['python3','scripts/ci/test-daemon-packaging.py']),('graph-guard',['python3','scripts/ci/check-daemon-dependencies.py','--offline']),('source-binding',['python3',str(HERE.parent/'slim-grapheme-2026-11-13/verify.py')])]:
    m.run(label,cmd)
print('PASS authenticated cached executable; 59 focused tests rerun plus route tests')
