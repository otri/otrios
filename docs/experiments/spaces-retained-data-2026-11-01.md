<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0028 M2: retained data and recoverable Spaces

Deployed runtime source `6b71fd0e9bf0eeb493e15afdf8d0331298b15455` as signed
release `a77bffea8c6f1e09aa8b27d120b549db1c12bab36a84504b8026e00e27032f00`.
Web and native agree on state protocol v18; the opening control envelope stays v1.
The management envelope is v3, with numeric request version 1 unchanged.

Edit now has separate red app-minus controls. Delete App defaults to retaining
data; Cancel resets Remove Data. Exact retained datasets appear as ghosted tiles
after loaded apps, and have reviewed restore/delete controls. Restoring a sealed
backup preserves both source and outgoing data and activates a fresh collection.
Deleting a Space retains a recoverable, authorization-checked record; checked
Remove Data tombstones its datasets. Successful management stays quiet.
Cross-Space moves and portable transfers remain M3/M4 work.

`bash scripts/check.sh` passed: strict workspace Clippy, 219 core library cases,
34 focused host-UI cases, 24 real authority/client process cases and the required
Python/Node/tooling checks. The old deletion-dialog tests were replaced; silent
success is an assertion in the removal path. New process checks preserve confirmed
state across detach/restore, Space deletion/recovery and sealed-backup restoration.
Process-exit tests recover scoped restores before app admission. These are UI/data
acceptance results, not Linux shared-memory IPC acceptance.

The canonical `watch-dashboard.py --once --stage-only` build passed, including
signed-closure verification, Web compilation, 80 Node cases and ARM compilation.
The focused disposable Chromium flow passed against its exact Wasm digest
`052a9eb3c42c54d47fa105e4442cb59d40b37994449f6f0f12dda400b3fece3a`.
It covers keep/delete data, Cancel, same-identity reattachment, deleted-Space
recovery and silent success. Existing map checks use two scroll reversals and
1280/390 widths. Browser executable selection now works directly on macOS, and
cleanup does not hide a launch failure. Production bytes were checked against
the frozen candidate; later changes were acceptance tooling only.

`backup-frame-release.py --tag 2026-11-01-spaces-retained-data` preserved a
quiescent Frame kit and verified 267 files in the private Mac archive. The pinned
SSH deployment applied once. The on-Frame migration audit found identical Space
metadata and 55 byte-identical non-collector files, with auth, native identity,
calibration and collector binding preserved. Inventory details stayed on Frame.
Readiness reports the signed release on gles2-direct/RGB565, one calibrated touch
reader and a successful collector. Public conformance passed 171 checks.

`test-frame-home.py --retained-review --skip-navigation --runtime-probe ...`
passed on the physical Frame: calibrated evdev contacts opened the minus dialog,
toggled Remove Data and cancelled, with unchanged registry generation, no app
writes and no pixel export. This is physical-device software acceptance; human
finger feel and visual observation were not claimed. Existing live app data was
not deleted for this check.

Persistence upgrades OTSP6 to OTSP7 without rebinding confirmed data. A private
copy test confirms the old OTSP6 binary rejects OTSP7 without changing control
files; no live code rollback was performed. Recovery/rollback must use OTSP7-aware
code and current safety roots. This release is the initial compatible baseline
for the remaining milestones. The pre-cutover registry is never a supported way
to undo current decisions.

[Sanitized evidence](data/spaces-retained-data-2026-11-01/README.md) contains only
fixture images, aggregate results and technical artifact identifiers.
