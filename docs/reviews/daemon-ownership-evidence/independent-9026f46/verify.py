#!/usr/bin/env python3
"""Noncompiling independent evidence audit, not a rerun or approval."""
import hashlib, io, json, pathlib, re, subprocess, tarfile
H=pathlib.Path(__file__).resolve().parent
R=H.parents[3]
C='9026f46d786460c1cad7178fa804876cd6000fb6'
S='c0d47829f11f643044b278a8694c4c9023e867ca'
def sha(b):return hashlib.sha256(b).hexdigest()
blob=(H/'evidence.tar.gz').read_bytes()
assert sha(blob)=='fc1ce4e3a104867c5a2a1b36c5d8377094b7f7be48ad6b6d0ca5083897abee77'
with tarfile.open(fileobj=io.BytesIO(blob),mode='r:gz') as t:
 f={pathlib.Path(m.name).name:t.extractfile(m).read() for m in t.getmembers() if m.isfile()}
manifest=json.loads(f['file-sha256.json'])
assert set(manifest)==set(f)-{'file-sha256.json'}
for n,h in manifest.items():
 assert sha(f[n])==h,n
 assert (H/n).read_bytes()==f[n],n
assert f['report.md']==(R/'docs/reviews/daemon-ownership-independent-harness-rereview-2026-11-13.md').read_bytes()
sources=json.loads(f['source-fingerprints.json']);assert len(sources)==1083
for p,h in sources.items():
 assert sha((R/p).read_bytes())==h,p
 assert sha(subprocess.check_output(['git','show',S+':'+p],cwd=R))==h,p
prior=json.loads(f['candidate-files.json']);assert len(prior)==65
for p,h in prior.items():
 assert sha((R/p).read_bytes())==h,p
 assert sha(subprocess.check_output(['git','show',C+':'+p],cwd=R))==h,p
assert f['source.patch']==subprocess.check_output(['git','diff','--binary','71a149b3',S],cwd=R)
env=json.loads(f['environment.json']);assert env['HEAD']==C and env['stack'] is None
assert env['TMPDIR'].startswith('/var/folders/') and env['real_tmp'].startswith('/private/var/folders/')
assert env['jobs']=='2' and env['offline']=='true'
receipts=json.loads(f['receipts.json']); by={r['label']:r for r in receipts}
assert len(by)==len(receipts)==19
for r in receipts:
 assert r['exit_code']==r['expected_exit_code']==0 and not r['stopped_at_8GiB'],r['label']
 assert r['min_free_bytes']==min(s['free_bytes'] for s in r['samples']) and r['min_free_bytes']>8*2**30
l=f['committed-lifetime.log']
assert by['committed-lifetime']['command']==['bash','scripts/ci/test-platform-lifetime.sh']
assert l.count(b'test result: ok. 1 passed; 0 failed; 0 ignored;')==l.count(b'running 1 test')==3
names=['symlink','drain','identity','settings','contracts','spine','catalog','grapheme','recovery','boundary']
counts=[1,4,2,5,4,7,4,11,1,9]
for label,count in zip(names,counts):
 assert len(by[label]['summaries'])==1
 assert re.search(fr'ok\. {count} passed; 0 failed; 0 ignored;',by[label]['summaries'][0]),label
assert sum(counts)+3==51
assert b'Ran 9 tests' in f['packaging.log'] and f['packaging.log'].rstrip().endswith(b'OK')
b=json.loads(f['binary.json'])
assert b['sha256']=='c54a858343716ae93208de944c7ff66cf8019c88c5f4b0c2f04655d43cd7bc03'
a=b['artifact'];assert a['fresh'] and a['features']==['full-daemon'] and a['profile']['test']
assert a['target']['src_path']==str(R/'src/lib.rs')
assert pathlib.Path(a['executable']).name.encode() in l
assert b'803 unique package names' in f['graph.log'] and b'881 unique package names' in f['graph.log']
assert b'781 Rust files' in f['ownership-python.log']
print(f'PASS independent evidence: {len(manifest)} archived files; 1083 source fingerprints; 65 candidate evidence/report files unchanged.')
print('51 unique Rust and 9 packaging tests recorded as freshly executed; authenticated cached binary, not rebuilt by reviewer. No acceptance/integration implied.')
