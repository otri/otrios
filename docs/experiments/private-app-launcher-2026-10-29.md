<!-- SPDX-License-Identifier: Apache-2.0 -->
# Private app launcher indicators — 2026-10-29

Follow-up to [standalone app installation](app-bundle-frame-2026-10-29.md).
Source `1ffda1a2b26c2b8408353f9045a9e67187499011`; signed candidate
`19feedfb994aebde973ebe08cbc37738af59414943dc8d6444e1c4ca7f2a2b82`.

Home's generated public app cards omit the generic "Installed app" subtitle.
Private cards retain their app icon and add a primitive padlock badge: closed
with "Sign-in required" or open with "Access available". Both graphics hosts
use the shared Rust scene. The browser follows validated cookie-session status;
Frame's trusted supervisor declares its access locally. Disconnects relock Frame's
indicator. These values never become shared state or authority grants, and trusted
native access does not enable browser cookie sign-out controls.

UI format v7 explicitly admits the field-0 `SessionAccess` projection; older UI
formats reject it. Bundle required features advertise that UI version. State
protocol v8 and the stable opening release-control v1 envelope are unchanged.
The supervisor supplies access through an environment variable so an older native
release can safely ignore it during operational rollback.

The RFC reserves `/Applications` and `/PrivateApplications` as the proposed system
namespace, with Monitoring in the private class. This change documents that design;
it does **not** migrate the current `authority/bundles` store, create Linux root
folders, or implement a filesystem discovery watcher. Folder placement must agree
with signed policy in a future migration; moving a folder cannot grant access.

## Checks

- `bash scripts/check.sh` passed, including 63 Canvas tests and the complete signed
  app lifecycle process test. The focused access test exercises locked → unlocked
  → locked on public Home, UI-version rejection and separation from cookie actions.
- All-feature, all-target Clippy passed for core/dashboard/web-host on macOS and
  Linux/ARM. The frozen signed build repeated process tests, browser compilation
  and 74 Node tests and built matching ARM binaries. An additional focused Node
  session/worker run passed.
- `scripts/test-launcher-browser.mjs --test-login` passed against a real temporary
  local authority with a test-only email/password account and temporary-profile
  Chrome. It navigated to Monitoring, submitted the actual login form, returned
  Home with an open padlock, then used the real Sign out control and confirmed
  Home relocked. No runtime exceptions. An initial test fixture used a non-email
  username and was blocked by the existing form's email validation; the corrected
  fixture passed. No production credentials were used or changed.

[Browser result](data/private-app-launcher-2026-10-29/probe.json),
[locked](data/private-app-launcher-2026-10-29/home-locked.png),
[unlocked](data/private-app-launcher-2026-10-29/home-unlocked.png), and
[relocked](data/private-app-launcher-2026-10-29/home-relocked.png) captures retain
only launcher metadata and test-session status, without passwords or cookies.

## Live Frame deployment

The tested source was pushed and its exact SHA fetched from the configured remote
before activation. The full signed transfer completed; the authority, native
last-good pointer and current native ready record all agree on the candidate
release above. Authority, direct display and tunnel services are active.
The native host presents Home with Monitoring's open padlock and "Access available"
under its existing trusted session. A fresh public Chrome session presents the
closed padlock and "Sign-in required". No production login credentials changed.

The [native record](data/private-app-launcher-2026-10-29/native.json) confirms
12 admitted apps, `gles2-direct`, confirmed state, ready stream, existing rotation 90
and 800×1280 scanout. The accepted touch calibration hash is unchanged.
The [actual Frame scanout](data/private-app-launcher-2026-10-29/frame-scanout.png)
retains the physical hardware rotation. The independently installed Counter
remains in Home and opens successfully in a separate read-only browser probe;
its [sample](data/private-app-launcher-2026-10-29/counter.json) reports value 2.
This task sent no Counter edits and did not restore or copy authority state.

Public conformance passed **171 checks** on the stable live release, including
private/admin isolation and anonymous denial. The new
[public launcher probe](data/private-app-launcher-2026-10-29/live-browser.json)
also passed with no runtime exceptions. Existing Linux drivers and calibration
were preserved. The builder remains stopped as found and is pinned to the
completed follow-up source for its next run.
