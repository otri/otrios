<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB Designer design package

Date: 2026-11-11. Status: **M0 design and rapid prototype; production implementation
not started**. This package is the proposed complete editing experience, not an
announcement that its features are deployed. It extends the existing D2/L1/L2
foundation without rewriting those qualification records.

## Restart here after context compaction

1. Read this file, then [VALIDATION.md](VALIDATION.md), especially V09, and
   [COMPLETENESS_AUDIT.md](COMPLETENESS_AUDIT.md) for actual progress and gaps.
2. Read [DESIGN.md](../../DESIGN.md) for visual direction and
   [SHELL_UX.md](SHELL_UX.md) for workspace/inspector organization, and
   [COPY_AUDIT.md](COPY_AUDIT.md) for text decisions.
3. Use [UX_JOURNEYS.md](UX_JOURNEYS.md) J01–J27 for user narratives and
   [TECHNICAL_DESIGN.md](TECHNICAL_DESIGN.md) T01–T13 for contracts and algorithms.
4. Use [CONTROL_MATRIX.md](CONTROL_MATRIX.md) Q01–Q16 as the implementation and
   qualification ledger. Do not equate a prototype pass with a runtime pass.
5. Consult [REFERENCES.md](REFERENCES.md) for source provenance and the Figma
   alignment matrix. Then inspect the [prototype](prototype/index.html).
6. Before changing code, read repository AGENTS.md and required root documents.
   Check branch/status and the live source pin; do not assume this docs branch is
   deployed. Preserve the original repository on main.

## Decisions to preserve

- SpaceX-inspired monochrome OTriOS identity; readable ordinary UI typography.
- Descriptive named variables and typed named style references, no magic accent.
- Light/Dark mode inheritance; interaction state is a separate dimension.
- Horizontal/Vertical Auto Layout with Fixed/Hug/Fill; no constraint solver.
- Definition, preset and instance are distinct. Stable IDs survive renaming.
- Edit routes input to authoring; Test uses isolated sample data and behavior.
- A local working copy previews changes; explicit Apply confirms one authorized,
  generation-checked candidate. Publish and adoption are separate operations.
- Deleting an in-use definition blocks. Retire preserves existing uses. Replacement
  requires explicit typed mappings; no automatic cascade or cross-app transaction.
- One responsive workspace, no Frame/Desktop switch. Underlined task tabs, matching
  segmented Edit/Test and Dark/Light controls have distinct roles.
- Required design targets: logical Frame 1280×800 landscape and desktop 1920×1080.
  Phone portrait authoring is unsupported. A smaller landscape floor is proposed,
  not a reason to shrink Frame targets or raise scene limits.
- Unknown dependencies use conservative compilation. Proven color-only changes
  update paint; layout and geometry work are classified separately.
- Full lifecycle completeness does not mean full Figma/SwiftUI feature parity.
  Capability gates precede unsupported rendering, assets, text, wrap/grid and XR.

## Files and their authority

| File | Authority |
| --- | --- |
| Root DESIGN.md | Visual vocabulary, palette, typography and base control direction |
| RFC 0037 | Named-style/mode/state design; authoritative style precedence |
| SHELL_UX.md | Workspace, inspector, command and error presentation |
| UX_JOURNEYS.md | End-to-end behavior; all branches include cancellation/error outcomes |
| TECHNICAL_DESIGN.md | Proposed record, layout, transaction and dependency semantics |
| CONTROL_MATRIX.md | Representative control breadth and implementation/test backlog |
| REFERENCES.md | Reviewed references, inaccessible sources and alignment differences |
| COPY_AUDIT.md | Keep/remove/move decisions for all UI text families |
| COMPLETENESS_AUDIT.md | Field coverage, asymmetry findings and explicit remaining gaps |
| VALIDATION.md | Commands, evidence scope, findings and remaining qualification |
| prototype/ | Original disposable HTML/JS UX model; not a runtime implementation |

If narratives and technical semantics disagree, record and resolve the discrepancy;
do not silently implement whichever document was read most recently. Update this
entry point and the ledger when decisions, milestones or evidence change. Use
stable J/T/Q IDs in follow-up tasks and commits rather than copying whole files.

## Current state and next work

The design includes 27 journeys and 16 representative control rows. The prototype
covers a subset using native HTML controls, in-memory state, simple CSS layout and
simulated generation/publication. It intentionally uses no services, third-party
component source, production credentials, signing or Wasmi execution.

M0 is the design/prototype review checkpoint. M1 is the first production work:
versioned core layout contracts, measurement and input/layout golden fixtures.
M2 adds the coherent working-copy/Apply path; M3 adds styles and composition;
M4 dependency lifecycle; M5 full representative-control qualification; M6 signed
Frame/Web delivery. See T12 for exit criteria. Detailed wire addresses, bounds and
mixed structural/property Apply must be finalized before production coding.

This request authorizes design and prototype testing, not an automatic deployment
of the prototype as OIB Designer. Existing live OTriOS remains unchanged. No full
repository preflight or runtime qualification is claimed by this package.
