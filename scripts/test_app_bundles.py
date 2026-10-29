# SPDX-License-Identifier: Apache-2.0
"""Signed bundle installation across real tool, authority and guest processes."""
import hashlib,importlib.util,json,os,pathlib,shutil,socket,subprocess,sys,time,unittest,urllib.request
from types import SimpleNamespace
from unittest import mock
HERE=pathlib.Path(__file__).parent;REPO=HERE.parent
def module(name,file):
    spec=importlib.util.spec_from_file_location(name,HERE/file);value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value
legacy=module('release_fixtures','test_dashboard_release.py')
installer=module('bundle_installer','install-app.py')
activation=installer.activation
BIN=pathlib.Path(os.environ.get('OTRIOS_TEST_BIN_DIR',str(REPO/'target/debug'))).resolve()
CHECKER=BIN/'canvas-bundle';OPENSSL=legacy.OPENSSL

def run(*args):
    try:return subprocess.check_output([str(arg)for arg in args],text=True,stderr=subprocess.PIPE)
    except subprocess.CalledProcessError as error:
        error.add_note(error.stderr);raise
def port():
    with socket.socket()as listener:listener.bind(('127.0.0.1',0));return listener.getsockname()[1]

class SignedBundleInstallation(unittest.TestCase):
    def setUp(self):
        self.fixture=legacy.SignedReleases('test_published_protocol_matches_native_session_contract');self.fixture.setUp();self.addCleanup(self.fixture.tearDown)
        self.root=self.fixture.root;self.runtime=self.root
        (self.runtime/'build').mkdir();(self.runtime/'publisher').mkdir()
        shutil.copyfile(self.fixture.key,self.runtime/'publisher/private.pem');shutil.copyfile(self.fixture.public,self.runtime/'publisher/public.pem')
        self.key=self.runtime/'publisher/public.pem';self.private=self.runtime/'publisher/private.pem'
        apps=self.root/'apps';behavior=apps/'component-playground/behavior';behavior.mkdir(parents=True)
        shutil.copyfile(REPO/'apps/component-playground/behavior/main.wat',behavior/'main.wat')
        run(BIN/'canvas-pack',apps)
        unsigned=self.root/'unsigned';signed=self.root/'signed';unsigned.mkdir();signed.mkdir()
        for package in sorted(apps.glob('*/package')):
            app=package.parent.name;dest=unsigned/(app+'.otapp')
            run(CHECKER,'export',apps,app,dest,'--revision','1')
            run(CHECKER,'sign',dest,signed/(app+'.otapp'),'--key',self.private,'--openssl',OPENSSL)
        installed=self.root/'installed'
        run(CHECKER,'compose',apps/'catalog.flex',signed,installed,'--key',self.key,'--sign-key',self.private,'--openssl',OPENSSL)
        self.base=installer.publisher.publish(self.fixture.web,self.fixture.native,self.fixture.out,self.private,OPENSSL,catalog=installed/'catalog.flex',bundles=installed,bundle_checker=CHECKER)
        self.assertEqual(activation.verify(self.fixture.out,self.key,OPENSSL,json.dumps(self.base).encode(),bundle_checker=CHECKER),self.base['id'])
        self.trusted=port();self.public=port();self.server=None;self.addCleanup(self.stop)

    def stop(self):
        if self.server and self.server.poll()is None:self.server.terminate();self.server.wait(timeout=10)
        self.server=None

    def start(self):
        self.log=(self.root/'server.log').open('ab');self.addCleanup(self.log.close)
        self.server=subprocess.Popen([str(BIN/'dashboard-server'),'--bind',f'127.0.0.1:{self.trusted}','--public-bind',f'127.0.0.1:{self.public}','--releases',str(self.fixture.out),'--bundle-key',str(self.key),'--app-state',str(self.root/'state'),'--origin','http://bundle.test'],stdout=self.log,stderr=self.log)
        for _ in range(150):
            if self.server.poll()is not None:self.fail((self.root/'server.log').read_text())
            try:
                with urllib.request.urlopen(f'http://127.0.0.1:{self.trusted}/health',timeout=1)as response:
                    if json.load(response)['status']=='ok':return
            except urllib.error.HTTPError as error:error.close()
            except Exception:pass
            time.sleep(.1)
        self.fail((self.root/'server.log').read_text())

    def counter(self,name,step,revision):
        source=self.root/(name+'-source');unsigned=self.root/(name+'-unsigned.otapp');signed=self.root/(name+'.otapp')
        run(BIN/'examples/bundle_counter',source,step)
        run(CHECKER,'export',source,'bundle-counter',unsigned,'--revision',revision)
        run(CHECKER,'sign',unsigned,signed,'--key',self.private,'--openssl',OPENSSL)
        return signed

    def stage(self,bundle=None,remove=None):
        args=[sys.executable,HERE/'install-app.py','--runtime',self.runtime,'--checker',CHECKER,'--openssl',OPENSSL]
        args+=['--bundle',bundle]if bundle else['--remove',remove]
        run(*args)
        return json.loads((self.runtime/'build/app-install.json').read_bytes())

    def activate(self,pointer):
        file=self.root/'pending.json';file.write_text(json.dumps(pointer))
        # Local activation uses the same core checker as the native installation.
        with mock.patch.dict(os.environ,{'OTRIOS_BUNDLE_CHECKER':str(CHECKER)}):
            activation.activate(SimpleNamespace(pointer=file,releases=self.fixture.out,key=self.key,openssl=OPENSSL,notify_url=f'http://127.0.0.1:{self.trusted}/admin/activate'))

    def probe(self,release,action='read',app='bundle-counter',public=False):
        args=[BIN/'examples/bundle_app_probe',f'ws://127.0.0.1:{self.public if public else self.trusted}/session',self.fixture.out/release['id']/'apps/catalog.flex',app,action]
        if public:args.append('http://bundle.test')
        return run(*args)

    def test_install_upgrade_failure_restart_uninstall_and_explicit_rollback(self):
        self.start();pid=self.server.pid
        v1=self.counter('counter-v1',1,1);v2=self.counter('counter-v2',2,2)
        old=(self.fixture.out/'current.json').read_bytes();first=self.stage(v1)
        self.assertEqual((self.fixture.out/'current.json').read_bytes(),old)
        before=json.loads((self.fixture.out/self.base['id']/'manifest.json').read_bytes());after=json.loads((self.fixture.out/first['id']/'manifest.json').read_bytes())
        self.assertTrue(activation.agent.app_install_change(before,after));self.assertFalse(activation.agent.app_only_change(before,after))
        self.assertEqual({k:v for k,v in before['assets'].items()if k!='apps/catalog.flex'},{k:v for k,v in after['assets'].items()if k!='apps/catalog.flex'})
        with mock.patch.object(activation,'durable_release',side_effect=OSError('injected flush failure')):
            with self.assertRaisesRegex(OSError,'injected flush failure'):self.activate(first)
        self.assertEqual((self.fixture.out/'current.json').read_bytes(),old)
        self.activate(first);self.assertEqual(self.server.pid,pid)
        self.assertIn('value=1',self.probe(first,'increase'));self.assertIn('value=1',self.probe(first))
        second=self.stage(v2)
        cached=self.fixture.out/second['id']/'authority/bundles/bundle-counter.otapp/UI/main.flex';saved=cached.read_bytes();cached.write_bytes(b'corrupt staged cache')
        with self.assertRaises(subprocess.CalledProcessError):self.stage(v2)
        self.assertEqual((self.fixture.out/'current.json').read_bytes(),json.dumps(first,sort_keys=True).encode())
        cached.write_bytes(saved);self.activate(second)
        self.assertIn('value=3',self.probe(second,'increase'))
        self.assertIn('DENIED app=monitoring payloads=false',self.probe(second,'deny','monitoring',True))
        for name in after['authority']:
            with self.assertRaises(urllib.error.HTTPError)as error:urllib.request.urlopen(f'http://127.0.0.1:{self.public}/releases/{first["id"]}/{name}')
            self.assertEqual(error.exception.code,404)
            error.exception.close()
        pointer=(self.fixture.out/'current.json').read_bytes()
        for bad in [v1,self.counter('conflict',1,2)]:
            with self.assertRaises(subprocess.CalledProcessError):self.stage(bad)
            self.assertEqual((self.fixture.out/'current.json').read_bytes(),pointer)
        corrupt=self.root/'corrupt.otapp';shutil.copytree(v2,corrupt);(corrupt/'Code/behavior.wasm').write_bytes(b'corrupt')
        with self.assertRaises(subprocess.CalledProcessError):self.stage(corrupt)
        self.assertEqual((self.fixture.out/'current.json').read_bytes(),pointer)
        # Metadata/bytes cannot bypass inner signature checks by being re-signed
        # only as an outer release. Tampering fails before pointer advancement.
        forged=self.root/'forged';shutil.copytree(self.fixture.out/second['id'],forged)
        receipt='authority/bundles/bundle-counter.otapp/Integrity/publisher.flex'
        data=bytearray((forged/receipt).read_bytes());data[0]^=1;(forged/receipt).write_bytes(data)
        manifest=json.loads((forged/'manifest.json').read_bytes());manifest['authority'][receipt]={'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest()}
        raw=json.dumps(manifest,sort_keys=True,separators=(',',':')).encode();forged_id=hashlib.sha256(raw).hexdigest();(forged/'manifest.json').write_bytes(raw)
        run(OPENSSL,'pkeyutl','-sign','-rawin','-inkey',self.private,'-in',forged/'manifest.json','-out',forged/'manifest.sig')
        forged.rename(self.fixture.out/forged_id)
        bad=dict(second,id=forged_id,web_entry=f'/releases/{forged_id}/web/index.html',manifest=f'/releases/{forged_id}/manifest.json',signature=f'/releases/{forged_id}/manifest.sig')
        with self.assertRaisesRegex(ValueError,'Installed bundle publisher/closure rejected'):self.activate(bad)
        self.assertEqual((self.fixture.out/'current.json').read_bytes(),pointer)
        self.stop();self.start();self.assertIn('value=3',self.probe(second))
        removed=self.stage(remove='bundle-counter');self.activate(removed)
        with self.assertRaises(subprocess.CalledProcessError):self.probe(removed)
        self.assertTrue(any((self.root/'state').glob('bundle-counter-*.merkle')))
        self.activate(first);self.assertIn('value=3',self.probe(first));self.assertIn('value=4',self.probe(first,'increase'))
        self.assertTrue((self.fixture.out/first['id']/'authority/bundles/bundle-counter.otapp/Integrity/publisher.flex').exists())
        # The established scoped UI authoring command can consume a signed bundle
        # baseline, without losing independently installed apps. Source-fingerprint
        # behavior has its own tests; this fixture isolates its package bridge.
        scoped=module('scoped_bundle_publish','publish-app.py')
        checker=self.runtime/'build/mac-target/debug/canvas-bundle';checker.parent.mkdir(parents=True);checker.symlink_to(CHECKER)
        record=self.root/'qualified-runtime.json';record.write_text(json.dumps({'runtime_inputs':'fixture-runtime'}))
        args=SimpleNamespace(runtime=self.runtime,repo=REPO,record=record,app='component-playground',kind='ui',openssl=OPENSSL,baseline=self.fixture.out/'current.json',deploy_config=None)
        source=REPO/'apps/component-playground';sources={'ui.rs':scoped.file_hash(source/'ui.rs')}
        with mock.patch.object(scoped,'runtime_inputs',return_value='fixture-runtime'):
            repeated=scoped.publish_bundle(args,self.fixture.out/first['id'],sources,source)
        self.assertEqual(repeated['id'],first['id']);self.assertIn('value=4',self.probe(first))

if __name__=='__main__':
    unittest.main()
