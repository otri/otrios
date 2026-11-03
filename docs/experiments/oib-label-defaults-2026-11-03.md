<!-- SPDX-License-Identifier: Apache-2.0 -->
# Volume/Brightness authored label defaults — 2026-11-03

The owner reported that a restored Brightness slider had the correct identity but
its label reset/choices offered Volume. Read-only Frame inspection at generation
204 found restored objects 65553 (Volume factory 20) and 65554 (Brightness factory
30), both inheriting component label `Volume`. Both signed label suggestion lists
were `Level, Volume, Sound`. The original data bindings remained separate.

[RFC 0034](../rfcs/0034-oib-authored-instance-defaults.md) introduces an explicit
app-authored instance override beneath live overrides and above shared component
values. Live UI declares Volume and Brightness label defaults and suggestions
separately; style still inherits the shared component. Reset reveals the authored
label; Default explicitly writes it. Duplicates and restored factories carry
their own authored layer. Whole-UI Reset preserves the signed authored defaults.
Existing explicit user overrides remain until cleared.

Protocol v26 carries the new seed field; release-control v1 remains unchanged.
Bundles require `otrios-oib-authored-defaults-v1`. The exact previous v25 UI fixture
verifies canonical decoding and the explicit signed migration while preserving
existing property values, allocator state and app content. No new dependencies,
services, identities, trust roots or touch mapping changes were introduced.

## Executed checks

- `cargo test -p otrios-core --features canvas --test oib --locked` passed all 35
  cases in 63.52 seconds. A new test recreates both sliders, selects Brightness,
  activates its real Reset/Change/Default controls and checks Volume's override
  and both data fields. Another checks the exact deployed predecessor upgrade
  and journal replay. Existing reclamation, gesture and compact-layout cases pass.
- Strict core Clippy with `--features canvas --all-targets -- -D warnings` passed.
- Both real OIB authority-process tests passed against the newly built server;
  both unified-session tests passed, including obsolete codec/recovery isolation.
- Focused Node session/update/worker tests passed all 34 cases. Python release
  verification passed all 35 cases. Mac tools, Wasm and ARMv7 binaries compiled;
  complete signed release/app closures were verified before deployment.

No full `scripts/check.sh`, broad browser campaign or cross-release rollback was
run. These checks do not claim Linux shared-memory IPC acceptance.

## Live acceptance

Deployed release:
`2b89991d519006358a070b24900c49d0489a2096b8d03b016c9dec4d1079e4f7`.
The private builder retains a verified matching v26 code recovery kit at
`rollback/2026-11-03-oib-label-defaults-code/<release>/`, with the signed closure
and matching server, checker and bootstrap helpers. Older codecs are not rollback
targets after this metadata upgrade.

Guarded synthetic calibrated evdev taps selected Brightness on Frame. A CRDT edit
deliberately set only its label to Volume, then the actual native controls produced:

| Control | Result |
| --- | --- |
| Brightness Label Reset | `Brightness (authored)` |
| Brightness Label Change | `Light (override)` |
| Brightness Label Default | `Brightness (override)` |
| Brightness Label Reset | `Brightness (authored)` |

Every other property and the two-instance roster stayed unchanged. Volume and
Brightness values remained 68% and 31%. Native PID 24818 and mount epoch 1 stayed
stable throughout the data/button checks; Brightness 65554 remained selected.
The compact geometry and 80 px row pitch were preserved. This was automated native
input acceptance, not a new human finger calibration pass.

The deployed public Wasm client observed every generation from 205 through 210
and the same final authored label. Public JS/Wasm/catalog/font bytes matched the
verified signed candidate. The temporary observer stopped; the existing watcher
was restored with current v26 tooling and source fingerprint
`720c303474727200544a1575486fa19c7360c5918bd667fb724baf776c9e2fbd`.
Authority, direct display and tunnel were active at the final confirmed view.
See the [machine-readable acceptance record](data/oib-label-defaults-2026-11-03.json).
