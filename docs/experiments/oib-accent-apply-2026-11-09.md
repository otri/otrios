<!-- SPDX-License-Identifier: Apache-2.0 -->
# Display accent Apply responsiveness — 2026-11-09

The user reported up to four seconds between changing Display's Accent and
tapping Apply in OIB Designer. A calibrated software touch through the live
Frame's existing evdev device reproduced **3718.379 ms** and **3403.804 ms**
from Apply touch-down to a native presentation with the new accent, confirmed
state, no pending commands and released input. The two edits changed blue then
restored the user's gold override. No app data was reset.

The [earlier diagnosis](oib-ui-rebuild-2026-11-09.md) isolated repeated source
processing and structural toolbar preparation. The focused fix:

- Checks OIB root, confirmation, saving and editability before formatting status
  on an unchanged Designer tick.
- Keeps one validated source entry keyed by exact immutable source and signed
  baseline leaves plus admitted UI identity. Document clones share the bounded
  entry. Changed or malformed bytes still run canonical validation.
- Computes the definitions/presets digest once on a source miss, preserving
  authored UI reuse for placement/residency-only changes.
- Reads allocation, placement and connection metadata from shared source rather
  than cloning and validating the complete document for each accessor.
- Prepares structural tools only when their pane is visible. Frame Focus's
  closed Layers drawer presents no structural controls. Opening Layers
  regenerates the current actions, and Desktop Studio keeps preparing its
  visible tools. Actual structural candidate and authority validation is intact.
- Validates a normalized immutable signed seed on its binding-cache miss rather
  than repeating that check on every property delta. The key includes exact
  baseline bytes and admitted UI identity.
- Retains at most two successful compiled UI/layout results. Keys include the
  admitted UI identity, exact binding/source leaves, live instances, canonically
  encoded effective property values and selection. Equivalent draft/active
  compiler inputs reuse a result; new bytes, residency, values or selection
  rekey it. Raw causal/schema/closure validation still runs independently.

There are no dependency, wire-format, state-protocol, signature or trust-root
changes. State protocol remains `otrios-canvas-sync-v30` and release control v1.

## Focused verification

Using the private Rust 1.90.0 environment:

```sh
cargo test -p otrios-core --features canvas --lib --release --locked --offline \
  -- cache_tests display_root_accent_apply \
  frame_focus_invalid_drafts_cancel_and_color_picker_stay_local_until_apply \
  compatible_library_activation_waits_for_held_slider --nocapture
cargo test -p otrios-core --features canvas --lib --release --locked --offline \
  -- oib_designer::tests --nocapture
```

The first run passed six tests, including three cache tests. The second passed
44 editor tests with the manual diagnostic ignored. The added regression uses
Display root 30's actual Accent property and visible Apply action twice. It
checks local-only draft selection, exactly one confirmed revision per Apply,
unchanged structural generation/business values/Audio register, fresh actions
when reopening Layers, and clean drafts afterward. Existing drawer, Desktop
Studio, stale-input, modal, scrolling and held-library-activation tests passed.

After adding the two-entry compiler cache and moving immutable seed validation,
the combined editor/cache release run passed **50 tests**, with one manual
diagnostic ignored. All **10 library-adoption tests** passed, including restart,
rollback, coalesced updates and malformed metadata. Core Canvas Clippy with
`-D warnings`, formatting and `git diff --check` passed.

The ignored optimized diagnostic also passed using the exact deployed
library-backed UI in a fresh isolated Mac authority/client. Confirmed property
Mount refresh decreased from a 42.801 ms median to 3.885 ms; unchanged client
ticks decreased from 314 to 2 microseconds. These are Mac stage measurements,
not Frame end-to-end timings; unchanged tick samples include one warm-up outlier.

## Live measurement method

The probe uses the existing strict SSH configuration and accepted touch
calibration. It checks the selected Display instance, native idle state,
rotation/output and absence of physical touch. Each tap holds for 80 ms. It
polls native readiness every 5 ms until the expected active and presented
generation match, Accent contains the expected color, state is confirmed,
input is idle and pending commands are zero. Timing includes the hold and poll
resolution. This measures software touch-to-confirmed native presentation,
not photon latency. No pixels, signing keys or authentication tokens are exported.

Two colors exercise actual changes, with the original gold override restored.
The observation client used to locate controls is separate from the native
renderer; only native readiness supplies presentation completion.

## Final signed release and live result

The deployed signed Frame/Web release is
`388d7d8209a44e6b773e585483718993fb64ecfee2ea187f366976d4ea0b2c5a`,
built from implementation commit `cfadf50`. Its Designer UI and exact library
bytes are unchanged from the preceding release. The authority restarted for
the code update and recovered its existing state; no collection was reset.

| Native Accent Apply | Before | Final |
| --- | ---: | ---: |
| Blue | 3718.379 ms | 790.655 ms |
| Restore gold | 3403.804 ms | 642.752 ms |
| Mean of these two trials | 3561.092 ms | 716.704 ms |

The measured mean improved **4.97×**. New property values still incur compilation,
raw state validation, confirmation and composition; the result remains a
noticeable subsecond operation. These two trials are not a latency distribution
or an assertion about every authored document.

An intermediate deployment measured 2120.002 and 1679.965 ms before the compiler
cache and immutable-seed validation changes. The final idle UI/state worker used
11.61% of one core over 3.014 seconds, compared with the diagnosis's 90.17% over
3.016 seconds. Native whole-process RSS was 61,452 kB, including graphics/fonts;
this is a point observation rather than a controlled cache-memory benchmark.

The signed ARM/Wasm build, native readiness, matching runtime/release pointers,
confirmed idle input, authority/tunnel services, Monitoring collector and public
HTTPS health all passed. The readiness helper's `authority_restarted: false`
describes that read-only check; the deployment itself restarted the authority.
Browser interaction testing was not repeated for this Frame-focused correction.

Evidence: [summary](data/oib-accent-apply-2026-11-09/summary.json),
[before](data/oib-accent-apply-2026-11-09/native-before.json),
[final](data/oib-accent-apply-2026-11-09/native-after.json),
[readiness](data/oib-accent-apply-2026-11-09/readiness.json),
[idle CPU](data/oib-accent-apply-2026-11-09/idle-after.json),
[Mac stages](data/oib-accent-apply-2026-11-09/mac-after.jsonl), and
[bounded native touch probe](data/oib-accent-apply-2026-11-09/native-probe.py).
The probe requires an idle Frame and the existing gold override, selects Display
then its explicit Edit action, and restores gold after the two Apply trials.

The full repository suite and Linux shared-memory IPC acceptance are outside
this focused UI change.
