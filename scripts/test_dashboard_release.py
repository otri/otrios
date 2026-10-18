# SPDX-License-Identifier: Apache-2.0
"""Conformance checks for the code-signing boundary; never executes test artifacts."""
import hashlib, importlib.util, json, os, pathlib, shutil, subprocess, tempfile, unittest
HERE=pathlib.Path(__file__).parent

def module(name,file):
    spec=importlib.util.spec_from_file_location(name,HERE/file);value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value
publisher=module('publisher','publish-dashboard.py');agent=module('frame_agent','frame-dashboard-agent.py')
module_publisher=module('module_publisher','publish-dashboard-module.py')
OPENSSL=os.environ.get('OTRIOS_TEST_OPENSSL','/opt/homebrew/opt/openssl@3/bin/openssl' if pathlib.Path('/opt/homebrew/opt/openssl@3/bin/openssl').exists() else 'openssl')
class SignedReleases(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=pathlib.Path(self.temp.name)
        self.key=self.root/'private.pem';self.public=self.root/'public.pem'
        subprocess.run([OPENSSL,'genpkey','-algorithm','ED25519','-out',str(self.key)],check=True,capture_output=True)
        subprocess.run([OPENSSL,'pkey','-in',str(self.key),'-pubout','-out',str(self.public)],check=True,capture_output=True)
        self.web=self.root/'web';(self.web/'pkg').mkdir(parents=True)
        for file in ['index.html','app.js','state-worker.js','release-gate.mjs','monitoring-session.mjs','artifact-cache.mjs','artifact-cache-worker.js','style.css','pkg/otrios_dashboard.js']:(self.web/file).write_text('test')
        (self.web/'pkg/otrios_dashboard_bg.wasm').write_bytes(b'\0asm\x01\0\0\0')
        elf=bytearray(52);elf[:6]=b'\x7fELF\x01\x01';elf[18:20]=(40).to_bytes(2,'little')
        self.native=self.root/'native';self.native.write_bytes(elf)
        (self.web/'fonts').mkdir();(self.web/'fonts/Roboto-Regular.otq.zst').write_bytes(b'font-test-resource')
        for name in ['Slug-LICENSE.txt','gpu-scene.mjs','slug-shaders.mjs','font-resource.mjs','font-reference.mjs']:(self.web/name).write_text('test')
        self.out=self.root/'releases';self.cache=self.root/'cache';self.cache.mkdir()
        self.pointer=self.publish();self.id=self.pointer['id'];self.copy_cache(self.id)
    def tearDown(self):self.temp.cleanup()
    def publish(self):return publisher.publish(self.web,self.native,self.out,self.key,OPENSSL)
    def copy_cache(self,id):
        dest=self.cache/id;dest.mkdir()
        for file in ['manifest.json','manifest.sig']:shutil.copyfile(self.out/id/file,dest/file)
        for name in ['fonts/Roboto-Regular.otq.zst','Slug-LICENSE.txt']:
            if (self.out/id/'web'/name).exists():
                (dest/name).parent.mkdir(parents=True,exist_ok=True);shutil.copyfile(self.out/id/'web'/name,dest/name)
        for name in ['dashboard-native','dashboard-sdl']:
            source=self.out/id/'native/armv7'/name
            if source.exists():shutil.copyfile(source,dest/name)
    def test_font_resource_is_verified_with_cached_native_release(self):
        path=self.cache/self.id/'fonts/Roboto-Regular.otq.zst'
        path.write_bytes(b'corrupt')
        with self.assertRaises(ValueError):agent.cached(self.cache,self.id,self.public,OPENSSL)
    def test_resource_budget_rejects_oversized_font_before_download(self):
        with self.assertRaises(ValueError):list(agent.resource_items({'assets':{'web/fonts/Roboto-Regular.otq.zst':{'bytes':1024*1024+1}}}))
    def test_signed_native_artifact_is_accepted(self):
        id,path=agent.cached(self.cache,self.id,self.public,OPENSSL);self.assertEqual(id,self.id);self.assertEqual(path.read_bytes(),self.native.read_bytes())
    def test_download_reserves_budget_and_never_evicts_active_release(self):
        from unittest.mock import patch
        (self.web/'app.js').write_text('candidate')
        candidate=self.publish()['id']
        new=self.out/candidate
        required=(new/'manifest.json').stat().st_size+64+self.native.stat().st_size+(new/'web/fonts/Roboto-Regular.otq.zst').stat().st_size+(new/'web/Slug-LICENSE.txt').stat().st_size
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
    def test_direct_and_recovery_executables_are_both_signed_and_selected(self):
        fallback=self.root/'sdl';fallback.write_bytes(self.native.read_bytes()+b'recovery')
        pointer=publisher.publish(self.web,self.native,self.out,self.key,OPENSSL,fallback);release=pointer['id'];self.copy_cache(release)
        _,primary=agent.cached(self.cache,release,self.public,OPENSSL)
        _,recovery=agent.cached(self.cache,release,self.public,OPENSSL,sdl=True)
        self.assertEqual(primary.name,'dashboard-native');self.assertEqual(recovery.name,'dashboard-sdl')
        self.assertEqual(recovery.read_bytes(),fallback.read_bytes())
        recovery.write_bytes(recovery.read_bytes()+b'tampered')
        with self.assertRaises(ValueError):agent.cached(self.cache,release,self.public,OPENSSL,sdl=True)
    def test_module_replica_checks_all_assets_before_transfer(self):
        release,pointer,payload=module_publisher.bundle(self.out,self.public,OPENSSL)
        self.assertEqual(release,self.id);self.assertIn('web/pkg/otrios_dashboard_bg.wasm',payload)
        self.assertEqual(json.loads(pointer)['id'],self.id)
        (self.out/self.id/'web/app.js').write_text('tampered module')
        with self.assertRaises(ValueError):module_publisher.bundle(self.out,self.public,OPENSSL)
    def test_staged_module_does_not_announce_until_replica_is_ready(self):
        (self.web/'app.js').write_text('staged code')
        staged=publisher.publish(self.web,self.native,self.out,self.key,OPENSSL,activate=False)
        self.assertNotEqual(staged['id'],self.id)
        self.assertEqual(json.loads((self.out/'current.json').read_text())['id'],self.id)
        release,_,_=module_publisher.bundle(self.out,self.public,OPENSSL,json.dumps(staged).encode())
        self.assertEqual(release,staged['id'])
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
    def test_blob_hash_is_verified_and_cache_can_survive_offline(self):
        from unittest.mock import patch
        with tempfile.TemporaryDirectory() as tmp:
            root=pathlib.Path(tmp);data=b'immutable blob';digest=hashlib.sha256(data).hexdigest()
            with patch.object(agent,'fetch',return_value=data):path=agent.cached_blob('https://example.com',root,digest,len(data),limit=100)
            with patch.object(agent,'fetch',side_effect=AssertionError('Network used for cached blob')):self.assertEqual(agent.cached_blob('https://example.com',root,digest,len(data),limit=100).read_bytes(),data)
            path.unlink()
            with patch.object(agent,'fetch',return_value=b'wrong'):
                with self.assertRaises(ValueError):agent.cached_blob('https://example.com',root,digest,len(data),limit=100)
class DirectHostContract(unittest.TestCase):
    def test_direct_candidate_cannot_claim_wayland_or_uncalibrated_readiness(self):
        good={'renderer':'opengles2','video':'KMSDRM','rotation':90,'output':[800,1280],'touch_calibrated':True,'touch_devices':1}
        agent.ready_contract(good,'KMSDRM',90)
        for update in [{'video':'wayland'},{'touch_calibrated':False},{'touch_devices':0},{'rotation':0},{'output':[1280,800]}]:
            with self.assertRaises(RuntimeError):agent.ready_contract(dict(good,**update),'KMSDRM',90)
        agent.ready_contract({'renderer':'opengles2'},'wayland',0)
    def test_owned_gles_backend_requires_verified_scanout_and_direct_service(self):
        good={'renderer':'gles2-direct','video':'DRM','rotation':90,'output':[800,1280],'touch_calibrated':True,'touch_devices':1,'scanout_format':'RGB565','color_bits':[5,6,5,0],'stride':1600}
        agent.ready_contract(good,'KMSDRM',90)
        for update in [{'color_bits':[8,8,8,0]},{'stride':800},{'scanout_format':'unknown'},{'touch_devices':0}]:
            with self.assertRaises(RuntimeError):agent.ready_contract(dict(good,**update),'KMSDRM',90)
        with self.assertRaises(RuntimeError):agent.ready_contract(good,'wayland',90)
    def test_live_rule_is_read_without_rewriting_and_bad_matrices_are_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            rule=pathlib.Path(tmp)/'accepted.rules';data='ENV{LIBINPUT_CALIBRATION_MATRIX}="0 1 0 -1 0 1"'
            rule.write_text(data);self.assertEqual(agent.touch_calibration(rule),'0,1,0,-1,0,1');self.assertEqual(rule.read_text(),data)
            for bad in ['0 0 0 0 0 0','nan 1 0 -1 0 1','1 0 0 0 1']:
                rule.write_text('ENV{LIBINPUT_CALIBRATION_MATRIX}="'+bad+'"')
                with self.assertRaises(ValueError):agent.touch_calibration(rule)
if __name__=='__main__':unittest.main()
