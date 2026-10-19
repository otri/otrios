<!-- SPDX-License-Identifier: Apache-2.0 -->
# Canvas foundation retirement and acceptance — 2026-10-20

The bounded dashboard foundation in RFC 0010 is implemented, qualified and live.
This closes that milestone; it does not complete the separately proposed Linux
shared-memory OS, federation, general collection queries or sequence/text CRDTs.
The branch remains `feat/canvas-replication-pipeline` and PR #6 remains unmerged.

## Ownership and retirement

There is one mutable Canvas document/intent owner per active app, one authority
with server policy and durable receipts, and one negotiated session per host.
The renderer consumes immutable selected views; clocks, focus/caret, gesture
feedback, five-second trails and GPU/font resources are presentation state.
Pending edits are RAM-only, with one pending map and one immutable issued batch.

The global dashboard Replica/SyncSession, duplicate Presentation intent overlay,
ambient bindings, old Snapshot/signal/controls paths, default layout binary,
Motion Stress and trusted `.automerge` importer are deleted. Automerge and its
unused transitive packages leave Cargo.lock (14 packages total). Host-specific
screen/action routing and the layout-seed CLI are gone. Monitoring owns its local
view templates. All seven package/catalog bytes are identical to the preceding
release: catalog SHA-256
`3944cecef69c77868c4f2a85a269edd744d42ffd149cf6061d2463b26c113dbd`.

The publisher and supervisor admit one direct native executable. SDL selection,
recovery artifacts, old service/trial tooling and the unused ARM builder package
are removed. The cleaned pinned ARM builder image builds successfully; the signed
native ELF needs DRM, GBM, EGL/GLES, libc/math and libgcc, with no SDL dependency.
No shared system graphics libraries or unrelated kiosk services are uninstalled.
The inactive old OTriOS service is removed from the frame only after the new
supervisor passes readiness; private rollback copies of both service generations
are retained. The current direct unit remains enabled for boot.

Intentionally retained: generic scene/responder/keyboard/pins-and-struts helpers;
the bounded static layout-v1 reader for live Monitoring aggregates; the stable
control-v1 metadata alias; explicit retired-route tombstones; negative obsolete
state-file checks and deletion of old browser DB names; friendly entry aliases
that normalize to app IDs; dated benchmark sources; unrelated OS scaffold work.
These introduce no second mutable app or transport owner.

A malformed FlexBuffers bool span exposed a panic in the dependency's reader.
The shared safe structural guard now checks offsets, spans, widths, map keys,
vector tables, terminators and visit/depth budgets before reader traversal.
Schema/type/policy validation remains separate. The exact bad bool fixture,
truncations and 4,096 seeded mutations pass without unchecked reader panics.
No unsafe code, dependency fork or new allocation framework is introduced.

## Matched frame replication trials

Frozen reference: `157ca5b00fa14c28ecb95f4895a1d061cee8b99d` (Rust inputs identical
to the preceding live source `67d2d58`). Candidate:
`42157f5f08753bbf04354283421e2cabbe6c934f`; deployment source `5000476` changes
only docs and the isolated GPU-trial runner after that frozen Rust build.
[Metadata](data/runtime-retirement-2026-10-20/direct-metadata.json) identifies
archives, binaries, catalog and separate signed releases. Dirty candidate status
records two prose files that were subsequently committed; it does not conceal
uncommitted runtime changes.

Three alternating repetitions × two input rates × two binaries = 12 fresh-store
trials. Two real clients run on the RK3126 frame, with 20 settled warm-ups and 120
open-loop absolute assignments plus final release. The live Home renderer remains
running. No compilation runs during measurement; clocks, thermals, network and
background work are unpinned. Temporary Tailscale routes are restored exactly and
frame experiment files removed. CPU is the paired client process, including
mount/warm-up/waits, excluding Mac authority, relay, SSH and GPU.

Medians of three run-level results, including median per-run p95:

| Input Hz | Build | Distinct / 120 | Delivered age p95 ms | Final ACK ms | CPU % of one core | Total WebSocket B | Peak RSS KiB |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 10 | reference | 120 | 98.433 | 56.825 | 18.226 | 353,880 | 7,988 |
| 10 | candidate | 120 | 99.900 | 29.735 | 18.891 | 353,973 | 7,988 |
| 120 | reference | 26 | 104.106 | 41.760 | 28.789 | 76,674 | 7,980 |
| 120 | candidate | 26 | 104.172 | 43.283 | 29.907 | 76,581 | 7,988 |

All 12 trials durably confirm and converge to 120. High-rate intermediate samples
are deliberately consolidated; they are not zero-latency deliveries. Final ACK
is one value/trial, not a percentile. Traffic and delivered age are essentially
unchanged. CPU medians rise 0.666 and 1.118 percentage points (about 3.7–3.9%
relative). Paired CPU differences are mixed at 120 Hz and all positive at 10 Hz;
user/system breakdown varies between repetitions. Source inspection identifies
an additional structural traversal before Canvas/Layout Flex decoding, including
mount and typed UI processing, as an added work source. This matrix does not
isolate its causal share. Retain the bounds check that prevents a demonstrated
panic; do not label this cleanup a CPU optimization or remove validation to win
a noisy timing. Future profiling should separate admission from steady updates.

The Mac authority shrinks from 8,012,496 to 5,708,496 bytes (28.75%). Signed native
code grows from 6,305,856 to 6,319,012 bytes (+0.21%); Wasm grows from 2,680,584 to
2,709,544 (+1.08%). Old native/browser dead code was already stripped and the new
guard adds code. Fewer dependencies do not imply every artifact is smaller.
Raw [trials](data/runtime-retirement-2026-10-20/direct-results.jsonl) and computed
[summary](data/runtime-retirement-2026-10-20/direct-summary.json) preserve the costs.

## Direct presentation isolation

The exact signed candidate native executable runs alone, with the normal direct
service stopped for each trial and restored in `finally`. RGB565, 800×1280 scanout,
rotation 90 and calibrated input are reported. A two-second warm-up precedes six
seconds of kernel page-flip completion measurement. No private view or document
data is copied into this isolated probe.

| Scene | Kernel flip FPS | Compose/submit p95 | Missed vblanks |
| --- | ---: | ---: | ---: |
| 12 tetrahedra, idle | 55.48 | 2.491 ms | 7 |
| 64 tetrahedra, requested 30 Hz state load, 250 ms worker stalls | 55.15 | 3.341 ms | 9 |

The loaded run performs 189 isolated Merkle updates while presentation continues.
This tests worker separation, not network latency or perfect vsync. The median
flip interval is 17.653 ms; occasional missed vblanks remain. Startup-inclusive
host cadence is ~49 FPS and must not replace the warm kernel measurement. This
is not locked 60 FPS or a speed comparison with the preceding renderer.
[GPU results](data/runtime-retirement-2026-10-20/gpu-results.json) and
[metadata](data/runtime-retirement-2026-10-20/gpu-metadata.json) identify the method
and exact signed binary.

## Acceptance and live deployment

- Required `scripts/check.sh`, strict workspace/all-target/all-feature Clippy,
  six native-worker tests and 40 signed-build Node checks pass. Linux CI run 149
  passes for deployed source `5000476`. Tests cover coherent subtree/policy
  isolation, delayed/final dispatch, real process boundaries, durable failure and
  recovery, reconnect/upgrade, malformed input and resource overload.
  [Check record](data/runtime-retirement-2026-10-20/checks.json) includes the
  successful Linux CI run and reproducible command scope.
- Staged browser mounts all seven apps. Playground slider clamps after dragging
  outside; label agrees, strokes stay separate with top-left coordinates and fade
  after five seconds. Input Lab taps at the start of `abc`, inserts `x` there and
  retains confirmed `xabc` after reload while local focus drops. Layout Lab's
  Toggle/width controls respond. Animation advances under local state load;
  3D tetrahedra rotate on their local presentation clock.
- Monitoring anonymous navigation requires sign-in. The private staged mount
  renders the existing static layout-v1 aggregate after sign-in; sign-out revokes
  access and returns Home. Private values/screenshots and credentials are not
  committed. Both staged and deployed public conformance pass 81 checks,
  including exact Origin, cookie revocation, denied private assets/admin and
  obsolete stream paths, public signed code and compatible hello.
- Live release:
  `c549b460eee88add24a97436221150d06477f057bdc512d60d54b40a022eae42`.
  The authority retains all seven canonical durable app roots and the existing
  Monitoring aggregate. A stopped-state rollback snapshot precedes replacement;
  rollback restores code/config, never overwrites newer confirmed writes.
  Source availability is preflighted before stopping services. The subsequent
  builder-only cleanup pin is `5b684c5`; its watched runtime-input fingerprint is
  unchanged, so tested signed code is not rebuilt or replaced. The temporary
  staged authority and idle build container are stopped; production remains
  one authority. See
  [deployment identifiers](data/runtime-retirement-2026-10-20/deployment.json).
- The frame automatically upgrades through control v1 to the same signed release.
  The matching direct-only supervisor/service then restarts successfully. Final
  readiness: one renderer PID 5977, no Weston, confirmed Home, pending 0, RGB565,
  rotation 90 and calibrated touch. The enabled service and installed hashes are
  [recorded](data/runtime-retirement-2026-10-20/supervisor-install.json).
- The live browser edits Playground state, settles with pending/issued 0, and
  reports a shared replica/document root with zero compression scratch. Public
  [browser image](data/runtime-retirement-2026-10-20/live-browser.png),
  [diagnostics](data/runtime-retirement-2026-10-20/browser-resources.json),
  [frame readiness](data/runtime-retirement-2026-10-20/frame-acceptance.json) and
  [scanout](data/runtime-retirement-2026-10-20/frame.png) are recorded. Scanout is
  in physical portrait coordinates. Physical touch, color/orientation and reboot
  are not re-tested remotely; automated calibration/readiness is a separate fact.

Current code paths, resource ownership, meaningful process/overload tests,
browser/direct-frame acceptance and performance evidence satisfy RFC 0010's
bounded foundation gates. Normal replication stays raw sync-v4 packed transport;
canonical Merkle bytes and opening control v1 remain unchanged. Future collection
selectors, text CRDTs, general OS execution and further performance tuning have
separate scope and must preserve these invariants.
