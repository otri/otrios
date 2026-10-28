# Prometheus monitoring view

Open **Monitoring** from the [Home Screen](HOME_AND_INPUT.md), or use the browser `#/monitoring` route. Back/Home navigate locally while the collector continues refreshing.

This read-only dashboard recreates a subset of Grafana's **SPD & M3D — Overall Service Status** in the shared primitive compositor. The native GLES2 photo frame and authorized Wasm/WebGL browser sessions receive the same complete FlexBuffers presentation. Public demo visitors receive none of that view. The frame does not run Grafana or a browser.

## Panels and query semantics

- Current scrape health for Gallery production, RC, staging and WordPress.
- A common seven-day health timeline: `min_over_time(up{job=...}[1h])`, evaluated hourly. A downward dip means at least one failed scrape in that hour. The card's `N/M hourly windows clear` is a count of evaluated windows, **not an uptime percentage or outage duration**. This monitors the configured metrics scrape endpoints, not full user journeys.
- Production `bundle_count`: hourly point samples, plus a separate current instant value. Small changes can look nearly flat on the explicitly labelled zero-based axis.
- Production `memory_usage_heap_used`: hourly means in MiB, plus a separate current instant value. The chart's scale includes both the history and current reading.
- Current WordPress database status, Apache busy workers, English/Japanese post counts and days since backup.

All range queries share an aligned end hour and start exactly seven days earlier, with a 3,600-second step (169 inclusive evaluation timestamps). UTC bounds are printed on the plots. The newest history point can be up to an hour behind collection; current cards use instant queries at the collection time. Hourly point samples can miss short-lived bundle/count changes; health and memory use the aggregations described above. The first health bucket also looks back an hour before the first evaluation timestamp.

Absent/nonfinite samples remain missing. Paths split at gaps while retaining their original horizontal time positions; isolated valid samples render as dots. Missing current values say `NO DATA`, never the old demo samples. Multiple matching series are rejected rather than silently selecting an arbitrary instance. Prometheus warnings, invalid timestamps, overlarge responses, query failures and invalid binary scenes retain the previous complete view. Its **AS OF** label stays unchanged, so it must not be interpreted as a fresh health check. There is no automatic stale colour/alarm yet.

This is deliberately a subset: no Grafana embedding, dashboard editor, arbitrary PromQL, time-range picker, hover inspection, alert management or interactive chart controls.

## Data path and bounded replacement

`scripts/collect-monitoring.py` runs on the photo frame and queries the existing
Prometheus HTTPS API over Tailscale. It uses fixed, read-only queries and a
private Basic-auth credential; neither browsers nor the native display receive
that credential. It does not require SSH or the Mac. JSON is confined to this
external adapter boundary and an optional private evidence cache.

`dashboard-monitoring` consumes the window on stdin and authors typed component/instance data directly into FlexBuffers. It reuses Screen, Text and Panel templates and adds Path/Circle templates for plots. It formats units, normalizes plotted values against labelled axes and resolves data into a complete presentation; raw metric values are not a general client-queryable telemetry schema in this PoC. The same existing primitive interpreter expands and validates the result on both hosts. There is no JSON scene representation or JavaScript chart renderer.

The collector posts the complete binary view to trusted `/presentation`. The
server persists only the latest complete aggregate, validates it before replacing
the current view and sends it as a replaceable Canvas `View` on an authorized
Monitoring lease. It uses channel 1 of the same negotiated `/session` connection;
there is no OTML1 side stream or global editable layout register.

Clients retain the private aggregate only in RAM for that activation. Refreshes
do not append metrics or binary layouts to durable Merkle documents. Revocation
drops the private activation/view. Clearing the aggregate reveals the Monitoring
app's empty-feed presentation, rather than an old global dashboard. A disconnected
view's AS OF label is unchanged and must not be treated as fresh data.

Each presentation remains limited to 128 KiB, 128 instances and 512 expanded nodes. Literal series allow at most 256 points. Extremely fragmented history that exceeds the scene limits is rejected and leaves the previous view intact. A general content-addressed raw telemetry store, independent data-binding schema and per-chart delta protocol are future work.

## Build and collect

```sh
cargo build --release --locked -p otrios-dashboard --features server \
  --bin dashboard-server --bin dashboard-monitoring
python3 scripts/collect-monitoring.py --config /etc/otrios-authority/monitoring-config.json
```

Private config example (`/etc/otrios-authority/monitoring-config.json`):

```json
{
  "prometheus_url": "https://monitoring.steampunk.digital/prometheus",
  "prometheus_credentials": "/etc/otrios-authority/prometheus-credentials.json",
  "renderer": "/opt/otrios-authority/dashboard-monitoring",
  "output": "/var/lib/otrios-authority/monitoring.flex"
}
```

The credential file contains `username` and `password`, is owned by the service
user and has mode 0600. HTTPS is mandatory; redirects are rejected so credentials
cannot be forwarded to another destination. Each response and the full window
are bounded at 2 MiB, and collection has a 180-second request budget.
The optional `evidence` path retains the private query window. Publication remains
fixed to the trusted local `http://127.0.0.1:8876/presentation` endpoint.

`otrios-monitoring.timer` runs the one-shot service five minutes apart, starting
30 seconds after boot. A failed collection leaves the prior complete view and
its AS OF timestamp unchanged; the failed unit remains visible to systemd.
`--watch` also supports a foreground loop. The frame needs Tailscale and network
access to refresh; a disconnected view must not be read as a fresh health check.
The former Mac collector LaunchAgent is retired.

## Inspect and restore

Trusted `GET /presentation` returns the current binary aggregate. Trusted
`POST /presentation` with `application/octet-stream` replaces it atomically; an
empty body clears it. Public clients receive it only after Monitoring admission,
through the Canvas lease on `/session`; authenticated HTTP inspection is also
available at `/monitoring/presentation`. There is no separate Monitoring socket.
`/layout` and the old global dashboard editor are retired. `/health` reports app
roots, release and presentation availability, without returning metric values.

The collector owns its view templates locally in `apps/monitoring/`; it no longer
borrows the global dashboard seed. New aggregates use layout v2. Existing static
v1 views remain readable within the bounded layout contract.

Stop the OTriOS collector before clearing its presentation, otherwise the next
scheduled collection restores the view. This affects only OTriOS; Grafana and
Prometheus keep running. Confirmed Canvas app state is independent of the
replaceable Monitoring aggregate. See [live operations](FRAME_HOSTING.md).
