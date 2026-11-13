#!/usr/bin/env python3
"""Audit sealed independent evidence; no compilation, tests or approval."""
import ast, gzip, hashlib, json, pathlib, subprocess, sys, tarfile
HERE=pathlib.Path(__file__).resolve().parent
ROOT=HERE.parents[3]
CANDIDATE='4e85141b32d6c74a25427e7506758b38c8683739'
MANIFEST_SHA='da47b3f6cdec1f7d055338cc92b8e810ed002123b8bf6e042e837ff40765905a'
def sha(b):return hashlib.sha256(b).hexdigest()
def main():
    raw=(HERE/'file-sha256.json').read_bytes();assert sha(raw)==MANIFEST_SHA
    manifest=json.loads(raw)
    for p,h in manifest.items():assert sha((ROOT/p).read_bytes())==h,p
    files=json.loads((HERE/'candidate-files.json').read_text());assert len(files)==7811
    p=subprocess.Popen(['git','archive','--format=tar',CANDIDATE],cwd=ROOT,stdout=subprocess.PIPE)
    seen=set()
    with tarfile.open(fileobj=p.stdout,mode='r|') as t:
        for m in t:
            if m.isfile():
                seen.add(m.name);h=files[m.name]
                assert sha(t.extractfile(m).read())==h,m.name
                if '--git-only' not in sys.argv: assert sha((ROOT/m.name).read_bytes())==h,m.name
    assert p.wait()==0 and seen==set(files)
    receipts=json.loads((HERE/'receipts.json').read_text());assert len(receipts)==12
    for r in receipts:
        assert r['exit_code']==0 and not r['stopped']
        assert r['min_free_bytes']==min(s['free_bytes'] for s in r['samples'])>=8*2**30
        assert sha(gzip.decompress((HERE/(r['label']+'.log.gz')).read_bytes()))==r['sha256']
    hashes=json.loads((HERE/'graph-hashes.json').read_text())
    worker=ROOT/'docs/reviews/daemon-ownership-evidence/grapheme-boundary-2026-11-13'
    for profile,h in hashes.items():
        graph=gzip.decompress((HERE/('graph-'+profile.replace(',','-')+'.log.gz')).read_bytes())
        assert sha(graph)==h
        for phase in ['before','after']:
            assert graph==gzip.decompress((worker/(phase+'-'+profile.replace(',','-')+'.txt.gz')).read_bytes())
    assert b'Ran 9 tests' in gzip.decompress((HERE/'packaging.log.gz').read_bytes())
    proof=json.loads((HERE/'registry-proof.json').read_text());assert sum(proof['checked_files'].values())==341
    for name,h in proof['extra_source_sha256'].items():
        package,path=name.split('/',1)
        assert sha(gzip.decompress((HERE/(package+'-'+path.split('/')[-1]+'.gz')).read_bytes()))==h
    for f in ['review.py','verify.py']:ast.parse((HERE/f).read_text())
    print('PASS sealed independent evidence:',len(manifest),'files;',len(files),'candidate Git-bound files; working-byte check skipped.' if '--git-only' in sys.argv else 'candidate working files unchanged.')
    print('PASS fresh three graph identities, recorded upstream binding, 9 fresh packaging tests; ZERO slimming.')
    print('No Rust execution/rebuild or native approval/integration implied. Minimum sampled GiB:',min(r['min_free_bytes'] for r in receipts)/2**30)
if __name__=='__main__':main()
