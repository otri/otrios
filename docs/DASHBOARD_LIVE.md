# Live dashboard development

The development service runs on a Mac and serves the Rust/Wasm browser dashboard plus a persistent Automerge room. The Rockchip frame runs a native ARMv7 GLES2 executable. Its current [direct KMS profile](DIRECT_DISPLAY.md) bypasses Weston; the existing Weston service remains recovery. Both use the same Rust state projector and compositor. The frame runs no dashboard browser.

## Network and authority

Start the server with absolute paths outside the source checkout:

```sh
dashboard-server --bind 127.0.0.1:8876 --state /ABS/RUNTIME/state.automerge --releases /ABS/RUNTIME/releases --web-dir /ABS/RUNTIME/development-web --origin https://HOST.TAILNET.ts.net
```

Expose only that listener with the official Tailscale CLI:

```sh
tailscale serve --bg --https=443 http://127.0.0.1:8876
tailscale serve status
```

Open `https://HOST.TAILNET.ts.net/` on an iPad connected to the same tailnet. Serve is private tailnet HTTPS, not public Funnel. Existing tailnet grants determine who can reach it; this app has no separate users, per-object permissions or read-only role. The signing key is never served. The frame uses `/stream` over WSS with certificate verification, not an SSH reverse tunnel.

`GET /health` identifies the current snapshot and code release. `/release` returns the current immutable entry. `/releases/ID/...` serves only the artifact whitelist. WebSocket `/stream` carries binary Automerge sync messages and JSON release notices. The optional `/sync` POST exchanges complete documents for baseline diagnostics. Browser cross-origin POST/upgrade requests are rejected. Connections/messages are bounded; slow state notifications coalesce rather than queue every revision.

The central file contains accepted history and is flushed/replaced before acknowledging changes. Each device keeps its own replica, so offline edits converge after reconnect. “Truth” here is the persisted merged history; screens need not switch at the exact same instant. There is one trusted development room, no consensus or multi-tenant security claim. Back up its state file when stopped (or an atomic captured copy), and retain the publisher key securely.

## Code publication and state preservation

`publish-dashboard.py` copies a complete web build and ARMv7 executable into an immutable directory named by the SHA-256 of its canonical manifest. An Ed25519 signature binds artifact hashes/sizes, state schema and sync protocol. The pointer changes only after all artifacts and signature exist. Old releases remain available.

`watch-dashboard.py` freezes source inputs, runs core tests, compiles Wasm with wasm-bindgen 0.2.129, syntax-checks the browser host, and compiles ARMv7 in an isolated Docker cross-builder. A failed or stale build retains the active release. It uses source-copy Docker input, preserving unrelated containers and avoiding host mount permissions. Create the builder image with `scripts/build-dashboard-armv7.sh` first.

A watcher JSON config supplies absolute `repo`, `runtime`, `openssl`, optional `node`, and `build_env` with `RUSTUP_HOME`, `CARGO_HOME`, and `PATH`. Example: `tools/dashboard/live-config.example.json`. Use OpenSSL 3 with Ed25519 support. Keep the private key at `RUNTIME/publisher/private.pem` with mode 0600 and a private runtime directory. Generate it once, export only its public key, and pin that key on the frame:

```sh
umask 077
openssl genpkey -algorithm ED25519 -out /ABS/RUNTIME/publisher/private.pem
openssl pkey -in /ABS/RUNTIME/publisher/private.pem -pubout -out /ABS/RUNTIME/publisher/public.pem
python3 scripts/watch-dashboard.py --config /ABS/RUNTIME/config.json
```

The Mac deployment uses two login LaunchAgents, `com.otri.otrios-dashboard-server` and `com.otri.otrios-dashboard-watch`. Runtime, logs, release store, build caches, source inbox and publisher key live under `~/Library/Application Support/OTriOSDashboard`. The compiler/tooling used by the background builder lives outside Documents so no additional macOS Documents privacy permission is required.

For immediate local edits, run this in an authorized checkout session:

```sh
python3 scripts/queue-dashboard-source.py --runtime "$HOME/Library/Application Support/OTriOSDashboard" --watch
```

This exports selected public build inputs to the builder inbox and holds a feed lock. It never exports `.git`, credentials, `.env` or arbitrary user files. If configured with `git_remote` and `git_branch`, the watcher refreshes that public branch every ten seconds when no local feed holds the lock. This gives pushed collaborative changes the same build path after the local chat closes. Only configure a trusted branch: the builder runs its tests/build scripts and signs successful output. A reboot/login restarts the installed services; Docker must be available for ARM builds. A failed refresh/build keeps the existing dashboard working.

Code reload in this first version means a browser page reload and a brief native process restart. It is not replacement of a live function inside an existing graphics process. The browser retains its CRDT in session storage; if saving fails, automatic code reload is paused. The frame checkpoints its CRDT before replacement and loads the same document in the new process. Releases require the current v1 state/sync contract; incompatible schema migrations are deferred. Both then merge any changes received during the reload. They need not activate simultaneously.

The native supervisor downloads into its private cache, verifies the pinned publisher signature and binary before execution, and requires a rendered GLES2 frame plus a live CRDT stream before accepting a new release. It retains the previous verified executable and returns to it if a candidate fails readiness. Cached accepted code may start offline. A signature is not a sandbox: the pinned publisher is trusted to supply executable native code. Browser code relies on the trusted HTTPS origin; it does not independently verify the detached signature. Revocation, rollback prevention, release garbage collection and signed arbitrary runtime modules remain future work.

## Native frame service and recovery

`tools/dashboard/otrios-dashboard.service` is a template for the existing SPD frame. It runs as `kiosk`, requires Weston, preserves its calibrated input/display configuration, stops `spd-browser` while active, and starts the old browser after dashboard failure. A successful service restart does not start a conflicting browser job; a deliberate manual stop uses the recovery commands below. Install:

- `scripts/frame-dashboard-agent.py` as `/opt/otrios-dashboard/frame-dashboard-agent.py`.
- The publisher public key as `/opt/otrios-dashboard/publisher.pem`, root-owned.
- Qualified SDL2/libXss runtime libraries and their notices under `/opt/otrios-dashboard/lib` and `licenses`.
- `/etc/otrios-dashboard.conf` containing `DASHBOARD_URL=https://HOST.TAILNET.ts.net`.
- The service template under `/etc/systemd/system/otrios-dashboard.service`.

After verifying dependency closure and the unit, reload systemd and start it. Enable at boot only for an intentional persistent native deployment. The state/release cache is `/var/lib/otrios-dashboard`, private to `kiosk`; no kernel, bootloader, touch calibration, SSH policy or old browser profile needs editing.

For manual return to the old kiosk:

```sh
sudo systemctl disable --now otrios-dashboard.service
sudo systemctl enable --now spd-browser.service
```

For the Mac, stop the two LaunchAgents to stop serving/building. `tailscale serve --https=443 off` removes this HTTPS proxy; check status first if other services have since been added. Keep state, key and releases for recovery. The Mac must remain awake, online and logged in for this deployment to be reachable. This is not an always-on cloud host.

## Verification limits

Run `bash scripts/check.sh`, then all-feature Clippy where SDL2 is installed. The checks exercise incremental offline merge/reconnect, real HTTP/WS processes, durable restart, origin rejection, release notification and old-asset retention, and actual Ed25519 verification/tamper rejection. Linux CI builds both hosts and presents a GLES2 frame under Mesa/Xvfb.

A matching `/health` snapshot, browser `data-snapshot`, and native ready JSON demonstrates convergence. A changed release ID on both hosts with the same snapshot demonstrates state-preserving code reload. Framebuffer readback establishes GPU output, not physical panel colour or touch alignment. Someone at the frame still needs to accept those physical properties. iPad access must be checked on the actual iPad; desktop browser validation alone does not establish it.

For the SDL-free frame profile, private builder config `native_host: "direct"` builds both `dashboard-direct` and signed `dashboard-sdl` recovery assets. The supervisor checks both hashes, chooses the recovery executable for Wayland, and checks direct framebuffer colour/stride metadata before accepting a new direct release. Platform `.c` and `.h` files participate in source fingerprints. See [direct display](DIRECT_DISPLAY.md).

The [public module host](PUBLIC_ACCESS.md) gates all payloads server-side and uses a separately verified read-only code store. `module_volume` enables module delivery before state-server code announcements; browser release readiness also tolerates lagging replicas.
