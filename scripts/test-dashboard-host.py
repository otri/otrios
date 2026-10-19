#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Public code/demo and private Monitoring conformance. Never logs credentials."""
import argparse,base64,hashlib,http.client,json,pathlib,secrets,socket,urllib.parse

def main():
    p=argparse.ArgumentParser();p.add_argument('--url',required=True);p.add_argument('--credentials',type=pathlib.Path);p.add_argument('--public-only',action='store_true',help='Skip authenticated login checks');p.add_argument('--connect-ip',help='DNS override retaining TLS hostname verification');p.add_argument('--state-unavailable',action='store_true');a=p.parse_args()
    if not a.public_only and not a.credentials:p.error('--credentials is required unless --public-only is selected')
    fields=dict(line.split(': ',1)for line in a.credentials.read_text().splitlines()if ': 'in line) if a.credentials else {'Username':'unused','Password':'unused'}
    username,password=fields['Username'],fields['Password'];url=urllib.parse.urlsplit(a.url)
    if url.scheme not in ('http','https') or url.path not in ('','/'):p.error('Supply the origin URL')
    if a.connect_ip:
        original=socket.getaddrinfo
        socket.getaddrinfo=lambda host,port,*args,**kw:original(a.connect_ip if host==url.hostname else host,port,*args,**kw)
    count=0
    def request(path,method='GET',extra=None,body=None,upgrade=False):
        connection=(http.client.HTTPSConnection if url.scheme=='https' else http.client.HTTPConnection)(url.hostname,url.port,timeout=15)
        headers={'User-Agent':'Mozilla/5.0 (compatible; OTriOS-Host-Check/3.0)'};headers.update(extra or {})
        connection.request(method,path,body=body,headers=headers);response=connection.getresponse();status=response.status;headers=response.headers
        if upgrade and status==101:return connection,response
        data=response.read(40*1024*1024);connection.close();return status,headers,data
    status,_,raw=request('/release');assert status==200,('public release',status)
    release=json.loads(raw);prefix='/releases/'+release['id']+'/'
    status,headers,_=request('/');assert status in (302,307) and headers['Location']==release['web_entry'];count+=1
    status,_,raw=request(prefix+'manifest.json');assert status==200 and hashlib.sha256(raw).hexdigest()==release['id'];manifest=json.loads(raw);count+=1
    for name,item in manifest['assets'].items():
        if not name.startswith('web/') and name!='apps/catalog.flex':continue
        status,headers,data=request(prefix+name);assert status==200,(name,status)
        assert len(data)==item['bytes']and hashlib.sha256(data).hexdigest()==item['sha256'],name
        assert 'public' in headers.get('Cache-Control',''),name
        if name.endswith('.wasm'):assert headers.get('Content-Type')=='application/wasm'
        if name.endswith('artifact-cache-worker.js'):assert headers.get('Service-Worker-Allowed')=='/'
        count+=1
    assert manifest['state_schema']=='otrios-canvas-merkle-state-v1' and manifest['sync_protocol']=='otrios-canvas-sync-v4';count+=1
    assert 'apps/catalog.flex' in manifest['assets'];count+=1
    for name in manifest.get('authority',{}):
        assert name.startswith('authority/apps/') and request(prefix+name)[0]==404;count+=1
    for path in ['/layout','/presentation','/sync','/private/stream','/stream','/demo/stream','/monitoring/stream','/publish','/admin/activate']:
        for method in ['GET','POST']:
            assert request(path,method=method,body=b''if method=='POST'else None)[0]==404,(path,method);count+=1
    def ws_headers(origin):return{'Connection':'Upgrade','Upgrade':'websocket','Sec-WebSocket-Version':'13','Sec-WebSocket-Key':base64.b64encode(secrets.token_bytes(16)).decode(),'Origin':origin}
    def frame(response):
        def read(n):
            data=response.fp.read(n)
            assert len(data)==n,'incomplete WebSocket frame';return data
        header=read(2);size=header[1]&127
        if size==126:size=int.from_bytes(read(2),'big')
        elif size==127:size=int.from_bytes(read(8),'big')
        assert not header[1]&128 and size<=2*1024*1024
        return header[0]&15,read(size)
    assert request('/release/events',extra=ws_headers('https://unrelated.example'))[0]==403;count+=1
    def hello(path='/session',protocol='otrios-canvas-sync-v4',cookie=None):
        headers=ws_headers(a.url.rstrip('/'))
        if cookie:headers['Cookie']=cookie
        connection,response=request(path,extra=headers,upgrade=True)
        payload=json.dumps({'type':'hello','version':1,'release':release['id'],'state_protocol':protocol,'state':True}).encode()
        mask=secrets.token_bytes(4);header=bytes([0x81,0x80|126])+len(payload).to_bytes(2,'big') if len(payload)>=126 else bytes([0x81,0x80|len(payload)])
        connection.sock.sendall(header+mask+bytes(byte^mask[i%4]for i,byte in enumerate(payload)))
        opcode,data=frame(response);message=json.loads(data);connection.close()
        assert opcode==1 and len(data)<=8192 and message['type']=='hello' and message['version']==1 and message['release']['id']==release['id']
        return message
    message=hello('/release/events','retired-crdt-v0');assert message['refresh'] and not message['compatible'];count+=1
    assert request('/session',extra=ws_headers('https://unrelated.example'))[0]==403;count+=1
    message=hello();assert message['compatible'] and message['state_available']==(not a.state_unavailable);count+=1
    for path in ['/apps/stream','/demo/ink']:
        assert request(path,extra=ws_headers(a.url.rstrip('/')))[0]==410;count+=1
    if a.state_unavailable:
        assert request('/health')[0]==503;count+=1;print(f'PASS {count} checks: public signed module independent of unavailable state adapter');return
    for path in ['/auth/session','/monitoring/presentation']:
        assert request(path)[0]==401,path;count+=1
        assert request(path,extra={'Cookie':'__Host-otrios_monitoring='+'a'*64})[0]==401;count+=1
    basic='Basic '+base64.b64encode((username+':'+password).encode()).decode()
    assert request('/monitoring/presentation',extra={'Authorization':basic})[0]==401;count+=1
    assert request('/monitoring/presentation',method='HEAD')[0]==401;count+=1
    assert request('/health')[0]==200;count+=1
    if a.public_only:
        print(f'PASS {count} checks: public signed code, release/session negotiation, private/admin isolation and anonymous denial (login not tested)');return
    def login(password,origin):return request('/auth/session',method='POST',extra={'Origin':origin,'X-OTriOS-Request':'session','Content-Type':'application/json'},body=json.dumps({'username':username,'password':password}).encode())
    assert login(password,'https://unrelated.example')[0]==403;count+=1
    assert login('wrong-password',a.url.rstrip('/'))[0]==401;count+=1
    status,headers,_=login(password,a.url.rstrip('/'));assert status==200;cookie=headers['Set-Cookie']
    for attribute in ['Secure','HttpOnly','SameSite=Strict','Path=/']:assert attribute in cookie;count+=1
    cookie=cookie.split(';')[0]
    assert request('/auth/session',extra={'Cookie':cookie})[0]==200;count+=1
    status,headers,_=request('/monitoring/presentation',extra={'Cookie':cookie});assert status==200 and headers.get('Cache-Control')=='no-store';count+=1
    assert hello(cookie=cookie)['compatible'];count+=1
    assert request('/presentation',method='POST',extra={'Cookie':cookie},body=b'')[0]==404;count+=1
    assert request('/auth/session',method='DELETE',extra={'Origin':a.url.rstrip('/'),'X-OTriOS-Request':'session','Cookie':cookie})[0]==200;count+=1
    assert request('/monitoring/presentation',extra={'Cookie':cookie})[0]==401;count+=1
    assert request('/release')[0]==200;count+=1
    print(f'PASS {count} checks: public signed catalog/code and app transport; Monitoring HTTP cookie revocation, private artifact and admin isolation verified (private app mounts tested separately)')
if __name__=='__main__':main()
