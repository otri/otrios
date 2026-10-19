#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Parse captured DRM page-flip completions; no device or service access."""
import collections,re,struct

def flip_events(text):
    events=[]
    for line in text.splitlines():
        match=re.search(r'read\(\d+, "((?:\\x[0-9a-f]{2})+)".*?= (\d+)',line)
        if not match:continue
        data=bytes.fromhex(match[1].replace('\\x',''));at=0
        while at+8<=len(data):
            kind,size=struct.unpack_from('<II',data,at)
            if size<8 or at+size>len(data):break
            if kind==2 and size==32:
                sec,usec,sequence,crtc=struct.unpack_from('<IIII',data,at+16)
                if usec<1_000_000:events.append((sec+usec/1e6,sequence,crtc))
            at+=size
    return events

def flip_summary(events,warmup=2.):
    if len(events)<3:raise ValueError('Insufficient DRM page-flip completion events')
    crtc=collections.Counter(e[2] for e in events).most_common(1)[0][0]
    events=[e for e in events if e[2]==crtc];begin=events[0][0]
    events=[e for e in events if e[0]>=begin+warmup]
    if len(events)<3:raise ValueError('Insufficient page flips after warm-up')
    intervals=[(b[0]-a[0])*1000 for a,b in zip(events,events[1:])]
    if min(intervals)<=0:raise ValueError('Non-monotonic flip timestamps')
    ordered=sorted(intervals);p=lambda f:ordered[round((len(ordered)-1)*f)]
    return {'measurement':'kernel DRM_EVENT_FLIP_COMPLETE timestamps from DRM reads','warmup_seconds':warmup,'crtc':crtc,'events':len(events),'seconds':events[-1][0]-events[0][0],'presented_fps':(len(events)-1)/(events[-1][0]-events[0][0]),'p50_ms':p(.5),'p95_ms':p(.95),'p99_ms':p(.99),'max_ms':max(intervals),'missed_vblanks':sum(max(0,((b[1]-a[1])&0xffffffff)-1)for a,b in zip(events,events[1:]))}

