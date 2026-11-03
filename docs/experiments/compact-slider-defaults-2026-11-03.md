<!-- SPDX-License-Identifier: Apache-2.0 -->
# Compact slider component defaults — 2026-11-03

The approved compact design was stored as shared OIB edits while the signed app
still declared stacked rows. Reset UI defaults therefore erased the design, and
restoration could only preserve whatever geometry remained. The compact design
is now the authored reusable component default: label width 128, track x 144,
track width 288, percentage width 76, content y 0, row pitch 80, inline text y 10
and hidden redundant headings. Add continues to inherit shared component geometry;
Restore retains later edits; Reset returns to compact geometry.

The explicit signed upgrade allowlists the exact previous v24 UI hash
`c25ff9811d3c1744b78a01f769aca73bc5e81090083987144595c826adb54db2`.
Only same-type geometry defaults may differ in retained component cells; unrelated
contracts remain fixed. Existing draft/active values, presence, allocation history
and app values are preserved, including edits different from the new defaults.
Wire format and state protocol remain v24; release-control v1 is unchanged.

Focused checks cover fresh defaults, empty preview/re-add, later shared spacing,
Restore, Reset, stable mount/app values, exact predecessor upgrade, journal replay
and rejected unlisted or unrelated contract changes. The original stacked-input
regressions continue against the exact previous signed UI fixture.

## Executed validation and live acceptance

`cargo test -p otrios-core --features canvas --test oib --locked` passed all 28
tests. `cargo test -p otrios-core --features canvas --test spaces_process oib_
--locked`, with `OTRIOS_SPACES_SERVER` pointing to the freshly built authority,
passed both real process tests in 11.05 seconds. The initial invocation without
that environment variable was only a scaffold check. Focused strict Clippy passed
for the core OIB test target. Mac host tools, Wasm and ARMv7 hosts compiled through
the existing signed builder; signed release and app closures were verified.

Deployed release:
`a8341bf77f91c8c26cf2f6ae23bf75c1df51e0bd4f748865aa3970db49d93cdb`.
The verified matching code recovery kit is retained in the private builder under
`rollback/2026-11-03-compact-slider-defaults-code/<release>/`. It contains code and
matching helpers, with no app state or credentials. An older seed-incompatible
authority is not a rollback target after the binding update. A cross-release
rollback was not exercised.

Frame authority/native and the public Wasm client observed generations 129–135:
remove Volume, remove Brightness, instantiate a replacement, Restore sliders,
Reset UI defaults, then restore the owner's Volume/Brightness captions. Actual
scene tracks remained 288 × 48 with 80 px row pitch and inline labels. Reset used
the new defaults without a corrective geometry batch. Native PID 13653 and mount
epoch 1 remained unchanged throughout these live operations. Both slider values
remained 72% and 48%. Original IDs 20/30 and bindings are active again; the one
acceptance allocation is inactive and retained under the existing lifetime limit.

Public JS/Wasm/catalog/font bytes matched the verified signed candidate. The
deployed Wasm client confirmed the final generation, received every intermediate
generation, and retained its own local inspection. The temporary observer stopped;
the existing builder watcher was restored with the deployed source fingerprint.
Authority, direct display and tunnel were active at final health verification.
No full `scripts/check.sh`, physical finger acceptance or broad browser campaign
was run for this scoped default change. See the [machine-readable acceptance
record](data/compact-slider-defaults-2026-11-03.json).
