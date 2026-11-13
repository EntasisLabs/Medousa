#!/usr/bin/env python3
"""Independent read-only bounded review; no Cargo compilation or upstream writes."""
import gzip, hashlib, io, json, os, pathlib, signal, subprocess, tarfile, time, tomllib
HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[3]
WORKER = ROOT/'docs/reviews/daemon-ownership-evidence/grapheme-boundary-2026-11-13'
CANDIDATE = '4e85141b32d6c74a25427e7506758b38c8683739'
def sha(b): return hashlib.sha256(b).hexdigest()
def bind():
    p = subprocess.Popen(['git','archive','--format=tar',CANDIDATE],cwd=ROOT,stdout=subprocess.PIPE)
    hashes = {}
    with tarfile.open(fileobj=p.stdout,mode='r|') as t:
        for m in t:
            if m.isfile():
                raw=t.extractfile(m).read(); assert (ROOT/m.name).read_bytes()==raw,m.name
                hashes[m.name]=sha(raw)
    assert p.wait()==0
    return hashes
receipts=[]
env=os.environ.copy();env.update(CARGO_BUILD_JOBS='2',CARGO_NET_OFFLINE='true',MEDOUSA_TEST_HERMETIC='1',PYTHONDONTWRITEBYTECODE='1')
def run(label,cmd):
    out=HERE/(label+'.log.gz');samples=[];stopped=False
    with __import__('tempfile').TemporaryFile() as f:
        p=subprocess.Popen(cmd,cwd=ROOT,env=env,stdout=f,stderr=subprocess.STDOUT,start_new_session=True)
        while True:
            free=__import__('shutil').disk_usage(ROOT).free;samples.append({'time':time.time(),'free_bytes':free})
            if free < 8*2**30 and p.poll() is None:
                stopped=True;os.killpg(p.pid,signal.SIGINT)
                try:p.wait(timeout=10)
                except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL)
            if p.poll() is not None: break
            time.sleep(2)
        f.seek(0);raw=f.read()
    out.write_bytes(gzip.compress(raw,mtime=0))
    receipts.append(dict(label=label,command=cmd,exit_code=p.returncode,samples=samples,min_free_bytes=min(s['free_bytes'] for s in samples),stopped=stopped,sha256=sha(raw)))
    (HERE/'receipts.json').write_text(json.dumps(receipts,indent=2)+'\n')
    assert not stopped and p.returncode==0,(label,p.returncode)
    print('PASS',label,flush=True);return raw

def main():
    assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()==CANDIDATE
    initial=bind();(HERE/'candidate-files.json').write_text(json.dumps(initial,indent=2,sort_keys=True)+'\n')
    run('environment',['sh','-c','rustc -Vv; cargo -V; uname -a; df -k .; command -v rg || true'])
    run('worker-auditor',['/opt/homebrew/bin/python3.11',str(WORKER/'verify.py')])
    graphs={}
    for profile in ['full-daemon','tui','full-daemon,iroh-transport']:
        raw=run('graph-'+profile.replace(',','-'),['cargo','tree','--locked','--offline','-p','medousa','--no-default-features','--features',profile,'-e','normal,build','--prefix','none','--format','{p} FEATURES=[{f}]'])
        assert raw==gzip.decompress((WORKER/('before-'+profile.replace(',','-')+'.txt.gz')).read_bytes())
        graphs[profile]=sha(raw)
    (HERE/'graph-hashes.json').write_text(json.dumps(graphs,indent=2)+'\n')
    for package in ['crossterm@0.29.0','grapheme-wasm','grapheme-compiler','wasmer-wasix']:
        run('inverse-'+package.replace('@','-').replace('.','-'),['cargo','tree','--locked','--offline','-p','medousa','--no-default-features','--features','full-daemon','-e','features,no-dev','-i',package])
    proof=json.loads((WORKER/'upstream-registry-checksums.json').read_text())
    source={};registry=pathlib.Path.home()/'.cargo/registry'
    for name,rec in proof.items():
        roots=list((registry/'src').glob('*/'+name));crates=list((registry/'cache').glob('*/'+name+'.crate'))
        assert len(roots)==len(crates)==1
        assert sha(crates[0].read_bytes())==rec['locked_crate_sha256']
        with tarfile.open(crates[0]) as t:
            for path,h in rec['checked_files'].items():
                assert sha((roots[0]/path).read_bytes())==h
                assert sha(t.extractfile(name+'/'+path).read())==h
        source[name]=len(rec['checked_files'])
    # Extend the prior worker's binding to the formatting implementation used in this review.
    extra={}
    for name,path in [('polars-core-0.46.0','src/fmt.rs'),('comfy-table-7.2.2','src/table.rs')]:
        root=next((registry/'src').glob('*/'+name));crate=next((registry/'cache').glob('*/'+name+'.crate'))
        raw=(root/path).read_bytes()
        with tarfile.open(crate) as t: assert raw==t.extractfile(name+'/'+path).read()
        extra[name+'/'+path]=sha(raw)
        (HERE/(name+'-'+path.split('/')[-1]+'.gz')).write_bytes(gzip.compress(raw,mtime=0))
    (HERE/'registry-proof.json').write_text(json.dumps({'checked_files':source,'extra_source_sha256':extra},indent=2)+'\n')
    b=json.loads((WORKER/'binary.json').read_text());assert sha(pathlib.Path(b['artifact']['executable']).read_bytes())==b['sha256']
    (HERE/'binary-proof.json').write_text(json.dumps({'path':b['artifact']['executable'],'sha256':b['sha256'],'rebuilt':False,'executed_by_this_review':False},indent=2)+'\n')
    run('packaging',['/opt/homebrew/bin/python3.11','scripts/ci/test-daemon-packaging.py'])
    run('ownership-equivalent',['/opt/homebrew/bin/python3.11','docs/reviews/daemon-ownership-evidence/harness-correction-2026-11-13/check-runtime-ownership.py'])
    assert initial==bind()
    print('PASS unchanged candidate',len(initial),'files; upstream',sum(source.values()),'files; no Rust tests/rebuild by this review')
if __name__=='__main__':main()
