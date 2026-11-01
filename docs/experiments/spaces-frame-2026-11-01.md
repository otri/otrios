<!-- SPDX-License-Identifier: Apache-2.0 -->
# Spaces Frame rollout — 2026-11-01 JST

RFC 0027 is deployed on the existing Frame and public web environment. The
[acceptance record](data/spaces-frame-2026-11-01.json) contains sanitized artifact
hashes, migration checks, physical readiness, browser samples and rollback results.
The source starts at `196b7bb` on `feat/spaces-and-app-data`; this follow-up adds
qualification portability fixes, the collector unit argument and the native
updater correction described below. No Issue or PR was opened.

## Accepted configuration

| Space | Audience | Apps | Unattended Frame |
| --- | --- | --- | --- |
| Household | Closed | Ten existing shared/demo roots | Member |
| Brass Relay | Public view and signed-in visitor comments | Brass Relay | Authenticated visitor |
| Operations | Closed | Monitoring | No membership |

The named human is explicitly Admin of all three Spaces. The existing password
verifier was preserved byte-for-byte. Frame and Monitoring collector use separate
private token files; the collector has only its Operations collection grant.
The temporary human acceptance token and disposable browser profile were removed.
The publisher private key remained on the Mac.

SSH used the Mac's existing Ed25519 host pin with `StrictHostKeyChecking=yes`:
`SHA256:MkTehL53moSAWs/omMMpaXhS9emXjZiljHT+lmpeBBs`. The existing publisher public
PEM has SHA-256 `eab886983e0e1df4694b5682ca6da2495520c11e4a294bba78baedbeb0805285`.
No newly scanned host key or disposable signing key established live trust.

| Artifact | SHA-256 / identifier |
| --- | --- |
| Final complete signed release | `4dac08d73a93aca3345cb49d9a35631a0c60e4f125b6270054c297037c438add` |
| Qualified compatible code rollback | `f53e3bfadd4a3bf7ad7bd576ca48c90068f25e49910e5615d9fbc3a2ccb30904` |
| Installed ARM authority | `d0431eb105b7250dddfcc303c56e770b4cdfffd6d56695a31720303285f54a56` |
| Final ARM direct host | `3705e3933b0e151d8989c62fc6f41339768573b2a1711447af3fc6ae53e149de` |
| Fixed native supervisor | `97d7ca5495336c221077421ec150ad0472149a6d8a6916a32621b49aa5341d4e` |

Opening release control remains v1, state is `otrios-canvas-sync-v16`, and the
registry uses OTSP5. Both releases support current admission, OTCHAT02 and the
configured device-local namespace. The final native binary was rebuilt from the
final fixture changes and signed separately; its authority and browser bytes
remain identical to the initially qualified release.

## Confirmed-data migration and backups

Stopped the collector, display and authority before each consistent source
snapshot. The preflight and fresh cutover snapshots each retained 33 state files.
The version-2 provisioning plan explicitly imports all 12 installed document
roots, complete writer receipts, the full Brass Relay journal and Monitoring's
complete presentation into their selected collections. Nothing inferred a grant
from the old login or trusted loopback listener.

A private copy on the physical Frame passed provisioning, two authority starts,
public/closed discovery filtering and collector-versus-human upload checks.
Repeating provisioning preserved its completion record. Every recovered document
snapshot matched exactly before/after import, including confirmed receipts. All
48 chat messages matched the complete canonical-message digest:
`9f18c6fa23acffcc8c15f194316c9d5813ab776cefd6cbead76b1a0dbc3fcf5b`.
The stopped live migration repeated that exact comparison before reopening state.

Private Mac backups reside under
`~/Library/Application Support/OTriOSDashboard/rollback/spaces-2026-11-01/`:

- `preflight-state.tar` and `cutover-state.tar`: consistent original payloads,
  with every archived state-file digest verified.
- `managed-state.tar`: the immutable paired managed snapshot and safety metadata;
  23 file digests verified after transfer.
- `qualified-code/`: both verified complete releases, matching authority binaries,
  publisher public key and current helpers, including the socket correction.

The managed snapshot is
`000001a1950f0c0c-2d013d39582cb26c3ec7f536b838475c`. It was created using the
[documented offline storage command](../SPACES.md#managed-snapshots-and-offline-restore)
as the authority user. The Frame also retains private preflight/cutover and
rehearsal copies under its rollback/rollout directories. These operator-controlled
copies and Mac archives have separate retention from the managed seven-day store.
Eleven unattributed historical files remain preserved. `finalize-legacy` was not
run; no original cleanup or universal-erasure claim is made.

## Physical and live-client acceptance

The actual native service reported `gles2-direct`, RGB565 5/6/5/0, 800×1280
scanout, rotation 90, one calibrated touch device, confirmed state, an active
stream, idle input and zero pending commands. Authority, native runtime, UI and
last-good pointers agree on the final signed release. The authority, native,
tunnel and collector timer are active; the collector reports successful execution.

Calibrated synthetic evdev taps navigated Household into Counter and confirmed
2 → 3 on the initial executable, then 3 → 4 on the final executable. An independent
live HTTPS browser observed the same confirmed values. Native uninstall revoked
execution and returned to managed Home; a restart while removed retained that
restriction. Reinstall reopened the same collection. The other browser stayed at
4. The final namespace is generation 3 with no removed roots.

Removing Frame membership cleared its active Household activation and hid that
closed Space. Restoring it admitted a Member again; an attempted Member-to-Admin
change received 403. Collection identities remained unchanged. The current
registry generation is 19. Operations remained undiscoverable to Frame.

Anonymous live Chromium read all 48 public messages without a composer, while its
Space metadata exposed only Brass Relay. The clients followed final upgrade,
rollback and reactivation, retained Counter 4, and reported no runtime exceptions.
The existing touch-rule SHA-256 remained
`44f2b33a3a17140e6c662b9feba1f009c9ed7910f2c1fa2178b7f5a24a53859e`.

Physical acceptance is automated display/input-path evidence, without claiming
human touch or independent visual inspection. Final GLES pixel capture remains
private on Frame at
`/var/lib/otrios-authority/rollout/spaces-2026-11-01/physical-final-home.ppm`:
3,072,016 bytes, SHA-256
`8e03b3188b7befa23351aa239cebd7f3b358cc43aff2bd59f374074d2869b290`.
Pixels were not exported to the workspace.

## Signed rollback and preservation

Verified the complete fallback signature/closure before advancing the pointer and
notifying activation, then observed physical native last-good agreement. Returned
to the final release through the same checks. No state backup or older registry
was restored. Registry, latest-decision ledger, stable owner lock and paired-root
markers remained byte-identical across both switches. Keep those current safety
files when operating the code kit; pre-Spaces binaries are not a supported fallback.

A complete-root comparison initially detected a Counter metadata difference.
Comparing the immutable pre-rollback snapshot with recovered live state resolved
it: all 12 field and signal subtrees and all **262 existing writer receipt records**
were preserved. Two reconnecting editable mounts added unused sequence-zero,
zero-digest writer registrations. Those registrations legitimately change the
Merkle root; they are not replayed edits. The full chat digest was unchanged.
Since migration, only Counter changed, through the two acceptance edits above.

## Validation and rollout fixes

- `bash scripts/check.sh` passed with Rust 1.90.0, the existing Node 24 executable
  on PATH and `TMPDIR=/private/tmp`. This includes strict Clippy, workspace tests,
  22 real authority/client process cases, JavaScript checks and signed bundle
  checks. Log SHA-256:
  `80e7ae37f908b34af8f79b829b3346bea41601c42ebb381c9f7d2ce4dea58a34`.
- Rebuilt disposable `scripts/qualify-spaces-browser.py` passed without runtime
  exceptions, including real pointer/file input, visitor comments, retained-data
  recovery, selected publication, verified 200,000-byte attachment download and
  source-deletion independence. The JSON record retains its exact artifact hashes.
- `python3 scripts/test-dashboard-host.py --url https://otrios.otri.ca --public-only`
  passed 171 live HTTPS checks. Live password login was not tested; disposable
  normal-form login was tested and the installed human verifier was preserved.

Mac qualification exposed timestamp collisions in parallel fixtures and an
unresolved `/var` versus `/private/var` assertion. Fixtures now include a process
counter, and the assertion compares canonical paths. A short temporary root keeps
Unix socket fixtures within macOS's path limit.

Mac headless Chrome left isolated documents hidden even after `bringToFront`.
The driver now uses supported focus emulation for each test window. Public comment
revocation also allows a fresh read-only mount to repaint before polling observes
an empty transcript; the probe waits for revoked write controls, retains its
transport revocation check, and verifies the public transcript after reload.
No Worker or DOM prototypes were replaced.

Physical upgrade exposed cache accounting treating the device namespace's Unix
notification socket as a directory. The supervisor kept the previous signed app
running. The fix counts that socket as zero retained artifact bytes, preserves it
during managed eviction, and rejects other unexpected special entries. All 35
release/cache tests and the full required suite pass; a real socket regression
checks notification delivery after eviction. The installed authority unit also
explicitly selects its provisioned collector binding.

These results qualify the bounded Spaces/dashboard experiment. They do not
establish the separate Linux shared-memory IPC PoC or general OS capabilities.
