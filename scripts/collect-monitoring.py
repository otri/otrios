#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Read an allowlisted Prometheus HTTPS window; publish one immutable FlexBuffers view.
Credentials stay in a private service file; browser and display clients receive only the view.
"""
import argparse
import base64
import datetime
import json
import pathlib
import subprocess
import time
import urllib.request
import urllib.parse

QUERIES = {
    'prod_health': 'up{job="monocle-gallery-production"}',
    'rc_health': 'up{job="monocle-gallery-rc"}',
    'staging_health': 'up{job="monocle-gallery-staging"}',
    'wordpress_health': 'up{job="wordpress-blog"}',
    'bundles': 'bundle_count{job="monocle-gallery-production"}',
    'memory': 'memory_usage_heap_used{job="monocle-gallery-production"}',
    'workers': 'wordpress_apache_busy_workers{job="wordpress-blog"}',
    'posts_en': 'wordpress_posts_total{job="wordpress-blog",lang="en"}',
    'posts_ja': 'wordpress_posts_total{job="wordpress-blog",lang="ja"}',
    'backup_days': 'wordpress_backup_days_since_last{job="wordpress-blog"}',
    'database': 'wordpress_database_up{job="wordpress-blog"}',
}

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, request, response, code, message, headers, newurl):
        raise ValueError('Prometheus redirects are not allowed')

def collect(config):
    base=config['prometheus_url'].rstrip('/')
    parsed=urllib.parse.urlsplit(base)
    if parsed.scheme!='https' or not parsed.hostname or parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise ValueError('Expected a Prometheus HTTPS URL without embedded credentials')
    credentials=pathlib.Path(config['prometheus_credentials'])
    if credentials.stat().st_mode & 0o077:
        raise ValueError('Prometheus credentials must be private (mode 600)')
    auth=json.loads(credentials.read_text())
    if not isinstance(auth.get('username'),str) or ':' in auth['username'] or not isinstance(auth.get('password'),str):
        raise ValueError('Invalid Prometheus credentials')
    token=base64.b64encode((auth['username']+':'+auth['password']).encode()).decode()
    opener=urllib.request.build_opener(NoRedirect())
    generated=int(time.time());end=generated//3600*3600;start=end-7*86400
    result={'start':start,'end':end,'step':3600,'generated':generated,'queries':{},'current':{}}
    deadline=time.monotonic()+180
    def fetch(endpoint,params):
        remaining=deadline-time.monotonic()
        if remaining<=0:raise TimeoutError('Prometheus window collection timed out')
        url=base+'/api/v1/'+endpoint+'?'+urllib.parse.urlencode(dict(params,timeout='15s'))
        request=urllib.request.Request(url,headers={'Authorization':'Basic '+token,'Accept':'application/json'})
        with opener.open(request,timeout=min(25,remaining)) as response:raw=response.read(2*1024*1024+1)
        if len(raw)>2*1024*1024:raise ValueError('Prometheus response too large')
        data=json.loads(raw)
        if data.get('status')!='success':raise ValueError('Prometheus query failed')
        return data
    for key,query in QUERIES.items():
        ranged='min_over_time('+query+'[1h])' if key.endswith('_health') else ('avg_over_time('+query+'[1h])' if key=='memory' else query)
        result['queries'][key]=fetch('query_range',dict(query=ranged,start=start,end=end,step=3600))
        result['current'][key]=fetch('query',dict(query=query,time=generated))
    for key,timestamp in [('collected_label',generated),('start_label',start),('end_label',end)]:
        result[key]=datetime.datetime.fromtimestamp(timestamp,datetime.timezone.utc).strftime('%b %d %H:%M UTC')
    raw=json.dumps(result,separators=(',',':')).encode()
    if len(raw)>2*1024*1024:raise ValueError('Monitoring window too large')
    return raw

def refresh(config):
    raw=collect(config)
    output=pathlib.Path(config['output'])
    temporary=output.with_suffix('.pending.flex')
    subprocess.run([config['renderer'],str(temporary)],input=raw,check=True,timeout=30)
    payload=temporary.read_bytes()
    # Only the loopback OTriOS service can be a publication destination.
    request=urllib.request.Request('http://127.0.0.1:8876/presentation',data=payload,
                                  headers={'Content-Type':'application/octet-stream'},method='POST')
    with urllib.request.urlopen(request,timeout=15) as response:
        revision=response.read(256).decode()
    temporary.replace(output)
    if config.get('evidence'):
        pathlib.Path(config['evidence']).write_bytes(raw)
    stamp=json.loads(raw)['collected_label']
    print(f'PUBLISHED monitoring {revision} {len(payload)} bytes / collected {stamp}',flush=True)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--config',type=pathlib.Path,required=True)
    parser.add_argument('--watch',action='store_true')
    args=parser.parse_args()
    config=json.loads(args.config.read_text())
    while True:
        try:
            refresh(config)
        except Exception as error:
            print(f'MONITORING RETAINED previous view: {error}',flush=True)
            if not args.watch:
                raise
        if not args.watch:
            return
        time.sleep(300)

if __name__=='__main__':
    main()
