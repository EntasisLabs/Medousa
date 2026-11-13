#!/usr/bin/env python3
"""Offline independent review harness; no tracked source edits. Fresh caches per source."""
from pathlib import Path
import os, shutil, subprocess
root = Path(subprocess.check_output(['git', 'rev-parse', '--show-toplevel'], text=True).strip())
here = Path(__file__).resolve().parent
scratch = root / '.correction-independent-repro'
if scratch.exists():
    raise SystemExit('Refusing existing scratch path: ' + str(scratch))
scratch.mkdir()
env = dict(os.environ, CARGO_NET_OFFLINE='true', CARGO_INCREMENTAL='0',
           CARGO_PROFILE_DEV_DEBUG='0', CARGO_PROFILE_TEST_DEBUG='0', RUST_TEST_THREADS='2')
# This executes the audited scoped fmt/tests/clippy/build/export/docs script;
# it removes only its own full-suite target on success.
subprocess.run(['sh', str(root / 'apps/rndr-home/docs/desktop-correction-evidence/reproduce.sh')],
               cwd=root, env=dict(env, OUT=str(scratch/'full'), TARGET=str(scratch/'full-target')), check=True)
for name, rev in [('before', 'b5b2fa9cb116cd8cd25275d61a28a15b06abeb80'),
                  ('after', '542fe2dc98582a628e93c7f9aa46cb41cce3e786')]:
    h = scratch / name
    (h/'src').mkdir(parents=True)
    files = subprocess.check_output(['git','ls-tree','-r','--name-only',rev,'apps/rndr-home/src'],text=True).splitlines()
    for f in files:
        (h/'src'/Path(f).name).write_bytes(subprocess.check_output(['git','show',rev+':'+f]))
    for f in ['Cargo.toml','Cargo.lock']:
        text = subprocess.check_output(['git','show',rev+':apps/rndr-home/'+f],text=True)
        if f == 'Cargo.toml':
            text = text.replace('"../../crates/', '"'+str(root)+'/crates/')
        (h/f).write_text(text)
    p = h/'src/main.rs'
    original = root/'apps/rndr-home/docs/desktop-independent-review/regression-module.rs'
    p.write_text(p.read_text()+original.read_text())
    if name == 'after':
        p.write_text(p.read_text()+(here/'extra.rs').read_text()+(here/'pixel-probe.rs').read_text())
    target = scratch/(name+'-target')
    checks = [('independent_review_tests', 101 if name=='before' else 0, 2 if name=='before' else 4)]
    if name == 'after':
        checks += [('extra_review_tests',0,3),('desktop_preservation_tests',0,9),('pixel_probe',0,1)]
    for filt, expected_exit, expected_count in checks:
        cmd = ['cargo','test','--manifest-path',str(h/'Cargo.toml'),'--target-dir',str(target),'-j','2',filt]
        if filt == 'pixel_probe':
            cmd += ['--','--test-threads=1']
        result = subprocess.run(cmd,cwd=root,env=dict(env,REVIEW_PIXEL_OUT=str(scratch/'edge-renders')),
                                stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
        (scratch/(name+'-'+filt+'.log')).write_text(result.stdout)
        status = 'FAILED' if expected_exit else 'ok'
        passed, failed = (0,expected_count) if expected_exit else (expected_count,0)
        needle = f'test result: {status}. {passed} passed; {failed} failed;'
        if result.returncode != expected_exit or needle not in result.stdout:
            raise SystemExit(f'Unexpected exit/count: {name} {filt}: {result.returncode}; see log')
    # Never share package artifacts between copied source roots. Earlier aliasing
    # produced stale failures/zero-test successes; neither is an accepted result.
    shutil.rmtree(target)
print('Independent reproduction complete; logs/PNGs in '+str(scratch))
print('Own Cargo targets removed. Remove this scratch directory after collecting evidence.')
