from pathlib import Path
import hashlib,json
out=Path(__file__).resolve().parent; src=Path('apps/rndr-home'); evidence=src/'docs/split-evidence'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
for line in (evidence/'final/SOURCE_SHA256.txt').read_text().splitlines():
 h,p=line.split(None,1); assert sha(Path(p.strip()))==h,(h,p)
count=0
for line in (evidence/'SHA256SUMS').read_text().splitlines():
 h,p=line.split(None,1); assert sha(evidence/p.strip())==h,p; count+=1
comparisons=[]
for p in sorted(out.glob('*/*.png')):
 rel=p.relative_to(out); h=sha(p)
 comparisons.append({'path':str(rel),'sha256':h,'comparisons':{stage:sha(evidence/stage/rel)==h for stage in ['before','after','final']}})
 assert all(comparisons[-1]['comparisons'].values()),str(rel)
assert len(comparisons)==27
(out/'comparison.json').write_text(json.dumps(comparisons,indent=2)+'\n')
a=json.loads((out/'native-empty.json').read_text()); b=json.loads((evidence/'final/native-empty.json').read_text())
def changes(a,b,path=''):
 if isinstance(a,dict) and isinstance(b,dict):
  return [d for k in sorted(a.keys()|b.keys()) for d in changes(a.get(k),b.get(k),path+'/'+k)]
 if a!=b:return [{'path':path,'fresh':a,'architect':b}]
 return []
diffs=changes(a,b)
(out/'native-comparison.json').write_text(json.dumps(diffs,indent=2)+'\n')
result={'architect_evidence_hashes_matched':count,'fresh_exports_identical':len(comparisons),'native_differences':diffs}
print(json.dumps(result,indent=2)); (out/'integrity.json').write_text(json.dumps(result,indent=2)+'\n')
for name in ['default','observe']:
 print(name, '\n'.join(l for l in (out/f'{name}.log').read_text().splitlines() if 'test result:' in l))
r=json.loads((out/'run.json').read_text()); print('minimum sampled free bytes',min(min(c['minimum_free_bytes'],c['free_after_bytes']) for c in r['checks']))
print('Native capture log:',(out/'native.log').read_text())
