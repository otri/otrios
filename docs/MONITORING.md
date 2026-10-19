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

`scripts/collect-monitoring.py` uses an existing SSH alias to run fixed, read-only queries against Prometheus on the server's loopback port. It changes no monitoring configuration, exporters, firewall or authentication. It fetches only allowlisted queries through the [Prometheus HTTP API](https://prometheus.io/docs/prometheus/latest/querying/api/); JSON is confined to that external adapter boundary and the private evidence cache.

`dashboard-monitoring` consumes the window on stdin and authors typed component/instance data directly into FlexBuffers. It reuses Screen, Text and Panel templates and adds Path/Circle templates for plots. It formats units, normalizes plotted values against labelled axes and resolves data into a complete presentation; raw metric values are not a general client-queryable telemetry schema in this PoC. The same existing primitive interpreter expands and validates the result on both hosts. There is no JSON scene representation or JavaScript chart renderer.

The collector posts the complete binary view to `/presentation`. This is a **replaceable presentation**, separate from the editable `/layout` register and Automerge history. The server persists only the latest presentation beside its state file, validates it before activation, and pushes it to trusted native peers and separately authorized browser Monitoring subscriptions as `OTML1` followed by FlexBuffers bytes. A five-byte `OTML1` message clears the presentation. Peers must upgrade to this protocol before enabling a feed. The server coalesces announcements; native delivery coalesces a paired document/presentation so a newer CRDT message cannot drop a pending metrics view.

`Replica::set_presentation` creates a complete immutable view while retaining old pinned snapshots. Normal CRDT edits/merges preserve the presentation; clearing it reveals the saved collaborative layout and state. Refreshing the feed does not append telemetry blobs or hashes to Automerge. The server also validates the underlying layout, so the overlay cannot conceal a malformed collaborative layout. The native host caches the latest binary presentation beside its state; authorized browsers retain it only in memory while the Monitoring subscription is open. Neither cache makes the feed fresh while disconnected.

Each presentation remains limited to 128 KiB, 128 instances and 512 expanded nodes. Literal series allow at most 256 points (the demo state binding remains at its previous 32 samples). Extremely fragmented history that exceeds the scene limits is rejected and leaves the previous view intact. A general content-addressed raw telemetry store, independent data-binding schema and per-chart delta protocol are future work.

## Build and collect

```sh
cargo build --release --locked -p otrios-dashboard --features server \
  --bin dashboard-server --bin dashboard-monitoring
python3 scripts/collect-monitoring.py --config /PRIVATE/PATH/monitoring-config.json
```

Private config example, kept outside the repository:

```json
{
  "ssh_host": "your-existing-ssh-alias",
  "renderer": "/ABSOLUTE/PATH/dashboard-monitoring",
  "output": "/PRIVATE/PATH/monitoring.flex",
  "evidence": "/PRIVATE/PATH/monitoring-window.json"
}
```

The `evidence` cache is optional. The renderer does not read SSH settings or credentials. The collector accepts only a simple SSH alias and always publishes to `http://127.0.0.1:8876/presentation`; no client-selected Prometheus URL or arbitrary query execution endpoint is exposed. Use `--watch` for a foreground five-minute loop, or schedule the one-shot command through the host's service manager. Failed one-shot collection exits unsuccessfully for service-manager diagnostics.

The current Mac installation uses `com.otri.otrios-dashboard-monitoring`, a login LaunchAgent with `StartInterval=300` and `RunAtLoad=true`. Binaries, private config, the latest binary view, a private query cache and logs live under `~/Library/Application Support/OTriOSDashboard/`. Its SSH access uses the already configured alias and login agent; it does not provision keys or copy credentials to browser/frame. It requires the Mac to remain awake, logged in, connected and able to reach the existing server. Publishing a new code release does not stop the collector, and code reloads preserve the current presentation.

## Inspect and restore

- Trusted `GET /presentation`: exact current binary presentation, or an empty body when disabled. Public browsers use session-protected `GET /monitoring/presentation` or `/monitoring/stream`; anonymous `/demo/stream` never carries it.
- `POST /presentation`, `Content-Type: application/octet-stream`: replace the view; an empty body clears it. The same origin checks and trusted-tailnet editing boundary as `/layout` apply. This is one collector with latest-writer replacement, not optimistic collaborative publication.
- `GET /layout`: the underlying editable layout with its own ETag, even while a monitoring presentation is displayed. Its existing conditional publication API is unchanged.
- `/health`: active displayed layout ID plus a `presentation` boolean.

To restore the previous dashboard on the configured Mac, stop the collector before clearing, otherwise its next run will restore the monitoring view:

```sh
launchctl bootout gui/$(id -u)/com.otri.otrios-dashboard-monitoring
curl --fail -X POST -H 'Content-Type: application/octet-stream' \
  --data-binary '' http://127.0.0.1:8876/presentation
```

Re-enable with `launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.otri.otrios-dashboard-monitoring.plist`. The underlying saved layout, touch counter and CRDT history are retained. These commands affect only OTriOS presentation; Grafana and Prometheus keep running independently.
