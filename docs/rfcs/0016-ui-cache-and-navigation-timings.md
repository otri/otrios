# Public UI reuse and local navigation timings

Status: implemented; see [qualification](../experiments/ui-cache-navigation-timings.md).

The shared Canvas client retains fully qualified public UI documents in RAM by
SHA-256. At most eight documents and 2 MiB of canonical bytes survive between
mounts; least recently mounted documents leave first. Private UI and app State
are never retained in this cache. The existing behavior cache remains separate.

A sync-v5 Mount adds `known_ui` after the catalog digest: one byte, 0 for absent
or 1 followed by a 32-byte digest. Other flags, truncated hashes and invalid
lengths fail closed. The client advertises only the current admitted target's
cached public digest. The authority checks the catalog, State binding and access
first, and omits UI bytes only for that exact public digest. It always sends the
UI digest and a fresh State session. A wrong hint receives the full document.
The client rehashes and requalifies cached bytes, behavior, State, selectors and
scene before accepting the candidate. A missing/corrupt cache entry rejects the
candidate and preserves the previous scene. Updated UI hashes miss naturally;
old public revisions remain bounded, allowing a rejected update to retain its
previous view. Only successfully qualified mounts populate the cache.

All packed envelopes use `OTC5` and negotiate `otrios-canvas-sync-v5` consistently
across browser, native, authority and signed manifest. The release-control v1
hello remains unchanged: an incompatible client can discover a newer release,
while incompatible State bytes are rejected before decoding. This requires a
complete runtime deployment, rather than a UI-only update.

Browser diagnostics are opt-in at `/?timings=1`; the query survives navigation.
The panel includes a selectable JSON report and Copy button, useful on an iPad
without developer tools. The diagnostic module loads only when requested. Timings include the opt-in panel and module overhead. No
telemetry, storage, extra sockets, input contents, credentials, State values,
URLs or queries are recorded. Reports retain thirty completed mounts; unfinished
interactions are bounded to thirty-two. Normal edits and drags do not count as
navigation. Private app payloads never enter reports.

Startup begins at the browser's navigation time origin. Navigation begins at
pointer release (or accessibility activation/history navigation), with
pointer-down-to-completion reported separately when available. Event timestamps
also expose input handler delay. Main and worker clocks use `timeOrigin + now`.
The shared core exposes an unsent mount epoch, avoiding a second JavaScript
wire parser or app-specific navigation rules. Only a matching mounted epoch
with installed UI/controls and geometry can complete a timing. Completion is
GL submission followed by the next animation frame, not measured photons or
completion of decorative animation.

Reported phases distinguish main input delay/queue, delivery to the worker,
worker dispatch, send handoff, response wait (network plus authority), response
delivery, qualification, geometry construction, frame delivery, rendering (UI installation, waiting for the draw, and GL
submission), and the following animation frame. Startup also records Wasm compilation, font,
catalog and presenter milestones. `responseBytes` is the actual encoded state
packet that qualified the mount, excluding WebSocket/channel headers. Reconnects
or release-triggered remounts are not reported as new user taps.
