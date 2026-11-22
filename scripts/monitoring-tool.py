#!/usr/bin/python3
# SPDX-License-Identifier: Apache-2.0
"""Run the Monitoring tool only from Frame's current publisher-verified release."""
import importlib.util,json,os,pathlib,sys
cache=pathlib.Path('/var/lib/otrios-dashboard')
pointer=cache/'last-good.json'
spec=importlib.util.spec_from_file_location('agent','/opt/otrios-dashboard/frame-dashboard-agent.py')
agent=importlib.util.module_from_spec(spec);spec.loader.exec_module(agent)
def read(path,limit):
    if path.is_symlink():raise ValueError('Monitoring code symlink rejected')
    with path.open('rb') as file:data=file.read(limit+1)
    if len(data)>limit:raise ValueError('Monitoring code budget exceeded')
    return data
release=json.loads(read(pointer,8192))['id'];agent.valid_release(release)
folder=cache/release
if cache.is_symlink() or folder.is_symlink():raise SystemExit('Monitoring code directory symlink rejected')
manifest=agent.verify_manifest(read(folder/'manifest.json',65536),read(folder/'manifest.sig',64),release,pathlib.Path('/var/lib/otrios-monitoring'),pathlib.Path('/opt/otrios-dashboard/publisher.pem'),'/usr/bin/openssl')
tool=folder/'dashboard-monitoring'
item=manifest['assets'].get('native/armv7/dashboard-monitoring')
if not item:raise SystemExit('Current signed release has no Monitoring collector')
agent.verify_binary(read(tool,agent.MAX_BINARY),item)
os.environ['OTRIOS_MONITORING_RELEASE_ID']=release
os.environ['OTRIOS_MONITORING_RELEASE_POINTER']=str(pointer)
os.execv(str(tool),[str(tool),*sys.argv[1:]])
