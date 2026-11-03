<!-- SPDX-License-Identifier: Apache-2.0 -->
# Persistent browser sign-in — 2026-11-03

The user requested that refreshes, idle time and authority restarts stop ending
browser sign-in. This replaces the RAM-only, eight-hour policy recorded in
[browser auth status](browser-auth-status-2026-11-03.md).

Browser sessions now have no server-side time/idle expiry. Their 256-bit random
opaque tokens are represented only by SHA-256 digests in a bounded, private
host-owned snapshot. Username, identity and a digest of the configured password
verifier bind each grant to its account. Explicit account/credential changes
revoke old grants; neither app state nor signed release activation owns them.

Login rotation and Sign Out atomically replace and sync the session file before
reporting success. A storage failure fences browser grants until recovery rather
than keeping an unconfirmed RAM-only decision. A separate private file lock
admits one authority owner. Existing grants are never evicted to admit a new
login; rotating this browser's token reuses its slot at the 128-session bound.

Frame uses `/var/lib/otrios-authority/state.sessions.json`, mode 600, outside app
data and releases. `--sessions-file` can override the path. The former
`--session-seconds` flag is removed. Cookie protections remain host-only, Secure,
HttpOnly and SameSite=Strict. Authenticated status checks renew a 400-day cookie;
browser retention limits or clearing site data can still require signing in.
The host itself does not expire the grant. Client document/private-view storage,
Space permissions, login rate limits and native credentials are unchanged.

## Focused checks

- Strict Clippy for all dashboard targets with `server,canvas,host-sync` passed.
- Five authority identity/session unit tests passed, including durable identity
  and revocation recovery, exclusive ownership, corrupted state, account changes,
  capacity without eviction and storage failure handling.
- `public_access` (5), `collection_session` (3) and `chat_process` (2) passed.
  Tests kill and restart actual authority processes, reuse retained cookies,
  mount protected apps, and prove that Sign Out/rotation remain revoked after
  subsequent restarts. Existing expiry-triggered revocation tests now perform
  explicit Sign Out instead.
- `node --test scripts/test-dashboard-monitoring-session.mjs`: 12 passed,
  covering delayed responses, login/logout ordering and temporary status errors.

The repository check initially exposed inherited D1 fixture failures: shipped and
composed app counts/catalog paths still excluded the designer, and the generic
Canvas mount left OIB seed state unconfirmed before testing local reflow. The fixture now mounts
and synchronizes the configured authority seed through a real session. The
69-test Canvas suite passes without skipping the designer or weakening viewport
assertions. These corrections change tests only.

The final `bash scripts/check.sh` invocation passed on macOS: formatting,
strict workspace Clippy, workspace tests, Python/Node tooling, independent
portable-data verification, all 37 real authority-process tests, and all three
signed app-bundle lifecycle tests. Linux-only display/touch suites were not run
on macOS; the ARMv7 live acceptance below exercised the deployed host binary.

## Live Frame acceptance

The ARMv7 authority was built with Rust 1.90.0 and
`--features direct,server,canvas --bin dashboard-server --target armv7-unknown-linux-gnueabihf`.
Its complete crate source snapshot was checked against the worktree before
publication. Deployed binary SHA-256:
`5ab9daf0acf87097c68ad044186262dddac2e3563c9115dd271182f60d129364`.

The established deployment receiver verified the existing signed release closure
and transfer digests before replacing the authority over pinned SSH. Authority
executables use that trusted host deployment boundary; they are not part of the
signed renderer manifest. UI/Wasm/native assets, catalog and signed app bundles
remain on release
`f62027700e7d8a05a6791866231d22a7ea230f3324079e6e853a250f0dfee8ef`.
State protocol v27 and opening release-control v1 are unchanged.

The production authority recovered, created its private session owner lock, and
reported healthy; native display and tunnel remained active. A code-only rollback
of the preceding authority was retained at
`/var/lib/otrios-authority/rollback/2026-11-03-persistent-browser-sessions-code`.
No production account, credential, publisher, service identity or app data was
changed.

The deployed ARM binary also passed a separate loopback acceptance using a
private temporary test configuration: login returned 200, its retained cookie
returned 200 after killing/restarting that authority, Sign Out returned 200, and
the same cookie returned 401 after another restart. Its session file was mode
600. The temporary process/configuration was removed; this did not add a
production account or touch production app state.

Safari needs one fresh sign-in because the preceding volatile session had
already been lost. Real-account Safari refresh/restart acceptance is pending the
user's sign-in; the isolated ARM process results above do not claim that step.
