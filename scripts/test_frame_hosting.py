# SPDX-License-Identifier: Apache-2.0
"""Frame consumer and active-only release retention boundaries."""
import importlib.util, json, pathlib, tempfile, unittest
from unittest.mock import patch

def module(name,file):
    spec=importlib.util.spec_from_file_location(name,pathlib.Path(__file__).with_name(file))
    value=importlib.util.module_from_spec(spec);spec.loader.exec_module(value);return value

collector=module('collector','collect-monitoring.py')
pruner=module('pruner','prune-dashboard-releases.py')
host_check=module('host_check','test-dashboard-host.py')

class FrameHosting(unittest.TestCase):
    def test_navigation_shell_allows_only_catalog_title_variations(self):
        root=b'<title>OTriOS</title><meta property="og:title" content="OTriOS"><script src="/releases/abc/web/app.js"></script>'
        app=root.replace(b'<title>OTriOS</title>',b'<title>OTriOS Monitoring</title>').replace(b'content="OTriOS"',b'content="Monitoring"')
        self.assertEqual(host_check.entry_shell(root),host_check.entry_shell(app))
        self.assertNotEqual(host_check.entry_shell(root),host_check.entry_shell(app.replace(b'/abc/',b'/def/')))
        self.assertNotEqual(host_check.entry_shell(root),host_check.entry_shell(app+b'<p>unexpected body</p>'))

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

    def test_bound_collector_keeps_token_out_of_urls_and_rejects_readable_credentials(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=pathlib.Path(tmp);output=root/'view.flex';output.write_bytes(b'previous')
            identity=root/'collector.identity';identity.write_text('a'*64);identity.chmod(0o600)
            requests=[]
            class Response:
                def __enter__(self):return self
                def __exit__(self,*args):pass
                def read(self,n):return b'view-digest'
            class Opener:
                def open(self,request,timeout):requests.append(request);return Response()
            def render(args,**kwargs):pathlib.Path(args[1]).write_bytes(b'new-view')
            config={'output':str(output),'renderer':'fixture-renderer','authority_identity':str(identity)}
            with patch.object(collector,'collect',return_value=b'{"collected_label":"fixture"}'),patch.object(collector.subprocess,'run',side_effect=render),patch.object(collector.urllib.request,'build_opener',return_value=Opener()) as build:
                collector.refresh(config)
                self.assertIsInstance(build.call_args.args[0],collector.NoRedirect)
                self.assertEqual(requests[0].full_url,'http://127.0.0.1:8876/presentation')
                self.assertEqual(requests[0].get_header('Authorization'),'Bearer '+'a'*64)
                self.assertEqual(output.read_bytes(),b'new-view')
                identity.chmod(0o644)
                with self.assertRaises(ValueError):collector.refresh(config)
                self.assertEqual(len(requests),1)
                self.assertEqual(output.read_bytes(),b'new-view')

if __name__=='__main__':unittest.main()
