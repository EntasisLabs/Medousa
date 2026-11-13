"""Run only the existing qualified test executable; never invoke a compiler."""
import hashlib,json,os,pathlib,re,subprocess,time
out=pathlib.Path('docs/reviews/daemon-ownership-evidence')
log=(out/'focused-1.log').read_text()
binary=pathlib.Path(re.search(r'Running unittests src/lib.rs \(([^)]+)\)',log).group(1)).resolve()
filters=['tool_contract_baseline::','agent_runtime::spine_byte_parity::','agent_runtime::turn_completion_fsm::','grapheme_host_catalog::tests::','grapheme_runtime::tests::']
results=[]
for i,selection in enumerate(filters):
 command=[str(binary),selection,'--nocapture']
 start=time.monotonic()
 p=subprocess.run(command,env=dict(os.environ,MEDOUSA_TEST_HERMETIC='1'),stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
 (out/f'existing-{i+1}.log').write_text('command: '+' '.join(command)+'\n'+p.stdout)
 receipt=dict(command=command,exit_code=p.returncode,seconds=round(time.monotonic()-start,2))
 results.append(receipt);print(receipt,flush=True)
 if p.returncode:break
(out/'existing-receipts.json').write_text(json.dumps({'binary':str(binary),'sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'results':results},indent=2)+'\n')
