<!-- SPDX-License-Identifier: Apache-2.0 -->
# Frame authority

The RK3126 hosts the Rust authority, signed browser/native release store and
Cloudflare Tunnel for `https://otrios.otri.ca`. The existing native display uses
the authority's trusted loopback listener. Prometheus and Grafana stay on Linode;
a frame timer queries Prometheus directly over HTTPS/Tailscale and publishes a
complete local Monitoring view. The Mac is only a builder and signing publisher.

## Services and boundaries

| Service | Role | Bound |
| --- | --- | --- |
| `otrios-authority` | Code, release hello/upgrades, durable app roots, auth, ink | 4 trusted / 8 public sessions; 192 MiB memory high, 256 MiB hard |
| `otrios-tunnel` | Existing hostname to `127.0.0.1:8877` | 96 MiB high, 128 MiB hard |
| `otrios-monitoring.timer` | Five-minute HTTPS collector and native chart projection | One-shot 240 seconds, 96 MiB hard |
| `otrios-dashboard-direct` | Existing KMS/GLES2 display and touch | Separate process; 128 MiB artifact disk cache |

These are configured limits, not measured working sets or capacity guarantees.
Both HTTP listeners remain loopback-only. The tunnel exposes only the public
listener. Native state and code use `http://127.0.0.1:8876`, including the stable
v1 hello on `/session`; remote code still requires HTTPS and signed native bytes.
Browser clients retain the existing cookie/private-admission boundaries.

No kernel, graphics driver, panel, rotation, calibration, CMA or zram policy
changes are involved. Local confirmed edits and cached code remain usable when
the Internet is unavailable; public access and fresh metrics require networking.
Frame power and home networking determine public site availability.

## Installation layout

- `/opt/otrios-authority`: ARM `dashboard-server`, `dashboard-monitoring`, verified
  ARM `cloudflared`, collector and activation helpers.
- `/etc/otrios-authority`: service-owned mode-0600 Monitoring login hash,
  dedicated Prometheus Basic credential, collector config, publisher public key
  and the existing tunnel credential/config. No signing private key or Linode SSH
  key is installed on the frame.
- `/var/lib/otrios-authority`: `releases/current.json`, the complete active signed
  release, `state.apps`, and the latest complete private presentation.
- `/opt/otrios-dashboard` and `/var/lib/otrios-dashboard`: existing native
  supervisor, bootstrap helper and independent disposable cache.

Units in `tools/dashboard/` run the authority/tunnel/collector as the `otrios`
service user. Initial provisioning creates that user and installs the units and
private configuration. Verify the signed store before starting the authority.
The native service uses `/etc/otrios-dashboard.conf` with both `DASHBOARD_URL`
and `DASHBOARD_CODE_URL` set to `http://127.0.0.1:8876`.

## Build and publication

Rust 1.90.0 and Cargo.lock remain pinned; there are no new crate dependencies.
`bash scripts/build-dashboard-armv7.sh` cross-compiles the authority, chart
renderer, direct host, bootstrap helper and app packer on the build machine.

The private watcher config accepts:

```json
{
  "frame": {
    "ssh": ["ssh", "-oBatchMode=yes", "-oStrictHostKeyChecking=yes",
            "-oUserKnownHostsFile=/PRIVATE/pinned-known-hosts", "root@FRAME_TAILNET_IP"]
  }
}
```

Keep the other established builder settings and the publisher keys on the Mac.
The watcher builds browser/ARM artifacts, signs and stages the complete replica,
then invokes `deploy-dashboard-frame.py`. Explicit deployment is also supported:

```sh
python3 scripts/deploy-dashboard-frame.py --config /PRIVATE/config.json \
  --pointer /PRIVATE/build/staged-release.json \
  --server /PRIVATE/build/frame-dashboard-server \
  --monitoring /PRIVATE/build/frame-dashboard-monitoring \
  --bootstrap /PRIVATE/build/session-bootstrap
```

The receiver verifies every signed asset before changing the pointer. Authority
executables and helpers are installed through trusted SSH; they are not part of
the signed renderer update manifest. Server byte changes restart the authority,
which revokes RAM-only browser sessions; renderer-only updates retain hot reload.
`/admin/activate` publishes upgrades after the complete replica is installed.
Release discovery remains independent of state negotiation/recovery.

Published stores retain only the active release. The renderer temporarily keeps
its running release during activation; an active-only cache prune can follow
confirmed readiness. Old open browser releases may need a launcher-root reload
after pruning. There is no historical rollback promise.

```sh
python3 scripts/prune-dashboard-releases.py --releases /var/lib/otrios-authority/releases --apply
python3 scripts/prune-dashboard-releases.py --releases /var/lib/otrios-dashboard --native-cache --apply
```

The pruner locks authority activation, refuses missing/invalid pointers and
symlink release entries, and removes only obsolete digest-named directories.
Stop the native updater before manually pruning its cache during a cutover.
It never deletes durable app roots or unrelated hardware recovery backups.

## Hard cutover and checks

Qualify a disposable ARM authority first. Stop the Mac watcher, collector and
authority, copy the stopped canonical app checkpoints and journals plus latest
presentation, and transfer ownership to the frame service user. Verify file
digests before starting it. Stop the Mac tunnel before starting the frame tunnel;
two tunnel connectors must not route to independently writable authorities.
Start/enable the frame authority, display, timer and tunnel, then retire the Mac
serving LaunchAgents. Keep only the active release; preserve canonical app data.

Run `bash scripts/check.sh`, all-feature Clippy, browser worker/session/cache
tests, real server-process checks, and the public host conformance checker.
On hardware verify stable loopback hello, native signed/confirmed readiness,
direct Prometheus collection, shared browser/native mounts, peer admission limits,
durable restart, public/private route isolation and release activation.
Measure service memory, login peaks and rendering/ACK behavior under browser
load. A framebuffer capture does not establish physical touch/color acceptance.

```sh
systemctl status otrios-authority otrios-tunnel otrios-monitoring.timer otrios-dashboard-direct
curl -fsS http://127.0.0.1:8876/health
journalctl -u otrios-authority -u otrios-monitoring -u otrios-tunnel --since today
```
