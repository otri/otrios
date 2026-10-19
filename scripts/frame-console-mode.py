#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Root-only service hook: preserve console mode around exclusive KMS ownership."""
import fcntl,json,os,pathlib,struct,sys
state=pathlib.Path('/run/otrios-direct-console/state.json')
if os.geteuid()!=0:raise SystemExit('Console hook must run as root')
operation=sys.argv[1]
if operation not in ('enter','restore'):raise SystemExit('Use enter or restore')
if operation=='restore' and not state.exists():raise SystemExit(0)
fd=os.open('/dev/tty1',os.O_RDWR)
try:
    if operation=='enter':
        b=bytearray(4);fcntl.ioctl(fd,0x4b3b,b);mode=struct.unpack('i',b)[0]
        handle=os.open(state,os.O_WRONLY|os.O_CREAT|os.O_EXCL,0o600)
        with os.fdopen(handle,'w') as f:json.dump({'mode':mode},f)
        fcntl.ioctl(fd,0x4b3a,1)
    else:
        mode=json.loads(state.read_text())['mode']
        if mode not in (0,1,2):raise ValueError('Invalid saved console mode')
        fcntl.ioctl(fd,0x4b3a,mode);state.unlink()
finally:os.close(fd)
