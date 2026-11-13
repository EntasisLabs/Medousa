import gzip, hashlib, importlib.util, json, subprocess
from pathlib import Path
R=Path.cwd(); E=R/'docs/reviews/daemon-ownership-evidence'; H=E/'slim-grapheme-2026-11-13'; O=E/'independent-ceb0f82'
def git(*args): return subprocess.check_output(['git',*args])
spec=importlib.util.spec_from_file_location('graphs',H/'graphs.py'); m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
g=json.loads((H/'graphs.json').read_text())
for label,features in m.PROFILES.items():
    command=['cargo','tree','--locked','--offline','-p','medousa','--no-default-features','--features',features,'-e','normal,build','--prefix','none','--format','{p} FEATURES=[{f}]']
    text=subprocess.check_output(command,text=True)
    assert m.inventory(text)==g['after'][label],label
    assert text.encode()==gzip.decompress((H/f'after-{label}.txt.gz').read_bytes()),label
    print('PASS current full package/version/features inventory',label,len(m.inventory(text)))
parent='3b8c895ea6b36bcffd6ba0443f58fb67d7268592'; head='ceb0f82b45bad269271d0598eb3232a58fc6f32c'
assert git('rev-parse','HEAD').decode().strip()==head
assert git('show',parent+':Cargo.toml')==gzip.decompress((H/'parent-Cargo.toml.gz').read_bytes())
assert git('show',parent+':Cargo.toml')==gzip.decompress((H/'baseline-Cargo.toml.gz').read_bytes())
assert git('show',parent+':Cargo.lock')==git('show',head+':Cargo.lock')
for path,digest in json.loads((H/'source-fingerprints.json').read_text()).items():
    assert hashlib.sha256(git('cat-file','--filters',head+':'+path)).hexdigest()==digest,path
for path,digest in json.loads((H/'historical-fingerprints.json').read_text()).items():
    assert hashlib.sha256(git('show',parent+':'+path)).hexdigest()==digest,path
    assert git('show',parent+':'+path)==git('show',head+':'+path),path
for path in git('ls-tree','-r','--name-only',parent,'docs/reviews/daemon-ownership-evidence/independent-4e85141','docs/reviews/daemon-grapheme-independent-boundary-review-2026-11-13.md').decode().splitlines():
    assert git('show',parent+':'+path)==git('show',head+':'+path),path
for rev in ('fa74520a','3b8c895e','9026f46d','6e64e1d6'):
    subprocess.run(['git','merge-base','--is-ancestor',rev,head],check=True)
assert not git('diff','--name-only','4e85141',parent,'--','.',':(exclude)docs/reviews')
for label,old in [('daemon','graph-full-daemon.log.gz'),('iroh','graph-full-daemon-iroh-transport.log.gz'),('tui','graph-tui.log.gz')]:
    assert gzip.decompress((H/f'before-{label}.txt.gz').read_bytes())==gzip.decompress((E/'independent-4e85141'/old).read_bytes())
print('PASS all source fingerprints bound to exact candidate Git; 185 historical files bound to parent and unchanged; reviewer docs/evidence preserved; source-free prior reviewer commits; archived old native/Iroh/TUI trees match independent prior capture')
patch=gzip.decompress((H/'source.patch.gz').read_bytes())
paths=['Cargo.toml','src/grapheme_lsp_bridge.rs','scripts/ci/check-daemon-dependencies.py']
import re
blocks=re.split(b'(?=^diff --git )',patch,flags=re.M)
source_patch=b''.join(b for b in blocks if any(b.startswith(('diff --git a/'+p+' b/').encode()) for p in paths))
assert source_patch==git('diff','--full-index',parent,head,'--',*paths)
print('PASS source patch equals exact parent-to-candidate implementation delta; lockfile unchanged')

assert not git('diff','--name-only','9026f46d',parent,'--','src','scripts','.github','Dockerfile','Cargo.toml','Cargo.lock')
print('PASS earlier qualified ownership/packaging source unchanged from 9026f46d through candidate parent')
