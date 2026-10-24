# SPDX-License-Identifier: Apache-2.0
import importlib.util,pathlib,subprocess,tempfile,unittest
spec=importlib.util.spec_from_file_location('home_publish',pathlib.Path(__file__).with_name('publish-app-ui.py'));home=importlib.util.module_from_spec(spec);spec.loader.exec_module(home)
class HomePublish(unittest.TestCase):
 def test_runtime_provenance_excludes_only_home_ui_and_tracks_added_inputs(self):
  with tempfile.TemporaryDirectory() as tmp:
   repo=pathlib.Path(tmp);subprocess.run(['git','init','-q',str(repo)],check=True)
   for name in ['apps/home/ui.rs','crates/core/src/lib.rs','Cargo.lock']:
    p=repo/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text('baseline')
   initial=home.runtime_inputs(repo);(repo/'apps/home/ui.rs').write_text('new title');self.assertEqual(initial,home.runtime_inputs(repo))
   (repo/'crates/core/src/lib.rs').write_text('runtime changed');self.assertNotEqual(initial,home.runtime_inputs(repo))
   (repo/'crates/core/src/lib.rs').write_text('baseline');(repo/'apps/home/extra.rs').write_text('new dependency');self.assertNotEqual(initial,home.runtime_inputs(repo))
 def test_private_behavior_and_runtime_changes_require_a_full_release(self):
  import copy
  base={'ui_abi':'otrios-app-ui-v1','sync_protocol':'v4','assets':{'apps/catalog.flex':1,'native':2},'authority':{'authority/apps/home/ui.flex':3,'authority/apps/home/behavior.wasm':4,'authority/apps/other/ui.flex':5}}
  next=copy.deepcopy(base);next['assets']['apps/catalog.flex']=6;next['authority']['authority/apps/home/ui.flex']=7;self.assertTrue(home.agent.ui_only_change(base,next))
  for section,key in [('assets','native'),('authority','authority/apps/home/behavior.wasm'),('authority','authority/apps/home/behavior.wasm')]:
   bad=copy.deepcopy(next);bad[section][key]=9;self.assertFalse(home.agent.ui_only_change(base,bad))
if __name__=='__main__':unittest.main()
