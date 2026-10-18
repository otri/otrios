# SPDX-License-Identifier: Apache-2.0
"""Conformance checks for the code-signing boundary; never executes test artifacts."""
import hashlib, importlib.util, json, os, pathlib, shutil, subprocess, tempfile, unittest
HERE=pathlib.Path(__file__).parent

def module(name,file):
    spec=importlib.util.spec_from_file_location(name,HERE/file);value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value
publisher=module('publisher','publish-dashboard.py');agent=module('frame_agent','frame-dashboard-agent.py')
OPENSSL=os.environ.get('OTRIOS_TEST_OPENSSL','/opt/homebrew/opt/openssl@3/bin/openssl' if pathlib.Path('/opt/homebrew/opt/openssl@3/bin/openssl').exists() else 'openssl')
class SignedReleases(unittest.TestCase):
    def setUp(self):
        self.temp=tempfile.TemporaryDirectory();self.root=pathlib.Path(self.temp.name)
        self.key=self.root/'private.pem';self.public=self.root/'public.pem'
        subprocess.run([OPENSSL,'genpkey','-algorithm','ED25519','-out',str(self.key)],check=True,capture_output=True)
        subprocess.run([OPENSSL,'pkey','-in',str(self.key),'-pubout','-out',str(self.public)],check=True,capture_output=True)
        self.web=self.root/'web';(self.web/'pkg').mkdir(parents=True)
        for file in ['index.html','app.js','style.css','pkg/otrios_dashboard.js']:(self.web/file).write_text('test')
        (self.web/'pkg/otrios_dashboard_bg.wasm').write_bytes(b'\0asm\x01\0\0\0')
        elf=bytearray(52);elf[:6]=b'\x7fELF\x01\x01';elf[18:20]=(40).to_bytes(2,'little')
        self.native=self.root/'native';self.native.write_bytes(elf)
        self.out=self.root/'releases';self.cache=self.root/'cache';self.cache.mkdir()
        self.pointer=self.publish();self.id=self.pointer['id'];self.copy_cache(self.id)
    def tearDown(self):self.temp.cleanup()
    def publish(self):return publisher.publish(self.web,self.native,self.out,self.key,OPENSSL)
    def copy_cache(self,id):
        dest=self.cache/id;dest.mkdir()
        for file in ['manifest.json','manifest.sig']:shutil.copyfile(self.out/id/file,dest/file)
        shutil.copyfile(self.out/id/'native/armv7/dashboard-native',dest/'dashboard-native')
    def test_signed_native_artifact_is_accepted(self):
        id,path=agent.cached(self.cache,self.id,self.public,OPENSSL);self.assertEqual(id,self.id);self.assertEqual(path.read_bytes(),self.native.read_bytes())
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
if __name__=='__main__':unittest.main()
