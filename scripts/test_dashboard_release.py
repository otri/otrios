# SPDX-License-Identifier: Apache-2.0
"""Conformance checks for the code-signing boundary; never executes test artifacts."""
import hashlib, importlib.util, json, os, pathlib, shutil, subprocess, tempfile, unittest
HERE=pathlib.Path(__file__).parent

def module(name,file):
    spec=importlib.util.spec_from_file_location(name,HERE/file);value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value
publisher=module('publisher','publish-dashboard.py');agent=module('frame_agent','frame-dashboard-agent.py')
activation=module('activation','activate-dashboard-release.py')
OPENSSL=os.environ.get('OTRIOS_TEST_OPENSSL','/opt/homebrew/opt/openssl@3/bin/openssl' if pathlib.Path('/opt/homebrew/opt/openssl@3/bin/openssl').exists() else 'openssl')
class SignedReleases(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=pathlib.Path(self.temp.name)
        self.key=self.root/'private.pem';self.public=self.root/'public.pem'
        subprocess.run([OPENSSL,'genpkey','-algorithm','ED25519','-out',str(self.key)],check=True,capture_output=True)
        subprocess.run([OPENSSL,'pkey','-in',str(self.key),'-pubout','-out',str(self.public)],check=True,capture_output=True)
        self.web=self.root/'web';(self.web/'pkg').mkdir(parents=True)
        for file in ['index.html','app.js','state-worker.js','session.mjs','monitoring-session.mjs','artifact-cache.mjs','artifact-cache-worker.js','style.css','pkg/otrios_dashboard.js']:(self.web/file).write_text('test')
        (self.web/'pkg/otrios_dashboard_bg.wasm').write_bytes(b'\0asm\x01\0\0\0')
        elf=bytearray(52);elf[:6]=b'\x7fELF\x01\x01';elf[18:20]=(40).to_bytes(2,'little')
        self.native=self.root/'native';self.native.write_bytes(elf)
        (self.web/'fonts').mkdir();(self.web/'fonts/Roboto-Regular.otq.zst').write_bytes(b'font-test-resource')
        for name in ['app-catalog.mjs','Slug-LICENSE.txt','gpu-scene.mjs','slug-shaders.mjs','font-resource.mjs','font-reference.mjs']:(self.web/name).write_text('test')
        (self.root/'apps').mkdir();(self.root/'apps/catalog.flex').write_bytes(b'test-catalog')
        self.out=self.root/'releases';self.cache=self.root/'cache';self.cache.mkdir()
        self.pointer=self.publish();self.id=self.pointer['id'];self.copy_cache(self.id)
    def tearDown(self):self.temp.cleanup()
    def publish(self):return publisher.publish(self.web,self.native,self.out,self.key,OPENSSL,catalog=self.root/'apps/catalog.flex')
    def copy_cache(self,id):
        dest=self.cache/id;dest.mkdir()
        for file in ['manifest.json','manifest.sig']:shutil.copyfile(self.out/id/file,dest/file)
        if (self.out/id/'apps/catalog.flex').exists():shutil.copyfile(self.out/id/'apps/catalog.flex',dest/'app-catalog.flex')
        for name in ['fonts/Roboto-Regular.otq.zst','Slug-LICENSE.txt']:
            if (self.out/id/'web'/name).exists():
                (dest/name).parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(self.out/id/'web'/name,dest/name)
        for name in ['dashboard-native']:
            source=self.out/id/'native/armv7'/name
            if source.exists():shutil.copyfile(source,dest/name)
    def test_authority_app_payloads_are_signed_but_never_replicated_publicly(self):
        package=self.root/'apps/monitoring/package';package.mkdir(parents=True)
        (package/'app.flex').write_bytes(b'private-app-manifest')
        (package/'ui.flex').write_bytes(b'private-ui-canary')
        (package/'behavior.wasm').write_bytes(b'private-code-canary')
        pointer=self.publish();folder=self.out/pointer['id'];manifest=json.loads((folder/'manifest.json').read_bytes())
        self.assertIn('apps/catalog.flex',manifest['assets'])
        self.assertIn('authority/apps/monitoring/ui.flex',manifest['authority'])
        self.assertFalse(any(name.startswith('authority/') for name in manifest['assets']))
        self.assertEqual((folder/'authority/apps/monitoring/ui.flex').read_bytes(),b'private-ui-canary')
        self.assertEqual(activation.verify(self.out,self.public,OPENSSL,(self.out/'current.json').read_bytes()),pointer['id'])
        (folder/'authority/apps/monitoring/ui.flex').write_bytes(b'tampered private UI')
        with self.assertRaises(ValueError):activation.verify(self.out,self.public,OPENSSL,(self.out/'current.json').read_bytes())
    def test_cached_app_catalog_tampering_is_rejected(self):
        (self.cache/self.id/'app-catalog.flex').write_bytes(b'tampered')
        with self.assertRaises(ValueError):agent.cached(self.cache,self.id,self.public,OPENSSL)
    def test_font_resource_is_verified_with_cached_native_release(self):
        path=self.cache/self.id/'fonts/Roboto-Regular.otq.zst'
        path.write_bytes(b'corrupt')
        with self.assertRaises(ValueError):agent.cached(self.cache,self.id,self.public,OPENSSL)
    def test_resource_budget_rejects_oversized_font_before_download(self):
        with self.assertRaises(ValueError):list(agent.resource_items({'assets':{'web/fonts/Roboto-Regular.otq.zst':{'bytes':1024*1024+1}}}))
    def test_bootstrap_accepts_signed_future_crdt_protocol_but_not_invalid_metadata(self):
        raw=(self.out/self.id/'manifest.json').read_bytes();value=json.loads(raw)
        value.update(state_schema='future-app-state-v99',sync_protocol='future-crdt-v99')
        path=self.root/'future.json';sig=self.root/'future.sig'
        for protocol,valid in [('future-crdt-v99',True),('invalid\nprotocol',False)]:
            value['sync_protocol']=protocol;data=json.dumps(value).encode();path.write_bytes(data)
            subprocess.run([OPENSSL,'pkeyutl','-sign','-rawin','-inkey',str(self.key),'-in',str(path),'-out',str(sig)],check=True,capture_output=True)
            args=(data,sig.read_bytes(),hashlib.sha256(data).hexdigest(),self.cache,self.public,OPENSSL)
            if valid:self.assertEqual(agent.verify_manifest(*args)['sync_protocol'],protocol)
            else:
                with self.assertRaises(ValueError):agent.verify_manifest(*args)
    def test_single_store_activation_verifies_before_pointer_and_notifies_after(self):
        from types import SimpleNamespace
        from unittest.mock import patch
        (self.web/'app.js').write_text('new staged code')
        staged=publisher.publish(self.web,self.native,self.out,self.key,OPENSSL,activate=False,catalog=self.root/'apps/catalog.flex')
        pointer=self.root/'staged.json';pointer.write_text(json.dumps(staged));notified=[]
        def notify(url):
            self.assertEqual(json.loads((self.out/'current.json').read_text())['id'],staged['id']);notified.append(url)
        args=SimpleNamespace(releases=self.out,key=self.public,openssl=OPENSSL,pointer=pointer,notify_url='http://127.0.0.1:8876/admin/activate')
        with patch.object(activation,'notify',side_effect=notify):activation.activate(args)
        self.assertEqual(notified,[args.notify_url])
        (self.out/staged['id']/'web/app.js').write_text('corrupt')
        (self.out/'current.json').write_text(json.dumps(self.pointer))
        with patch.object(activation,'notify',side_effect=AssertionError('Unverified activation')):
            with self.assertRaises(ValueError):activation.activate(args)
        self.assertEqual(json.loads((self.out/'current.json').read_text())['id'],self.id)
    def test_signed_native_artifact_is_accepted(self):
        id,path=agent.cached(self.cache,self.id,self.public,OPENSSL);self.assertEqual(id,self.id);self.assertEqual(path.read_bytes(),self.native.read_bytes())
    def test_download_reserves_budget_and_never_evicts_active_release(self):
        from unittest.mock import patch
        (self.web/'app.js').write_text('candidate')
        candidate=self.publish()['id']
        new=self.out/candidate
        required=(new/'manifest.json').stat().st_size+64+self.native.stat().st_size+(new/'web/fonts/Roboto-Regular.otq.zst').stat().st_size+(new/'web/Slug-LICENSE.txt').stat().st_size+(new/'apps/catalog.flex').stat().st_size
        used=agent.cache_size(self.cache)
        def fetch(base,path,limit):return (self.out/path.removeprefix('/releases/')).read_bytes()
        with patch.object(agent,'fetch',side_effect=fetch):
            with self.assertRaises(ValueError):agent.qualify('https://example.com',self.cache,self.public,OPENSSL,candidate,limit=used+required-1,pins=(self.id,))
            self.assertTrue((self.cache/self.id/'dashboard-native').exists())
            _,exe=agent.qualify('https://example.com',self.cache,self.public,OPENSSL,candidate,limit=used+required,pins=(self.id,))
            self.assertEqual(exe.read_bytes(),self.native.read_bytes())
            self.assertLessEqual(agent.cache_size(self.cache),used+required)
    def test_modified_native_bytes_are_rejected(self):
        (self.cache/self.id/'dashboard-native').write_bytes(self.native.read_bytes()+b'changed')
        with self.assertRaises(ValueError):agent.cached(self.cache,self.id,self.public,OPENSSL)
    def test_wrong_signing_key_is_rejected_even_with_valid_manifest_hash(self):
        other=self.root/'other.pem';subprocess.run([OPENSSL,'genpkey','-algorithm','ED25519','-out',str(other)],check=True,capture_output=True)
        subprocess.run([OPENSSL,'pkeyutl','-sign','-rawin','-inkey',str(other),'-in',str(self.cache/self.id/'manifest.json'),'-out',str(self.cache/self.id/'manifest.sig')],check=True,capture_output=True)
        with self.assertRaises(ValueError):agent.cached(self.cache,self.id,self.public,OPENSSL)
    def test_new_web_code_publishes_new_complete_release_and_retains_old(self):
        old=(self.out/self.id/'web/app.js').read_bytes();(self.web/'app.js').write_text('new app code')
        new=self.publish();self.assertNotEqual(new['id'],self.id);self.assertEqual((self.out/self.id/'web/app.js').read_bytes(),old)
        self.assertEqual(json.loads((self.out/'current.json').read_text())['id'],new['id']);self.copy_cache(new['id']);agent.cached(self.cache,new['id'],self.public,OPENSSL)
    def test_modified_manifest_and_invalid_target_are_rejected(self):
        path=self.cache/self.id/'manifest.json';path.write_bytes(path.read_bytes()+b' ')
        with self.assertRaises(ValueError):agent.cached(self.cache,self.id,self.public,OPENSSL)
        self.native.write_bytes(b'not an ARM executable')
        with self.assertRaises(ValueError):self.publish()
    def test_one_direct_executable_is_published_and_verified(self):
        manifest=json.loads((self.out/self.id/'manifest.json').read_bytes())
        self.assertEqual({name for name in manifest['assets'] if name.startswith('native/')},{'native/armv7/dashboard-native'})
        _,primary=agent.cached(self.cache,self.id,self.public,OPENSSL)
        self.assertEqual(primary.name,'dashboard-native')
        primary.write_bytes(primary.read_bytes()+b'tampered')
        with self.assertRaises(ValueError):agent.cached(self.cache,self.id,self.public,OPENSSL)
    def test_staged_signed_store_remains_inactive_until_verified_activation(self):
        (self.web/'app.js').write_text('staged code')
        staged=publisher.publish(self.web,self.native,self.out,self.key,OPENSSL,activate=False,catalog=self.root/'apps/catalog.flex')
        self.assertNotEqual(staged['id'],self.id)
        self.assertEqual(json.loads((self.out/'current.json').read_text())['id'],self.id)
        self.assertEqual(activation.verify(self.out,self.public,OPENSSL,json.dumps(staged).encode()),staged['id'])
        (self.out/staged['id']/'web/app.js').write_text('tampered')
        with self.assertRaises(ValueError):activation.verify(self.out,self.public,OPENSSL,json.dumps(staged).encode())
class ArtifactBudget(unittest.TestCase):
    def test_lru_pins_blob_bytes_and_oversized_admission(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=pathlib.Path(tmp)
            for i in range(3):
                p=root/(str(i)*64);p.mkdir();(p/'payload').write_bytes(b'1234');os.utime(p,(i+1,i+1))
            agent.trim_cache(root,8,pins=('0'*64,))
            self.assertTrue((root/('0'*64)).exists());self.assertFalse((root/('1'*64)).exists());self.assertTrue((root/('2'*64)).exists())
            with self.assertRaises(ValueError):agent.trim_cache(root,3,pins=('0'*64,'2'*64))
            (root/'objects').mkdir();(root/'objects'/('f'*64)).write_bytes(b'1234')
            agent.trim_cache(root,8,pins=('0'*64,));self.assertLessEqual(agent.cache_size(root),8)
    def test_symlinks_never_escape_managed_cache(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=pathlib.Path(tmp);(root/('a'*64)).symlink_to('/etc')
            with self.assertRaises(ValueError):agent.trim_cache(root,1000)
    def test_reload_waits_for_server_and_finished_gesture(self):
        good={'stream_ready':True,'state_confirmed':True,'pending_commands':0,'input_busy':False}
        self.assertTrue(agent.hot_reload_ready(good))
        for update in [{'state_confirmed':False},{'pending_commands':1},{'input_busy':True}]:self.assertFalse(agent.hot_reload_ready(dict(good,**update)))
    def test_disconnected_ram_edits_can_be_discarded_but_gestures_still_block(self):
        clean={'stream_ready':False,'state_confirmed':False,'pending_commands':1,'reload_ready':True,'input_busy':False}
        self.assertTrue(agent.hot_reload_ready(clean))
        self.assertTrue(agent.hot_reload_ready(dict(clean,reload_ready=False)))
        self.assertFalse(agent.hot_reload_ready(dict(clean,input_busy=True)))
        self.assertFalse(agent.hot_reload_ready(dict(clean,stream_ready=True,reload_ready=False)))
        self.assertFalse(agent.hot_reload_ready({}))
    def test_unavailable_state_does_not_blacklist_the_current_code_pointer_forever(self):
        from types import SimpleNamespace
        from unittest.mock import patch
        old,new='a'*64,'b'*64
        clock=[1000.];attempts=[]
        class Host:
            def __init__(self,args,release,exe,screen=None):
                self.release=release;self.process=SimpleNamespace(poll=lambda:None)
            def wait_ready(self,require_stream=True):
                if self.release==new and require_stream:
                    attempts.append(clock[0])
                    if len(attempts)==1:raise RuntimeError('state authority unavailable')
                    agent.running=False
            def info(self):return {'reload_ready':True,'input_busy':False,'screen':'home'}
            def close(self):pass
        class Feed:
            def __init__(self,args,current):self.sent=False;self.process=SimpleNamespace(poll=lambda:None)
            def take(self):
                sleep(.1)
                if not self.sent:self.sent=True;return {'id':new}
            def close(self):pass
        with tempfile.TemporaryDirectory() as tmp:
            root=pathlib.Path(tmp);cache=root/'cache';cache.mkdir()
            (cache/'last-good.json').write_text(json.dumps({'id':old}))
            args=SimpleNamespace(cache=cache,runtime_dir=root/'run',key=root/'key',openssl=OPENSSL,url='https://example.test',cache_bytes=agent.CACHE_BYTES)
            previous=agent.running;agent.running=True
            def sleep(seconds):
                clock[0]+=seconds
                if clock[0]>1100:raise AssertionError('Candidate was stranded')
            try:
                with patch.object(agent,'Host',Host), patch.object(agent,'cached',return_value=(old,root/'old')), patch.object(agent,'qualify',return_value=(new,root/'new')), patch.object(agent,'ReleaseFeed',Feed), patch.object(agent,'session_host',return_value=True), patch.object(agent,'bootstrap_release',side_effect=AssertionError('Extra network connection')), patch.object(agent,'fetch',side_effect=AssertionError('Release pointer must not be polled')), patch.object(agent.time,'monotonic',side_effect=lambda:clock[0]), patch.object(agent.time,'sleep',side_effect=sleep):
                    agent.main(args)
            finally:agent.running=previous
            self.assertEqual(json.loads((cache/'last-good.json').read_text())['id'],new)
        self.assertEqual(len(attempts),2)
        self.assertGreaterEqual(attempts[1]-attempts[0],29.999)
    def test_local_notices_coalesce_and_do_not_open_a_remote_connection(self):
        import socket
        from types import SimpleNamespace
        with tempfile.TemporaryDirectory() as tmp:
            feed=agent.ReleaseFeed(SimpleNamespace(runtime_dir=pathlib.Path(tmp)))
            sender=socket.socket(socket.AF_UNIX,socket.SOCK_DGRAM)
            try:
                sender.sendto(b'invalid',str(feed.path))
                for c in ['a','b']:
                    sender.sendto(json.dumps({'type':'upgrade','version':1,'release':{'id':c*64,'web_entry':f'/releases/{c*64}/web/index.html'}}).encode(),str(feed.path))
                self.assertEqual(feed.take()['id'],'b'*64)
            finally:sender.close();feed.close()
            self.assertFalse(feed.path.exists())
    def test_code_download_identifies_client_and_bounds_response(self):
        from unittest.mock import patch
        class Response:
            def __enter__(self):return self
            def __exit__(self,*args):pass
            def read(self,limit):return b'x'*limit
        def open_request(request,timeout):
            self.assertEqual(request.get_header('User-agent'),'OTriOS-Release/1')
            self.assertEqual(request.full_url,'https://code.example/releases/asset');self.assertEqual(timeout,10)
            return Response()
        with patch.object(agent.urllib.request,'urlopen',side_effect=open_request):
            with self.assertRaises(ValueError):agent.fetch('https://code.example','/releases/asset',4)
        with patch.object(agent.urllib.request,'urlopen',side_effect=AssertionError('Cross-origin fetch')):
            with self.assertRaises(ValueError):agent.fetch('https://code.example','https://other.example/asset',4)
    def test_blob_hash_is_verified_and_cache_can_survive_offline(self):
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as tmp:
            root=pathlib.Path(tmp);data=b'immutable blob';digest=hashlib.sha256(data).hexdigest()
            with patch.object(agent,'fetch',return_value=data):path=agent.cached_blob('https://example.com',root,digest,len(data),limit=100)
            with patch.object(agent,'fetch',side_effect=AssertionError('Network used for cached blob')):self.assertEqual(agent.cached_blob('https://example.com',root,digest,len(data),limit=100).read_bytes(),data)
            path.unlink()
            with patch.object(agent,'fetch',return_value=b'wrong'):
                with self.assertRaises(ValueError):agent.cached_blob('https://example.com',root,digest,len(data),limit=100)
def direct_ready(**values):
    return dict({'renderer':'gles2-direct','video':'DRM','rotation':90,'output':[800,1280],'touch_calibrated':True,'touch_devices':1,'scanout_format':'RGB565','color_bits':[5,6,5,0],'stride':1600},**values)
class DirectHostContract(unittest.TestCase):
    def test_bootstrap_never_resumes_the_synthetic_locked_route(self):
        from types import SimpleNamespace
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as tmp:
            args=SimpleNamespace(runtime_dir=pathlib.Path(tmp),rotation=90,touch_matrix='0,1,0,-1,0,1',url='https://state.example',capture_first_frame=False)
            with patch.object(agent.subprocess,'Popen')as launch:
                agent.Host(args,'a'*64,pathlib.Path('/verified/dashboard-native'),screen='locked')
                self.assertNotIn('--screen',launch.call_args.args[0])
                agent.Host(args,'a'*64,pathlib.Path('/verified/dashboard-native'),screen='home')
                self.assertIn('--screen',launch.call_args.args[0]);self.assertEqual(launch.call_args.args[0][launch.call_args.args[0].index('--screen')+1],'home')
    def test_connected_candidate_waits_for_an_admitted_confirmed_app(self):
        from types import SimpleNamespace
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as tmp:
            host=agent.Host.__new__(agent.Host);host.a=SimpleNamespace(rotation=90)
            host.ready=pathlib.Path(tmp)/'ready.json';host.process=SimpleNamespace(poll=lambda:None)
            info=direct_ready(stream_ready=True,ui_ref='',state_confirmed=False);host.ready.write_text(json.dumps(info))
            waited=[]
            def tick(seconds):
                waited.append(seconds);info.update(ui_ref='app:home/ui',state_confirmed=True);host.ready.write_text(json.dumps(info))
            with patch.object(agent.time,'sleep',side_effect=tick):host.wait_ready()
            self.assertEqual(waited,[.1])
    def test_direct_candidate_cannot_claim_wayland_or_uncalibrated_readiness(self):
        good=direct_ready()
        agent.ready_contract(good,90)
        for update in [{'video':'wayland'},{'touch_calibrated':False},{'touch_devices':0},{'rotation':0},{'output':[1280,800]}]:
            with self.assertRaises(RuntimeError):agent.ready_contract(dict(good,**update),90)
        with self.assertRaises(RuntimeError):agent.ready_contract({'renderer':'opengles2'},0)
    def test_owned_gles_backend_requires_verified_scanout_and_direct_service(self):
        good={'renderer':'gles2-direct','video':'DRM','rotation':90,'output':[800,1280],'touch_calibrated':True,'touch_devices':1,'scanout_format':'RGB565','color_bits':[5,6,5,0],'stride':1600}
        agent.ready_contract(good,90)
        for update in [{'color_bits':[8,8,8,0]},{'stride':800},{'scanout_format':'unknown'},{'touch_devices':0}]:
            with self.assertRaises(RuntimeError):agent.ready_contract(dict(good,**update),90)
        with self.assertRaises(RuntimeError):agent.ready_contract(dict(good,renderer='opengles2'),90)
    def test_live_rule_is_read_without_rewriting_and_bad_matrices_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            rule=pathlib.Path(tmp)/'accepted.rules';data='ENV{LIBINPUT_CALIBRATION_MATRIX}="0 1 0 -1 0 1"'
            rule.write_text(data);self.assertEqual(agent.touch_calibration(rule),'0,1,0,-1,0,1');self.assertEqual(rule.read_text(),data)
            for bad in ['0 0 0 0 0 0','nan 1 0 -1 0 1','1 0 0 0 1']:
                rule.write_text('ENV{LIBINPUT_CALIBRATION_MATRIX}="'+bad+'"')
                with self.assertRaises(ValueError):agent.touch_calibration(rule)
if __name__=='__main__':unittest.main()
