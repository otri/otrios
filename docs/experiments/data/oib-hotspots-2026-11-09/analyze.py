# SPDX-License-Identifier: Apache-2.0
# Usage: python3 analyze.py TRACE_DIR TRIALS_JSON OUTPUT_JSON
import json,collections,pathlib,sys
p=pathlib.Path(sys.argv[1])
worker=[json.loads(l)for l in (p/'native-worker.jsonl').read_text().splitlines()]
renderer=[json.loads(l)for l in (p/'native-renderer.jsonl').read_text().splitlines()]
server=[json.loads(l)for l in (p/'authority.jsonl').read_text().splitlines()]
trials=json.loads(pathlib.Path(sys.argv[2]).read_text())['records']
batches=[r for r in worker if r.get('event')=='core_phases']
offsets={r['report']['origin_unix_us']-r['origin_process_us']for r in batches};assert len(offsets)==1
offset=offsets.pop()
native_events=[]
for batch in batches:
 for event in batch['report']['events']:
  native_events.append(dict(event,pan_us=batch['origin_process_us']+event['start_ns']/1000))
server_events=[]
for report in server:
 for event in report.get('events',[]):
  server_events.append(dict(event,unix_us=report['origin_unix_us']+event['start_ns']/1000))
def summary(events):
 d=collections.defaultdict(lambda:{'calls':0,'inclusive_ms':0.,'exclusive_ms':0.})
 for e in events:
  v=d[e['phase']];v['calls']+=1;v['inclusive_ms']+=e['duration_ns']/1e6;v['exclusive_ms']+=e['self_ns']/1e6
 return dict(sorted(d.items(),key=lambda kv:kv[1]['exclusive_ms'],reverse=True))
results=[]
for trial in trials:
 start=trial['touch_down_unix_us']-offset
 downs=[r for r in worker if r.get('event')=='processed'and r['input']['phase']=='down']
 down=min(downs,key=lambda r:abs(r['input']['sampled_us']-start));assert abs(down['input']['sampled_us']-start)<20000
 up=next(r for r in worker if r.get('event')=='processed'and r['input']['phase']=='up'and r['input']['sequence']==down['input']['sequence']+1)
 sequence=up['input']['sequence']
 frame=next(r for r in worker if r.get('event')=='produced'and r.get('oib_active_generation')==trial['generation']and r.get('oib_presented_generation')==trial['generation']and r['confirmed']and r['pending']==0)
 present=next(r for r in renderer if r.get('event')=='present'and r['serial']==frame['serial'])
 offers=[r for r in worker if r.get('event')=='state_offer'and r['input']and r['input']['sequence']==sequence and up['ended_us']<=r['offered_us']<frame['produced_us']]
 offer=offers[0]
 receives=[r for r in worker if r.get('event')=='state_receive'and r['started_us']>=offer['offered_us']and r['ended_us']<=frame['produced_us']]
 first=receives[0]
 ns=[e for e in native_events if e['pan_us']>=down['dequeued_us']and e['pan_us']+e['duration_ns']/1000<=present['returned_us']]
 ss=[e for e in server_events if e['unix_us']>=offer['offered_us']+offset and e['unix_us']+e['duration_ns']/1000<=first['arrived_us']+offset+1000]
 segments={
 'touch_hold':(up['input']['sampled_us']-down['input']['sampled_us'])/1000,
 'up_input_queue':(up['dequeued_us']-up['input']['sampled_us'])/1000,
 'apply_handler':(up['ended_us']-up['dequeued_us'])/1000,
 'handler_to_offer':(offer['offered_us']-up['ended_us'])/1000,
 'offer_to_response_arrival':(first['arrived_us']-offer['offered_us'])/1000,
 'receive_queue':(first['started_us']-first['arrived_us'])/1000,
 'client_receive':sum((r['ended_us']-r['started_us'])/1000 for r in receives),
 'compose':(frame['compose_ended_us']-frame['compose_started_us'])/1000,
 'frame_metadata':(frame['produced_us']-frame['compose_ended_us'])/1000,
 'produced_to_renderer_prepare':(present['prepare_started_us']-frame['produced_us'])/1000,
 'renderer_prepare_upload':(present['draw_started_us']-present['prepare_started_us'])/1000,
 'gl_draw_submit':(present['draw_submitted_us']-present['draw_started_us'])/1000,
 'present_wait':(present['returned_us']-present['present_started_us'])/1000,
 }
 total=(present['returned_us']-down['input']['sampled_us'])/1000
 result={'color':trial['color'],'generation':trial['generation'],'serial':frame['serial'],'total_sample_to_present_ms':total,'poll_observed_ms':trial['touch_to_confirmed_presented_ms'],'segments_ms':segments,'unassigned_ms':total-sum(segments.values()),'native_phases':summary(ns),'authority_phases':summary(ss),'base_triangle_vertices':frame['base_triangle_vertices'],'base_glyph_vertices':frame['base_glyph_vertices'],'uploaded_vertex_bytes':present['host_vertex_upload_bytes']}
 results.append(result)
 print('TRIAL',trial['color'],'total',round(total,3));print(json.dumps(segments,indent=2))
 for label,events in [('native',result['native_phases']),('authority',result['authority_phases'])]:
  print(label,'top exclusive',json.dumps(dict(list(events.items())[:16]),indent=2))
pathlib.Path(sys.argv[3]).write_text(json.dumps({'format':'otrios-oib-hotspots-analysis-v1','native_pan_origin_unix_us':offset,'trials':results},indent=2)+'\n')
