<!-- SPDX-License-Identifier: Apache-2.0 -->
# Monitoring app data and local collection

Open **Monitoring** in the Operations Space. Operations membership is required
for remote viewers. Frame's existing `frame-household` identity is a Member,
without administration privileges. Frame also explicitly owns a local Monitoring
installation, so its display can start and refresh while OrbStack is unavailable.
It uses native GLES2; browsers use the same Rust chart projection through WebGL.

## Panels and query semantics

- Current scrape health for Gallery production, RC, staging and WordPress.
- A common seven-day health timeline: `min_over_time(up{job=...}[1h])`, evaluated hourly. A downward dip means at least one failed scrape in that hour. The card's `N/M hourly windows clear` is a count of evaluated windows, **not an uptime percentage or outage duration**. This monitors the configured metrics scrape endpoints, not full user journeys.
- Production `bundle_count`: hourly point samples, plus a separate current instant value. Small changes can look nearly flat on the explicitly labelled zero-based axis.
- Production `memory_usage_heap_used`: hourly means in MiB, plus a separate current instant value. The chart's scale includes both the history and current reading.
- Current WordPress database status, Apache busy workers, English/Japanese post counts and days since backup.

All range queries share an aligned end hour and start exactly seven days earlier, with a 3,600-second step (169 inclusive evaluation timestamps). UTC bounds are printed on the plots. **AS OF** is formatted locally by the viewer:
browsers use their detected timezone (with the collection date's DST offset),
and Frame is configured with `TZ=Asia/Tokyo` to display JST. Canonical timestamps
and Merkle roots are unchanged by this presentation setting. The newest history point can be up to an hour behind collection; current cards use instant queries at the collection time. Hourly point samples can miss short-lived bundle/count changes; health and memory use the aggregations described above. The first health bucket also looks back an hour before the first evaluation timestamp.

Absent/nonfinite samples remain missing. Paths split at gaps while retaining their original horizontal time positions; isolated valid samples render as dots. Missing current values say `NO DATA`, never the old demo samples. Multiple matching series are rejected rather than silently selecting an arbitrary instance. Prometheus warnings, invalid timestamps, overlarge responses, query failures and invalid binary scenes retain the previous complete view. Its **AS OF** label stays unchanged, so it must not be interpreted as a fresh health check. There is no automatic stale colour/alarm yet.

This is deliberately a subset: no Grafana embedding, dashboard editor, arbitrary PromQL, time-range picker, hover inspection, alert management or interactive chart controls.

## Ownership and replication

Monitoring owns typed app data: eleven metrics, current readings, 169 hourly
samples per metric (including gaps), UTC window metadata and collection time.
Frame's collector queries Prometheus directly, then atomically commits a canonical
Merkle snapshot to `/var/lib/otrios-monitoring/data.merkle`. The root binds the
collection/feed ID, producer and monotonic revision. Invalid responses or clock
regressions leave the previous complete snapshot intact. Rendered charts are
derived locally; chart geometry is not replicated app state.

A separate publisher on Frame initiates the outbound
`wss://otrios.otri.ca/session` connection. It offers the newest root using channel
6 after the stable v1 hello. OrbStack checks the existing collector service grant
and current app/collection attachment on every request, requests a Merkle delta
or full snapshot, validates and durably commits it before acknowledging. Reconnect
publishes the newest revision; intermediate refreshes need not be queued.
No inbound Frame listener or OrbStack-to-Frame connection is needed.

OrbStack stores the same root in the Monitoring collection's `.data/` directory.
Authorized peers receive typed snapshots on their existing channel-1 Monitoring
lease and derive their own charts. Viewer membership and the producer grant are
separate; the Frame viewing token cannot publish, and human Admin status does not
grant collector publication. The old layout-upload contract is rejected for the
new typed Monitoring presentation. See [RFC 0038](rfcs/0038-local-monitoring-replication.md).

Remote viewers retain private snapshots in RAM under their revocable lease.
The explicitly configured local owner keeps its own durable data and signed UI;
server membership revocation cannot recall those locally owned bytes. This is
not general offline authorization or a durable browser cache.

## Failure behavior

- OrbStack down: Frame continues collecting from Prometheus and displaying local
  committed data, including after a display restart. Other peers need a reachable
  authorized server for fresh data. The publisher reconnects and catches up.
- Prometheus or Frame networking down: the previous snapshot and **AS OF** time
  remain; failure does not create a fresh health reading.
- Frame down: OrbStack and viewers retain the last published snapshot and its
  collection time. There is no second active collector or automatic failover.

This is a bounded single-producer data path, not a multi-authority merge engine.
Snapshots are limited to 128 KiB; the fixed metric schema and existing scene
limits apply. History is a replaceable seven-day window, not an unbounded archive.
There is no automatic stale alarm yet.

## Active Frame services

| Unit | Role |
| --- | --- |
| `otrios-monitoring-local.timer` | Runs the collector every five minutes |
| `otrios-monitoring-local.service` | HTTPS queries and atomic local Merkle commit |
| `otrios-monitoring-publish.service` | Outbound publication/reconnection, independent of collection |
| `otrios-dashboard-direct.service` | Reads the local snapshot and renders the signed Monitoring UI |

Private configuration is in `/etc/otrios-monitoring/`. `config.json` contains
`prometheus_url`, `prometheus_credentials`, `renderer`, `local_data`, `feed` and
`producer`. The renderer is `/opt/otrios-monitoring/monitoring-tool.py`, which
verifies the existing publisher signature and collector artifact hash before
executing the current signed ARM binary. `collector.identity` reuses the existing
producer credential; `publish.conf` sets `MONITORING_SESSION` to the WSS URL.
Prometheus credentials and tokens remain private mode-600 service files, outside
app snapshots and releases. The display process does not query Prometheus.

The native service drop-in sets
`OTRIOS_MONITORING_DIR=/var/lib/otrios-monitoring`. That directory contains
`data.merkle`, its writer lock, verified `ui.flex`, and technical publication ACK
metadata in `data.published.json`. Signed UI admission still checks the installed
catalog; device-local uninstall remains respected.

```sh
systemctl start otrios-monitoring-local.service
/opt/otrios-monitoring/monitoring-tool.py --inspect /var/lib/otrios-monitoring/data.merkle
journalctl -u otrios-monitoring-publish.service --no-pager -n 20
```

HTTPS is mandatory for Prometheus, redirects are rejected, responses and the
external JSON window are bounded at 2 MiB, and collection has a 180-second request
budget. JSON exists only at the external query adapter boundary; canonical app
data uses the existing FlexBuffers/Merkle representation.

OrbStack's former `otrios-monitoring.timer` is disabled. The collector and
publisher are on Frame; OrbStack retains the authority, access control and tunnel.
Do not reactivate the retired Frame authority or its old collector units.
See [hosting](FRAME_HOSTING.md) and the
[live acceptance record](experiments/monitoring-local-replication-2026-11-22.md).
