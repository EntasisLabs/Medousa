#!/usr/bin/env python3
"""Review-only harness. Copies app source; never edits tracked implementation.
Run from the repository root. Creates .desktop-review-repro and its bounded target.
Exit 101 is expected on the reviewed head: two preservation assertions fail.
"""
from pathlib import Path
import os,shutil,subprocess
root=Path(subprocess.check_output(['git','rev-parse','--show-toplevel'],text=True).strip())
here=Path(__file__).resolve().parent
scratch=root/'.desktop-review-repro'
if scratch.exists(): raise SystemExit('Refusing existing scratch directory; move/remove your own previous review run first')
h=scratch/'harness'; shutil.copytree(root/'apps/rndr-home/src',h/'src')
s=(root/'apps/rndr-home/Cargo.toml').read_text().replace('"../../crates/','"'+str(root)+'/crates/')
(h/'Cargo.toml').write_text(s); shutil.copy(root/'apps/rndr-home/Cargo.lock',h/'Cargo.lock')
p=h/'src/main.rs'; p.write_text(p.read_text()+(here/'regression-module.rs').read_text())
env=dict(os.environ,CARGO_NET_OFFLINE='true',CARGO_INCREMENTAL='0',CARGO_PROFILE_DEV_DEBUG='0',CARGO_PROFILE_TEST_DEBUG='0')
target=Path(os.environ.get('REVIEW_TARGET',str(scratch/'target'))).resolve()
command=['cargo','test','--manifest-path',str(h/'Cargo.toml'),'--target-dir',str(target),'-j','2','independent_review_tests']
print('Review harness only; remove '+str(scratch)+' after collecting results',flush=True)
raise SystemExit(subprocess.call(command,cwd=root,env=env))
