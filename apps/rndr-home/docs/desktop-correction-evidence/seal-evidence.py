#!/usr/bin/env python3
"""Audit prior evidence and seal exact source/additive bounded receipts. Run at repo root."""
from pathlib import Path
import hashlib, json, re, struct, subprocess
root=Path(subprocess.check_output(['git','rev-parse','--show-toplevel'],text=True).strip())
here=root/'apps/rndr-home/docs/desktop-correction-evidence'
base='b5b2fa9cb116cd8cd25275d61a28a15b06abeb80'
source='542fe2dc98582a628e93c7f9aa46cb41cce3e786'
reviewed='97e0c223bf1d1016b2e8b440e7e924fd9716ce7c'
def git(*args): return subprocess.check_output(['git',*args],cwd=root)
def sha(data): return hashlib.sha256(data).hexdigest()
def files_at(head,path): return git('ls-tree','-r','--name-only',head,'--',path).decode().splitlines()
assert git('rev-parse',source+'^').decode().strip()==base
assert git('rev-parse',base+'^').decode().strip()==reviewed
changed=git('diff','--name-only',base,source).decode().splitlines()
assert all(p.startswith('apps/rndr-home/src/') or p=='apps/rndr-home/docs/DESKTOP_PARITY.md' for p in changed)
prior={}
for p in files_at(base,'apps/rndr-home/docs'):
    if p=='apps/rndr-home/docs/DESKTOP_PARITY.md': continue # authorized parity clarification
    original=git('show',base+':'+p)
    assert (root/p).read_bytes()==original, 'prior documentation/evidence changed: '+p
    prior[p]=sha(original)
source_files=files_at(source,'apps/rndr-home/src')+[
 'apps/rndr-home/Cargo.toml','apps/rndr-home/Cargo.lock',
 'Cargo.toml','Cargo.lock','apps/rndr-home/docs/DESKTOP_PARITY.md']
source_hashes={}
for p in source_files:
    expected=git('show',source+':'+p)
    assert (root/p).read_bytes()==expected, 'tested source changed: '+p
    source_hashes[p]=sha(expected)
old=(root/'apps/rndr-home/docs/desktop-independent-review/regression-module.rs').read_text()
new=(root/'apps/rndr-home/src/desktop_preservation_tests.rs').read_text()
promoted=new[new.index('#[cfg(test)]'):new.index('#[cfg(test)]\nmod correction_tests')]
def normalized(text):
    # rustfmt adds whitespace/trailing commas and a block around the long map
    # expression. Preserve all identifiers/literals/operators and assertions.
    compact = re.sub(r",(?=[)}\]])", "", ''.join(text.split()))
    return re.sub(r"\.map\(\|i\|\{(Record::new\(.*?)\}\)\.collect", r".map(|i|\1).collect", compact)
assert normalized(old)==normalized(promoted), 'review reproduction was substantively changed'
results=dict(line.split('\t') for line in (here/'results.tsv').read_text().splitlines())
assert len(results)==9 and set(results.values())=={'0'}
assert (here/'docs-final.exit').read_text().strip()=='0'
assert (here/'focused-final.exit').read_text().strip()=='0'
assert (here/'fail-before.exit').read_text().strip()=='101'
pngs={}
for p in sorted((here/'renders').glob('*.png')):
    b=p.read_bytes(); assert b[:8]==b'\x89PNG\r\n\x1a\n'
    size=list(struct.unpack('>II',b[16:24])); assert size in [[1120,820],[480,760],[1120,650],[480,650]]
    pngs[p.name]={'sha256':sha(b),'size':size}
assert len(pngs)==216
receipts={str(p.relative_to(root)):sha(p.read_bytes()) for p in sorted(here.rglob('*')) if p.is_file() and p.name!='seal.json'}
seal={'baseline':base,'independently_reviewed_source':reviewed,'source_commit':source,
 'source_tree':git('rev-parse',source+'^{tree}').decode().strip(),
 'source_changed_paths':changed,'source_sha256':source_hashes,
 'prior_documents_unchanged_count':len(prior),'prior_documents_sha256':prior,
 'promoted_reviewer_tests_formatting_only_changes':True,'final_gates':results,
 'final_counts':{'focused':9,'default_library':6,'default_app':71,'observe_library':6,'observe_app':79},
 'renders':pngs,'evidence_sha256':receipts,
 'qualification':'Synthetic preview correction for independent verification; no implementation acceptance or integration.'}
(here/'seal.json').write_text(json.dumps(seal,indent=2,sort_keys=True)+'\n')
print('sealed source',source,'prior unchanged',len(prior),'PNG count',len(pngs),'receipts',len(receipts))
