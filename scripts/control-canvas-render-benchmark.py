#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Drive the bounded remote renderer trials and sample local proxy counters."""
import argparse,pathlib,subprocess,json
p=argparse.ArgumentParser();p.add_argument("--ssh-target",required=True);p.add_argument("--known-hosts",type=pathlib.Path,required=True);p.add_argument("--output",type=pathlib.Path,required=True);a=p.parse_args()
work=a.output.resolve();work.mkdir(parents=True,exist_ok=True)
ssh=['ssh','-oBatchMode=yes','-oConnectTimeout=8','-oStrictHostKeyChecking=yes','-oUserKnownHostsFile='+str(a.known_hosts),a.ssh_target,'python3 /var/lib/otrios-dashboard/bench-adapter/run-render.py']
errors=open(work/'canvas-frame-render-errors.log','w')
p=subprocess.Popen(ssh,stdin=subprocess.PIPE,stdout=subprocess.PIPE,stderr=errors,text=True,bufsize=1)
before=None
with open(work/'canvas-frame-render-results.jsonl','w') as output:
 for line in p.stdout:
  event=json.loads(line);phase=event['phase']
  if phase=='start':
   before=json.loads((work/'canvas-bench-proxy-stats.json').read_text())['stats'][event['label']]
   print('START',event['repetition'],event['label'],flush=True)
   p.stdin.write('go\n');p.stdin.flush()
  elif phase=='end':
   after=json.loads((work/'canvas-bench-proxy-stats.json').read_text())['stats'][event['label']]
   event['native_wire']={key:after[key]-before[key] for key in ['up_binary','down_binary','up_ws','down_ws']}
   values=sorted(e['receive_to_present_us'] for e in event['present'][12:])
   def q(f):return values[min(len(values)-1,int(len(values)*f))] if values else None
   event['presentation_us']={'samples':len(values),'p50':q(.5),'p95':q(.95),'p99':q(.99),'max':max(values) if values else None}
   output.write(json.dumps(event)+'\n');output.flush()
   print('END',event['repetition'],event['label'],'CPU',round(event['native_cpu']['process_one_core_percent'],2),'present p95 us',q(.95),'down bytes',event['native_wire']['down_binary'],flush=True)
   p.stdin.write('go\n');p.stdin.flush()
  else:
   print(line.strip(),flush=True)
 if p.wait()!=0:raise RuntimeError('Native controller failed; inspect error log')
