#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Live boundary checks. Credentials are read from a private file, never logged."""
import argparse,base64,hashlib,http.client,json,pathlib,secrets,socket,urllib.parse

def main():
    p=argparse.ArgumentParser();p.add_argument('--url',required=True);p.add_argument('--credentials',type=pathlib.Path,required=True);p.add_argument('--connect-ip',help='Diagnostic DNS override; TLS still verifies the original hostname');p.add_argument('--state-unavailable',action='store_true');a=p.parse_args()
    fields=dict(line.split(': ',1)for line in a.credentials.read_text().splitlines()if ': 'in line)
    username,password=fields['Username'],fields['Password'];url=urllib.parse.urlsplit(a.url)
    if url.scheme not in ('http','https') or url.path not in ('','/'):p.error('Supply the origin URL')
    if a.connect_ip:
        original=socket.getaddrinfo
        socket.getaddrinfo=lambda host,port,*args,**kw:original(a.connect_ip if host==url.hostname else host,port,*args,**kw)
    token='Basic '+base64.b64encode((username+':'+password).encode()).decode();wrong='Basic '+base64.b64encode((username+':wrong-password').encode()).decode()
    count=0
    def request(path,auth=None,method='GET',extra=None,body=None):
        connection=(http.client.HTTPSConnection if url.scheme=='https' else http.client.HTTPConnection)(url.hostname,url.port,timeout=15)
        headers={'User-Agent':'Mozilla/5.0 (compatible; OTriOS-Module-Check/1.0)'};headers.update(extra or {})
        if auth:headers['Authorization']=auth
        connection.request(method,path,body=body,headers=headers);response=connection.getresponse();status=response.status;headers=dict(response.getheaders())
        data=b''if status==101 else response.read(40*1024*1024)
        connection.close();return status,headers,data
    status,_,raw=request('/release',token);assert status==200,('authorized release',status)
    release=json.loads(raw);prefix='/releases/'+release['id']+'/'
    paths=['/','/release','/health','/sync','/stream','/layout','/presentation',prefix+'manifest.json',prefix+'manifest.sig',prefix+'web/index.html',prefix+'web/app.js',prefix+'web/state-worker.js',prefix+'web/release-gate.mjs',prefix+'web/style.css',prefix+'web/pkg/otrios_dashboard.js',prefix+'web/pkg/otrios_dashboard_bg.wasm']
    for path in paths:
        status,headers,data=request(path);assert status==401,(path,status);assert not data.startswith(b'\0asm');count+=1
        status,_,_=request(path,wrong);assert status==401,('wrong password',path,status);count+=1
    status,_,_=request(prefix+'web/pkg/otrios_dashboard_bg.wasm',method='HEAD');assert status==401;count+=1
    status,_,_=request('/sync',method='POST',body=b'');assert status==401;count+=1
    status,_,data=request('/',token);assert status==200 and b'Loading the signed OTriOS' in data;count+=1
    status,_,raw=request(prefix+'manifest.json',token);assert status==200
    assert hashlib.sha256(raw).hexdigest()==release['id'];manifest=json.loads(raw);count+=1
    for name,item in manifest['assets'].items():
        if not name.startswith('web/'):continue
        status,headers,data=request(prefix+name,token);assert status==200,(name,status)
        assert len(data)==item['bytes']and hashlib.sha256(data).hexdigest()==item['sha256'],name
        assert 'private' in headers.get('Cache-Control',''),name
        if name.endswith('.wasm'):assert headers.get('Content-Type')=='application/wasm'
        count+=1
    ws={'Connection':'Upgrade','Upgrade':'websocket','Sec-WebSocket-Version':'13','Sec-WebSocket-Key':base64.b64encode(secrets.token_bytes(16)).decode()}
    assert request('/stream',extra=ws)[0]==401;count+=1
    if a.state_unavailable:
        assert request('/health',token)[0]==502;count+=1
        print(f'PASS {count} checks: authenticated module runs independently of unavailable state upstream')
    else:
        assert request('/health',token)[0]==200;count+=1
        assert request('/stream',token,extra=dict(ws,Origin='https://unrelated.example'))[0]==403;count+=1
        assert request('/stream',token,extra=dict(ws,Origin='https://otrios.otri.ca'))[0]==101;count+=1
        print(f'PASS {count} checks: password precedes all payloads; signed web bytes, MIME, origin checks and live WebSocket upgrade verified')
if __name__=='__main__':main()
