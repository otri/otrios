<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB designer D1 execution plan

Date: 2026-11-03. Status: **in progress; acceptance not yet run**.

Implements only D1 of [RFC 0035](rfcs/0035-oib-component-designer.md).
The integration branch is `feat/oib-designer-d1`, based on
`19cd6407ee1196cd88ecc09e4af41466f30fd7b0`. The user authorized isolated
worktrees, bounded subagents, centralized orchestration and review, and work
toward this milestone on 2026-11-03.

## Concrete goal

Deliver a shared component inspector with an explicit OIB document root, a
selectable hierarchy, and a runtime canvas showing two linked `SettingsPanel`
instances. Each contains reusable plain-text/shape-based slider and toggle
components. Authors can edit admitted definition properties and nested instance
overrides, identify their source, and reset them correctly. Existing confirmed
application values and signed field bindings survive design edits and recovery.

Completion requires the integrated implementation, focused regression and
authority-process evidence, independent review with findings resolved, a verified
signed release, and focused acceptance on the existing Frame/Web environment
with a compatible code rollback available. Passing local tests alone does not
complete D1. Do not open a progress or draft PR.

## Scope boundary

Include document/class/property descriptors, stable full instance ancestry,
component references, exposed properties and forwarding, authored-default/live
override precedence, signed slot declarations, modular inspector sections,
whole-view activation for buttons/toggles, and existing bounded instance lifecycle.
Use existing layout, painting, responder, CRDT, persistence and session systems.

Keep definition structure and app-state connections signed in D1. No arbitrary
reparenting, user-created slot content, connection editing, general content
measurement, mixed document/editor behaviors, rich typography, bitmap/vector
image import, XR engine, new execution backend, or generalized transaction API.
`O2DTextArea` remains in the RFC's D3 set; `O2DLabel` is removed entirely.

The fixture must fit the existing admitted budgets. Nested inherited occurrences
are identified by ancestry and count toward expanded node/control/cell limits;
do not confuse them with separately allocated live instance roots. Select the
smallest fixture that proves repeated nested reuse. Preserve the existing
Live UI volume/brightness values and factory-specific labels. D1 uses a separate
`oib-designer` companion app/collection with its own signed numeric/toggle fields;
the current `live-ui-state-v1` schema and signed factories stay unchanged. This
avoids reshaping retained slider allocations or migrating the old app schema as
an incidental requirement. Never reset an existing collection to make the fixture
work. Data-only design edits and recovery must preserve the new fixture's values
as well as continued operation of the unchanged Live UI app.

## Ownership and worktree discipline

| Role | Deliverable and owned area | Boundary |
| --- | --- | --- |
| Parent orchestrator/integrator | Contract decisions, this plan, reviewed cherry-picks, integration glue, protocol coordination, release and live evidence | Sole integration/deployment owner |
| Contracts and identity implementer | Minimal public D1 records and SDK exposure; canonical encoding/addressing notes | `crates/otrios-interfaces` OIB contracts, SDK declarations, dedicated contract note |
| Runtime mapper, then implementer | Identify and implement bounded graph admission/resolution and current-cell lowering | Runtime implementation and tests in `crates/otrios-core`; no permissive runtime copies |
| Designer/fixture mapper, then implementer | Hierarchy/inspector interactions, signed nested fixture, authoring support and budget accounting | New `apps/oib-designer`, Test Playground registration, shared authoring helpers; existing Live UI retained |
| Independent reviewer/test role | Concrete integrated diff review and focused process/input/compatibility evidence | New delegate task for each review/re-check; no self-approval |

Every child uses a tool-managed worktree from a committed baseline. Children do
not edit sibling or original checkouts, create additional worktrees, subdelegate,
push, open PRs, deploy, change private builder configuration, add dependencies or
introduce unsafe code. Each records its base/head, changed files, actual commands
and results, unresolved findings, and integration assumptions. Completed scoped
changes are committed locally for parent review and cherry-pick.

Use per-worktree build outputs. Do not share writable Cargo target directories
or release staging directories with the existing test/commit work. Only the
parent coordinates a signed deployment after confirming no other deployment is
in flight. Preserve established signing keys, identities and strict host pinning.

## Execution and integration gates

### G0: shared design baseline

Restore and commit the RFC, its links, and this plan only in the integration
worktree. Leave `/Users/otri/Projects/otrios` and
`/private/tmp/otrios-oib-merge` untouched. Launch three isolated first-wave
tasks: public-contract implementation, read-only runtime mapping, and read-only
designer/fixture/acceptance mapping. Mapping tasks report concrete paths and
objections before their implementation tasks are assigned.

### G1: contract freeze and compatibility decision

Parent reviews the public contract with both mapping reports before runtime/UI
implementation proceeds. Require a coherent path from a stable nested property
address to the existing causal cell, render target and semantic responder.
Avoid a second independent UI tree or property-value store. Declare ownership,
signed defaults, forwarding, bounds, error behavior and canonical bytes.

Contract work may add declarations without activating them. Attaching new fields
to signed seeds or live wire state, changing negotiation, and migrating existing
data are integration decisions. Select the required feature and then-current
state protocol once that encoding is concrete. Preserve release-control v1.
Do not lower full ancestry to a collision-prone process hash or renderer slot.

Parent decisions after the runtime mapping:

- Give each admitted root-relative property address an explicit immutable signed
  cell key. Existing authored cells keep explicit aliases; dynamically allocated
  roots can retain checked ID/64-key arithmetic. Descriptor order, names and
  traversal never assign identity. Reject duplicate keys, collisions and exhaustion.
- Keep one source per property. Literal overrides may mask an admitted forwarding
  source and clear back to its current value. App-bound value properties cannot
  be disconnected by writing a literal UI override. Explicit reset and resolving
  a conflict to an explicit literal are distinct operations.
- Keep v26 Live UI definitions, schema and collection usable unchanged; introduce
  D1 in its own companion app. Preserve canonical old seed bytes when optional
  D1 metadata is absent. General retained-graph/schema migration remains deferred.
- Apply the narrowest limit at every layer, including existing template depth 12
  and instance depth 8; scene depth 16 does not override those limits.
- Extend factory admission/remapping for signed, field-checked toggle actions;
  the existing Set-only assumption is insufficient for this fixture.
- New real authority-process tests must set `OTRIOS_SPACES_SERVER` explicitly and
  use distinct volume/brightness fields. A silently skipped process test is not
  recovery evidence.
- Generate only the locally selected inspector. Hierarchy expansion and row
  selection must not allocate a replicated handler/property set per descendant.
  Use core-local routing for designer controls and keep Edit/Test mode local.
- Start with admitted boolean choices, bounded numeric suggestions/steps, color
  palettes, and text suggestions. General free-text inspector editing requires a
  deliberate adapter and is not implied by an existing text-input demo.
- Target three reusable definitions and roughly 20 shared plus six occurrence
  cells per resident panel. This is a design estimate: six roots would use 56
  cells, while ten populated residents would exceed 64. Admission must check the
  complete resident closure before allocation; do not raise ceilings silently.

### G2: runtime and designer implementation

After G1, launch runtime and app/inspector implementation on the same reviewed
contract commit with explicit file ownership. Partition any shared core inspector
or input files before edits begin. Parent resolves API questions and reviews each
completed commit before integration. Keep follow-ups small and scoped to D1.

Use the smallest end-to-end vertical slice first: select a nested occurrence,
edit a definition accent, override one occurrence, reset to authored/forwarded
inheritance, and observe both panels without modifying their app values. Extend
that path to lifecycle, validation and input. Compile and use targeted checks;
avoid routine broad preflight or repeated platform qualification.

### G3: integrated evidence and independent review

| Acceptance requirement | Required evidence |
| --- | --- |
| Stable composition | Two instances reuse the same nested definitions; ancestry distinguishes every occurrence; authored rename/reorder fixtures preserve surviving addresses |
| Correct values and reset | Shared edits propagate; nested overrides stay local; equal-to-default is still explicit; reset reveals authored defaults/forwarding; bindings and app values remain unchanged |
| Coherent publication | Disjoint edits merge; same-property conflicts retain the exact active graph; resolution activates consistently; invalid graph/type/expansion leaves active state usable |
| Lifecycle and recovery | Fresh IDs, reusable rendering slots, bounded repeated create/remove, retired-operation safety, pinned snapshots, receipt replay and real authority-process restart |
| Input and local state | Whole-view button/toggle hits, visible clipped controls, decorative pass-through, modal blocking and bubbling; held gestures retain targets; selection/scroll/focus stay local |
| Compatibility and scope | Required feature gates unsupported hosts; browser/native/server codecs agree; old state negotiation cannot prevent release discovery; isolated Space permissions hold |

Use existing focused suites and add meaningful tests for changed behavior.
Distinguish in-process tests from process-boundary acceptance. Run broader checks
only where the integrated change or a concrete failure warrants them, and record
any required check that remains unrun. The independent reviewer receives the
actual integrated base/head, scope, test results and known objections; resolve
findings before release completion. Each new review/re-check is a new delegate.

### G4: signed deployment and focused live acceptance

Parent uses the established verified build/sign/activation workflow with isolated
staging and a known compatible code rollback. Confirm the current release and
deployment owner before activation; do not collide with the other task's release.
No routine data inventory or paired backup campaign is required.

Verify native and browser agree on the active release and edited component UI.
Perform the core shared-edit/override/reset sequence plus held-slider input and
bounded lifecycle on the deployed system. Record commands, source/release hashes,
confirm-to-present observations, unchanged app values, and observed failures.
Keep unperformed physical/browser checks explicitly pending. Code rollback never
restores old application data. Complete readiness/health acceptance once for the
finished release. Only then is D1 ready for final review and any requested PR.

## Current evidence

- Isolation: parent moved to the dedicated integration worktree; original
  checkout changes were read and snapshotted without modification.
- RFC baseline and execution plan: prepared; local documentation checks recorded
  in the parent handoff when committed.
- Contracts: additive descriptor implementation delegated; parent contract freeze
  pending its result and both mapping reports.
- Runtime mapping completed read-only: existing causal publication and rendering
  can be reused; property-rank IDs, toggle admission, full-ancestry selection and
  forwarding need focused changes. No runtime implementation accepted yet.
- Designer/fixture mapping completed read-only: use three local scrolling areas,
  a nonvisual object selection case, one selected inspector, signed numeric
  0/1000 toggle fields with boolean semantics, and explicit Edit/Test interaction.
  Fixture estimates require measured admission before activation. Implementation
  remains pending contract freeze.
- Runtime tests, process acceptance, deployment and live acceptance: not run.

Update this record at integration gates with actual commits, commands and results.
