<!-- SPDX-License-Identifier: Apache-2.0 -->
# Spaces map M1 qualification — 2026-11-01

RFC 0028 M1 implements the common Spaces map and consolidated settings in the
shared Rust host UI. This record currently covers the validated local candidate;
signed ARM builds, deployment, physical Frame acceptance and compatible rollback
remain pending. It is not a claim of live acceptance.

The operational contract is [Spaces](../SPACES.md). The earlier
[Spaces migration](spaces-frame-2026-11-01.md) remains the confirmed-data baseline;
this increment does not move, replace or delete app collections.

## Scope

The map filters discovery by admission, hides empty Spaces outside Edit and keeps
sign-in/profile at upper-right and Edit/Done at lower-right. Selecting a single-app
Space enters its app; larger Spaces have app previews and a paged app list.
Space names, backgrounds, preferred sizes, Space order, app order and Space-local
app labels are authority-owned metadata. Pointer and keyboard management use
expected registry generations and preserve collection IDs and binding epochs.

App settings use an editable name and immutable bundle ID, with no Rename/Move/
Delete App commands. Share App, Export Data and Replace Data remain unavailable.
M2 retained-data presentation, M3 cross-Space moves and M4 portable transfers are
separate milestones. Existing data-management controls remain available through
Space settings / Apps & Data.

The state protocol is v17, Spaces use the v2 binary envelope with numeric request
version 1, and registry framing is OTSP6. Release-control v1 is unchanged. OTSP5
upgrades preserve existing IDs, safety decisions, paired storage and confirmed
content. An OTSP5-only writer is not a compatible fallback after this upgrade.

## Local validation

- `bash scripts/check.sh` passed with Rust 1.90.0, the existing Mac Node/Python
  toolchains and `TMPDIR=/private/tmp`. This includes strict Clippy, workspace
  tests, real local server/client process checks and signed bundle tooling.
- `bash scripts/build-dashboard-web.sh` built the browser Wasm host.
- `scripts/qualify-spaces-browser.py --driver scripts/test-spaces-map-browser.mjs`
  passed the common map, empty filtering, name/color persistence, implicit app
  label, stable bundle ID, keyboard resizing, real pointer reordering, held-peer
  preservation, 1024/736/390/320-pixel bounds, scoped transport replacement,
  anonymous discovery and single-app launch checks. Collection IDs were preserved.
- The existing complete `scripts/qualify-spaces-browser.py` regression passed
  retained-data recovery, scoped ink/chat, public comments, deletion/revocation,
  reviewed publication and a verified 200,000-byte attachment download, including
  independence after source deletion. Chromium reported no runtime exceptions.

Browser testing found and fixed a handoff that could change the URL while retaining
the previous Space's socket. Switching now replaces the scoped transport and
rejects old-epoch traffic, retaining the local destination. Visual review also
found the old heading minimum width and short-window settings overlap; the map
uses its own responsive minimum and settings retain enough height to stay usable.
The probe now rejects visible host errors as well as runtime exceptions.

These checks qualify the bounded dashboard increment, not Linux shared-memory IPC.
