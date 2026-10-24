# SPDX-License-Identifier: Apache-2.0
import importlib.util,pathlib,subprocess,tempfile,unittest
spec=importlib.util.spec_from_file_location('app_publish',pathlib.Path(__file__).with_name('publish-app.py'));publisher=importlib.util.module_from_spec(spec);spec.loader.exec_module(publisher)
class AppPublish(unittest.TestCase):
 def test_runtime_provenance_excludes_ui_and_tracks_added_inputs(self):
  with tempfile.TemporaryDirectory() as tmp:
   repo=pathlib.Path(tmp);subprocess.run(['git','init','-q',str(repo)],check=True)
   for name in ['apps/home/ui.rs','crates/core/src/lib.rs','Cargo.lock']:
    p=repo/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text('baseline')
   initial=publisher.runtime_inputs(repo);(repo/'apps/home/ui.rs').write_text('new title');self.assertEqual(initial,publisher.runtime_inputs(repo))
   (repo/'crates/core/src/lib.rs').write_text('runtime changed');self.assertNotEqual(initial,publisher.runtime_inputs(repo))
   (repo/'crates/core/src/lib.rs').write_text('baseline');(repo/'apps/home/extra.rs').write_text('new dependency');self.assertNotEqual(initial,publisher.runtime_inputs(repo))
 def test_behavior_source_is_not_a_host_build_input_but_abi_and_schema_are(self):
  with tempfile.TemporaryDirectory() as tmp:
   repo=pathlib.Path(tmp);subprocess.run(['git','init','-q',str(repo)],check=True)
   source=repo/'apps/demo/behavior/main.wat';source.parent.mkdir(parents=True);source.write_text('(module)')
   initial=publisher.runtime_inputs(repo);source.write_text('(module (func))');self.assertEqual(initial,publisher.runtime_inputs(repo))
   contract=source.parent/'events.rs';contract.write_text('pub const ABI: u32=2;');self.assertNotEqual(initial,publisher.runtime_inputs(repo))
 def test_private_behavior_and_runtime_changes_require_a_full_release(self):
  import copy
  base={'ui_abi':'otrios-app-ui-v1','sync_protocol':'v4','assets':{'apps/catalog.flex':1,'native':2},'authority':{'authority/apps/home/ui.flex':3,'authority/apps/home/behavior.wasm':4,'authority/apps/other/ui.flex':5}}
  next=copy.deepcopy(base);next['assets']['apps/catalog.flex']=6;next['authority']['authority/apps/home/ui.flex']=7;self.assertTrue(publisher.agent.ui_only_change(base,next))
  for section,key in [('assets','native'),('authority','authority/apps/home/behavior.wasm')]:
   bad=copy.deepcopy(next);bad[section][key]=9;self.assertFalse(publisher.agent.ui_only_change(base,bad))
if __name__=='__main__':unittest.main()
