<!-- SPDX-License-Identifier: Apache-2.0 -->
# Browser authentication status follow-up — 2026-11-03

The user reported appearing signed out when returning to Safari, including while
the site seemed unchanged, then reported being signed in again before this fix
was deployed. The idle Safari reload was not reproduced. Production inspection
showed no unexpected authority restart or release change during the reported
interval. The authority last started at 2026-11-03 06:34:47 UTC for the explicit
D1 recovery test, and its automatic restart count was zero.

One concrete error path was corrected: `MonitoringSession.refresh()` previously
treated every non-success HTTP response as an authoritative sign-out. It now
preserves its last confirmed status on temporary server/proxy or rate-limit
errors and rejects the check as unavailable. A 401 still revokes status
immediately. Anonymous/unknown status does not gain permission; the authority
continues checking every protected request independently. Login, logout,
membership, cookie flags and the eight-hour expiry are unchanged. Server sessions
remain RAM-only, so authority restarts still invalidate them.

## Checks and deployment

`node --test scripts/test-dashboard-monitoring-session.mjs` passed all 12 tests,
including login/logout ordering, delayed replies, expiry, anonymous failure and
HTTP 429/500/502/503/504 failures after a confirmed login. `git diff --check`
passed.

Signed release
`f62027700e7d8a05a6791866231d22a7ea230f3324079e6e853a250f0dfee8ef`
was built from the verified D1 release, changing only
`web/monitoring-session.mjs` and its deterministic gzip sidecar. Native, Wasm,
authority, app catalog and signed app bundles were reused unchanged. There was
no Rust rebuild. Both outer signature/closure and inner app signatures were
verified with the existing publisher identity before deploying to the existing
Frame/Web environment.

The actual published module's SHA-256 is
`dcdac4ac08b9dd5df1474bbc970dde905e85319c99db3914c9079707e7ac10a0`.
A live Chromium preview fetched and verified those bytes, then exercised the
published module with controlled status responses: 200 establishes status, 503
preserves it and rejects the check, and 401 revokes it. The page loaded the new
release and returned to the live Spaces view. This is deployed-module/browser
acceptance using a controlled fetch adapter, not a real-account Safari sign-in
or proof that the reported idle behavior is resolved.

Post-deployment inspection confirmed the authority start timestamp and restart
count were unchanged, preserving its existing in-memory session table during
this rollout. The previous complete D1 release remains the compatible code
rollback. No application state, credentials, identities or services were changed.
