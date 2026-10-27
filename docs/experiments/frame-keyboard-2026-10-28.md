<!-- SPDX-License-Identifier: Apache-2.0 -->
# Frame keyboard collection, dispatch and feedback — 2026-10-28

The reported symptoms were slow typing requiring long pauses and no visible
depressed keys. Inspection found that native state and network workers still
existed, but evdev reads and dispatch ran inside the graphics loop. Uploading,
drawing, glyph preparation and waiting for a page flip could delay collection.
Keyboard contact state was tracked but never painted. Every contact also
invalidated the authored scene, even when its labels and text had not changed.

The recent momentum changes do not impose a keyboard long-press threshold:
software keys bypass the generic scroll/momentum router. Normal characters
commit on release; repeatable keys act on down and use the existing repeat
delay. Edits apply locally without a CRDT/network receipt. These findings do
not identify a single recent commit as the cause of the physical symptom.

The fix gives evdev a separate same-thread owner and a dedicated collector/
dispatcher, preserving sampled times, calibration and the presented activation
and viewport. A bounded queue feeds the existing state owner. Graphics and
network owners remain separate. Replacement, revocation and lost evdev streams
cancel captured contacts; overload/device failure is explicit. The new input
owner never calls GL and stays inside the existing reviewed FFI boundary.

evdev supplies only changed axes. The old adapter queried current axis metadata
but initialized its cached position to zero. It now initializes both coordinates
from EVIOCGABS and re-queries them after SYN_DROPPED recovery. Existing held-contact
suppression remains in force. No kernel, driver or calibration changes were made.

Held keys now paint a brass effects overlay while retaining scene/text geometry.
A released key retains a short pulse until a frame includes it, then starts its
60 ms timeout. Renderer backpressure therefore cannot expire an unseen short
tap. One latest pulse is retained; there is no unbounded animation queue.
Cancellation, focus changes and hiding the keyboard clear it.

## Validation

`bash scripts/check.sh` passed, including real process/socket tests outside the
filesystem sandbox. The initial sandbox run stopped at a denied local socket;
it is not counted as a process-test pass. The final run passed after the pulse
lifetime correction. Separate native tests passed:

```sh
cargo test -p otrios-dashboard --features direct --bin dashboard-direct --locked --offline
bash scripts/test-linux-touch.sh
cargo clippy -p otrios-dashboard --all-targets --features direct,server,canvas \
  --target armv7-unknown-linux-gnueabihf --locked --offline -- -D warnings
```

The 11 native worker/input tests include dispatch while the graphics owner is
blocked, source timing, mount replacement, revocation, queue overload, device
failure and renderer credit. The core regression checks held/short taps,
retained scene/base/keyboard buffers, cancellation and an 878 ms renderer stall
between release and the next composed frame. The Linux syscall fixtures cover
unchanged first-tap coordinates, one changed axis, bounded batches, stream loss,
held-contact suppression, disconnect and partial-init cleanup. These fixtures
need no real input device or input privileges and run in `scripts/check.sh` on
Linux.

79 Node host/worker/input/GPU checks passed. Locked Rust 1.90.0 Wasm and ARMv7
release builds passed. Real Chrome on a disposable loopback authority passed
typing, selection, replacement, IME, chat, reload and graphics-recovery checks;
the final signed Wasm was then loaded and verified separately. No fixture
message was sent to production.

## Physical renderer qualification and limits

`keyboard_latency_probe` mounts the packaged Brass Relay locally, with particles
disabled. A timed source feeds 52 keys at 60 ms down / 60 ms up into a separate
state owner using the production editor, Composer, one-frame renderer credit,
GPU resource adapter and GLES2 display. It verifies the exact resulting text,
every edge, real kernel flip completions and changed depressed-key pixels.
No authority or network is involved. Runs used the exclusive display lock,
restored console mode and restarted the previously active native service in a
`finally` block.

| Build/run | Characters/edges | Release→present p95 | Key pixels |
| --- | --- | --- | --- |
| Deployed base, landscape 1 | 52 / 104 | 48.6 ms | absent |
| Deployed base, landscape 2 | 52 / 104 | 50.1 ms | absent |
| Candidate, landscape 1 | 52 / 104 | 51.5 ms | present |
| Candidate, landscape 2 | 52 / 104 | 49.0 ms | present |
| Candidate, portrait | 52 / 104 | 55.0 ms | present |

The candidate accepted all 156 characters at 8.33 keys/second. State dispatch
p95 was 2.0–2.2 ms and composition p95 8.5–8.8 ms. This synthetic source bypasses
the physical sensor/evdev collector, so matching baseline render timings do not
measure or establish a physical typing speedup. The blocked-graphics bridge
test and evdev fixtures validate those paths separately. Human typing feel and
long-term soak remain unverified.

The first depressed frame is read back to prove pixels; that instrumentation
and cold glyph preparation are included in candidate maxima of 228–263 ms.
Baseline has no depressed-frame readback. Those maxima are not comparable
steady-state latency claims. The measurements and artifact hashes are retained
in [the result data](data/frame-keyboard-2026-10-28.json).

## Release

The signed native/web release is
`35b71e2488fb2125d1b27bd58861733e6566e70bc702cbaf237debdcbd99efb2`;
native SHA-256 is
`9d9bb4e67aae97bd876fa200f96683f400b3bcac1192308df36860fae01ad31e`.
State protocol remains v8 and the opening release-control envelope remains v1.
The stopped-state rollback snapshot is
`/var/lib/otrios-authority/rollback/2026-10-28-keyboard`, preserving the prior
signed release, app journals and installed executables. The previous RFC 0024
rollback snapshot is retained separately.

Signed activation completed on both authority and native caches. The running
native executable matched the signed SHA-256, reported confirmed state with no
pending commands, opened `/dev/input/event0` and retained six process threads.
All four production services are active; the accepted calibration file kept
SHA-256 `44f2b33a3a17140e6c662b9feba1f009c9ed7910f2c1fa2178b7f5a24a53859e`.

The final public checker passed 120 checks. An earlier run overlapped activation
and observed different release IDs between requests; only the post-activation
run is counted. A real public Chrome probe loaded the new signed Wasm, negotiated
control v1/state v8 and recovered the same final ten message sequences and text
digest as before deployment:
`46d217869c7043560e90d1e64fe0dff465bc818a5c779bdb355739bf45007335`.
It sent no message commands. The disposable local authority was stopped and
this task's two remote probe directories were removed. The release is available
for hands-on typing at the Frame and [the web app](https://otrios.otri.ca).
