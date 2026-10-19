# SPDX-License-Identifier: Apache-2.0
"""Frame consumer and active-only release retention boundaries."""
import importlib.util, json, pathlib, tempfile, unittest
from unittest.mock import patch

def module(name,file):
    spec=importlib.util.spec_from_file_location(name,pathlib.Path(__file__).with_name(file))
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value

collector=module('collector','collect-monitoring.py')
pruner=module('pruner','prune-dashboard-releases.py')

class FrameHosting(unittest.TestCase):
    def test_pruning_keeps_active_and_unrelated_data_and_fails_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=pathlib.Path(tmp);active='a'*64;old='b'*64
            for name in [active,old,'state.apps']:
                (root/name).mkdir();(root/name/'data').write_bytes(b'state')
            (root/'current.json').write_text(json.dumps({'id':active}))
            self.assertEqual(pruner.prune(root)['obsolete'],1);self.assertTrue((root/old).exists())
            self.assertEqual(pruner.prune(root,True)['bytes'],5)
            self.assertTrue((root/active/'data').exists());self.assertTrue((root/'state.apps/data').exists())
            (root/'current.json').write_text(json.dumps({'id':'c'*64}))
            with self.assertRaises(ValueError):pruner.prune(root,True)
            self.assertTrue((root/active).exists())

    def test_direct_queries_keep_one_window_and_never_redirect_credentials(self):
        with tempfile.TemporaryDirectory() as tmp:
            auth=pathlib.Path(tmp)/'auth.json';auth.write_text(json.dumps({'username':'consumer','password':'test'}));auth.chmod(0o600)
            config={'prometheus_url':'https://metrics.example/prometheus/','prometheus_credentials':str(auth)}
            requests=[]
            class Response:
                def __enter__(self):return self
                def __exit__(self,*args):pass
                def read(self,n):return b'{"status":"success","data":{"result":[]}}'
            class Opener:
                def open(self,request,timeout):requests.append(request);return Response()
            with patch.object(collector.urllib.request,'build_opener',return_value=Opener()) as build:
                data=json.loads(collector.collect(config));self.assertIsInstance(build.call_args.args[0],collector.NoRedirect)
            self.assertEqual(len(requests),22);self.assertEqual(data['end']-data['start'],604800)
            self.assertEqual(data['end']%3600,0)
            self.assertTrue(all(r.full_url.startswith('https://metrics.example/prometheus/api/v1/') for r in requests))
            self.assertTrue(all(r.get_header('Authorization').startswith('Basic ') for r in requests))
            with self.assertRaises(ValueError):collector.NoRedirect().redirect_request(None,None,302,'',{},'https://other.example')
            for url in ['http://metrics.example','https://user:pass@metrics.example','https://metrics.example?token=test']:
                with self.assertRaises(ValueError):collector.collect(dict(config,prometheus_url=url))
            auth.chmod(0o644)
            with self.assertRaises(ValueError):collector.collect(config)

    def test_failed_collection_does_not_publish_or_replace_prior_output(self):
        with tempfile.TemporaryDirectory() as tmp:
            output=pathlib.Path(tmp)/'view.flex';output.write_bytes(b'previous')
            with patch.object(collector,'collect',side_effect=ValueError('query failed')),patch.object(collector.urllib.request,'urlopen',side_effect=AssertionError('Unexpected publication')):
                with self.assertRaises(ValueError):collector.refresh({'output':str(output)})
            self.assertEqual(output.read_bytes(),b'previous')

if __name__=='__main__':unittest.main()
