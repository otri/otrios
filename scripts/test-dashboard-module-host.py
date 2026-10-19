#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Public code/demo and private Monitoring conformance. Never logs credentials."""
import argparse,base64,hashlib,http.client,json,pathlib,secrets,socket,urllib.parse

def main():
    p=argparse.ArgumentParser();p.add_argument('--url',required=True);p.add_argument('--credentials',type=pathlib.Path,required=True);p.add_argument('--connect-ip',help='DNS override retaining TLS hostname verification');p.add_argument('--state-unavailable',action='store_true');a=p.parse_args()
    fields=dict(line.split(': ',1)for line in a.credentials.read_text().splitlines()if ': 'in line)
    username,password=fields['Username'],fields['Password'];url=urllib.parse.urlsplit(a.url)
    if url.scheme not in ('http','https') or url.path not in ('','/'):p.error('Supply the origin URL')
    if a.connect_ip:
        original=socket.getaddrinfo
        socket.getaddrinfo=lambda host,port,*args,**kw:original(a.connect_ip if host==url.hostname else host,port,*args,**kw)
    count=0
    def request(path,method='GET',extra=None,body=None,upgrade=False):
        connection=(http.client.HTTPSConnection if url.scheme=='https' else http.client.HTTPConnection)(url.hostname,url.port,timeout=15)
        headers={'User-Agent':'Mozilla/5.0 (compatible; OTriOS-Module-Check/2.0)'};headers.update(extra or {})
        connection.request(method,path,body=body,headers=headers);response=connection.getresponse();status=response.status;headers=response.headers
        if upgrade and status==101:return connection,response
        data=response.read(40*1024*1024);connection.close();return status,headers,data
    status,_,raw=request('/release');assert status==200,('public release',status)
    release=json.loads(raw);prefix='/releases/'+release['id']+'/'
    status,_,data=request('/');assert status==200 and b'Loading the signed OTriOS' in data;count+=1
    status,_,raw=request(prefix+'manifest.json');assert status==200 and hashlib.sha256(raw).hexdigest()==release['id'];manifest=json.loads(raw);count+=1
    for name,item in manifest['assets'].items():
        if not name.startswith('web/') and name!='apps/catalog.flex':continue
        status,headers,data=request(prefix+name);assert status==200,(name,status)
        assert len(data)==item['bytes']and hashlib.sha256(data).hexdigest()==item['sha256'],name
        assert 'public' in headers.get('Cache-Control',''),name
        if name.endswith('.wasm'):assert headers.get('Content-Type')=='application/wasm'
        if name.endswith('artifact-cache-worker.js'):assert headers.get('Service-Worker-Allowed')=='/'
        count+=1
    assert manifest['state_schema']=='otrios-canvas-merkle-state-v1' and manifest['sync_protocol']=='otrios-canvas-sync-v2';count+=1
    assert 'apps/catalog.flex' in manifest['assets'];count+=1
    for name in manifest.get('authority',{}):
        assert name.startswith('authority/apps/') and request(prefix+name)[0]==404;count+=1
    for path in ['/layout','/presentation','/sync','/private/stream','/stream','/demo/stream','/monitoring/stream']:
        for method in ['GET','POST']:
            assert request(path,method=method,body=b''if method=='POST'else None)[0]==404,(path,method);count+=1
    if a.state_unavailable:
        assert request('/health')[0]==502;count+=1;print(f'PASS {count} checks: public signed module independent of unavailable state adapter');return
    for path in ['/auth/session','/monitoring/presentation']:
        assert request(path)[0]==401,path;count+=1
        assert request(path,extra={'Cookie':'__Host-otrios_monitoring='+'a'*64})[0]==401;count+=1
    basic='Basic '+base64.b64encode((username+':'+password).encode()).decode()
    assert request('/monitoring/presentation',extra={'Authorization':basic})[0]==401;count+=1
    assert request('/monitoring/presentation',method='HEAD')[0]==401;count+=1
    assert request('/health')[0]==200;count+=1
    def ws_headers(origin):return{'Connection':'Upgrade','Upgrade':'websocket','Sec-WebSocket-Version':'13','Sec-WebSocket-Key':base64.b64encode(secrets.token_bytes(16)).decode(),'Origin':origin}
    assert request('/apps/stream',extra=ws_headers('https://unrelated.example'))[0]==403;count+=1
    def frame(response):
        def read(n):
            data=response.fp.read(n)
            assert len(data)==n,'incomplete WebSocket frame';return data
        header=read(2);size=header[1]&127
        if size==126:size=int.from_bytes(read(2),'big')
        elif size==127:size=int.from_bytes(read(8),'big')
        assert not header[1]&128 and size<=2*1024*1024
        return header[0]&15,read(size)
    connection,response=request('/apps/stream',extra=ws_headers(a.url.rstrip('/')),upgrade=True)
    opcode,data=frame(response);assert opcode==1 and json.loads(data).get('type')=='release';count+=1
    connection.close()
    def login(password,origin):return request('/auth/session',method='POST',extra={'Origin':origin,'X-OTriOS-Request':'session','Content-Type':'application/json'},body=json.dumps({'username':username,'password':password}).encode())
    assert login(password,'https://unrelated.example')[0]==403;count+=1
    assert login('wrong-password',a.url.rstrip('/'))[0]==401;count+=1
    status,headers,_=login(password,a.url.rstrip('/'));assert status==200;cookie=headers['Set-Cookie']
    for attribute in ['Secure','HttpOnly','SameSite=Strict','Path=/']:assert attribute in cookie;count+=1
    cookie=cookie.split(';')[0]
    assert request('/auth/session',extra={'Cookie':cookie})[0]==200;count+=1
    status,headers,_=request('/monitoring/presentation',extra={'Cookie':cookie});assert status==200 and headers.get('Cache-Control')=='no-store';count+=1
    ws=ws_headers(a.url.rstrip('/'));ws['Cookie']=cookie
    connection,response=request('/apps/stream',extra=ws,upgrade=True)
    opcode,data=frame(response);assert opcode==1 and json.loads(data).get('type')=='release';count+=1
    connection.close()
    assert request('/presentation',method='POST',extra={'Cookie':cookie},body=b'')[0]==404;count+=1
    assert request('/auth/session',method='DELETE',extra={'Origin':a.url.rstrip('/'),'X-OTriOS-Request':'session','Cookie':cookie})[0]==200;count+=1
    assert request('/monitoring/presentation',extra={'Cookie':cookie})[0]==401;count+=1
    assert request('/release')[0]==200;count+=1
    print(f'PASS {count} checks: public signed catalog/code and app transport; Monitoring HTTP cookie revocation, private artifact and admin isolation verified (private app mounts tested separately)')
if __name__=='__main__':main()
