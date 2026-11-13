import os, pathlib, signal, subprocess, time, shutil, json
root=pathlib.Path.cwd(); out=root/'docs/reviews/daemon-ownership-evidence'
env=dict(os.environ, CARGO_BUILD_JOBS='2', CARGO_INCREMENTAL='0', MEDOUSA_TEST_HERMETIC='1')
filters=['runtime::event_delivery::','ownership_tests::','runtime::settings::','tool_contract_baseline::','agent_runtime::spine_byte_parity::','agent_runtime::turn_completion_fsm::']
receipts=[]
for i,f in enumerate(filters):
 cmd=['cargo','test','--locked','--offline','-p','medousa','--no-default-features','--features','full-daemon','--lib',f]
 start=time.monotonic(); initial=shutil.disk_usage(root).free; minimum=initial; stopped=False
 with (out/f'focused-{i+1}.log').open('w') as log:
  log.write('command: '+' '.join(cmd)+'\n');log.flush()
  p=subprocess.Popen(cmd,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
  while p.poll() is None:
   free=shutil.disk_usage(root).free; minimum=min(minimum,free)
   if free < 6*1024**3:
    stopped=True;os.killpg(p.pid,signal.SIGTERM)
    try:p.wait(timeout=10)
    except subprocess.TimeoutExpired:os.killpg(p.pid,signal.SIGKILL);p.wait()
    break
   time.sleep(2)
 receipt=dict(command=cmd,exit_code=p.wait(),seconds=round(time.monotonic()-start,2),initial_free_bytes=initial,minimum_free_bytes=minimum,stopped_at_disk_floor=stopped)
 receipts.append(receipt);(out/'focused-receipts.json').write_text(json.dumps(receipts,indent=2)+'\n');print(receipt,flush=True)
 if receipt['exit_code']!=0:break
