// SPDX-License-Identifier: Apache-2.0
// Opt-in, RAM-only host diagnostics. Never records input contents or app state.
export const timestamp=(performance)=>performance.timeOrigin+performance.now();
export function eventTimestamp(event,performance){
  const now=timestamp(performance),t=event?.timeStamp;
  return Number.isFinite(t)&&t>0?Math.min(now,t<1e12?performance.timeOrigin+t:t):now;
}
const phases=[['inputDelay','inputAt','queued'],['queue','queued','dispatched'],['workerDelivery','dispatched','workerStarted'],['workerDispatch','workerStarted','mountQueued'],['sendHandoff','mountQueued','mountSent'],['responseWait','mountSent','receivedAt'],['responseDelivery','receivedAt','workerReceived'],['qualification','workerReceived','qualified'],['frameBuild','qualified','frameReady'],['frameDelivery','frameReady','accepted'],['draw','accepted','drawn'],['presentation','drawn','presented']];
const rounded=n=>Math.round(n*10)/10;
export class NavigationTimings{
  constructor(performance,onReport=()=>{}){this.performance=performance;this.onReport=onReport;this.serial=0;this.pending=new Map([[0,{id:0,kind:'startup',inputAt:performance.timeOrigin}]]);this.samples=[];this.milestones={};this.drawing=null;}
  now(){return timestamp(this.performance);}
  mark(name){this.milestones[name]=rounded(this.now()-this.performance.timeOrigin);}
  input(event,downAt){const id=++this.serial;this.pending.set(id,{id,kind:'navigation',inputAt:eventTimestamp(event,this.performance),downAt,queued:this.now()});while(this.pending.size>32)this.pending.delete(this.pending.keys().next().value);return {id};}
  dispatched(timing){const sample=this.pending.get(timing?.id);if(sample)sample.dispatched=this.now();}
  sent(timing){const sample=this.pending.get(timing?.id);if(sample){Object.assign(sample,timing);sample.mountSent=this.now();}}
  accept(frame){const timing=frame.timing,sample=this.pending.get(timing?.id);if(!sample||!frame.vertices||!frame.ui||frame.epoch!==timing.epoch)return;Object.assign(sample,timing,{accepted:this.now(),screen:String(frame.screen).slice(0,64),uiCacheHits:frame.resources?.ui_cache_hits||0});this.drawing=sample;}
  drawn(epoch){if(this.drawing?.epoch!==epoch)return null;const sample=this.drawing;this.drawing=null;sample.drawn=this.now();return sample;}
  presented(sample,epoch){if(!sample||sample.epoch!==epoch||!this.pending.has(sample.id))return;sample.presented=this.now();this.pending.delete(sample.id);const ms={};for(const [name,a,b] of phases)if(Number.isFinite(sample[a])&&Number.isFinite(sample[b]))ms[name]=rounded(Math.max(0,sample[b]-sample[a]));const report={kind:sample.kind,screen:sample.screen,totalMs:rounded(sample.presented-sample.inputAt),ms,responseBytes:sample.responseBytes,uiCacheHits:sample.uiCacheHits};if(Number.isFinite(sample.downAt))report.downToRenderMs=rounded(sample.presented-sample.downAt);this.samples.push(report);if(this.samples.length>30)this.samples.shift();this.onReport(this.snapshot());}
  snapshot(){return {format:'otrios-navigation-timings-v1',completion:'UI and controls installed, GL draw submitted, next animation frame (not physical display latency)',milestonesMs:{...this.milestones},samples:this.samples.map(s=>({...s,ms:{...s.ms}}))};}
}
export function createTimingDiagnostics(options){
  if(new URL(options.location.href).searchParams.get('timings')!=='1')return null;
  const {performance,document,navigator}=options;
  const panel=document.createElement('details');panel.id='navigation-timings';
  const summary=document.createElement('summary');summary.textContent='Timings · starting';
  const description=document.createElement('p');description.textContent='Local timings only. Completion is the frame after drawing, not measured display latency. Copy or select the report to share it.';
  const report=document.createElement('textarea');report.id='timing-report';report.readOnly=true;report.setAttribute('aria-label','Navigation timing report');
  const copy=document.createElement('button');copy.type='button';copy.textContent='Copy report';copy.addEventListener('click',async()=>{try{await navigator.clipboard.writeText(report.value);copy.textContent='Copied';}catch{report.focus();report.select();copy.textContent='Select and copy report';}});
  panel.append(summary,description,copy,report);document.body.append(panel);
  return new NavigationTimings(performance,data=>{report.value=JSON.stringify(data,null,2);const last=data.samples.at(-1);summary.textContent=`Timings · ${last.kind==='startup'?'startup':last.screen} ${Math.round(last.totalMs)} ms`;});
}
