<!-- SPDX-License-Identifier: Apache-2.0 -->
# Experiment summaries and qualification

Current operation follows [frame hosting](../FRAME_HOSTING.md) and the
[modern app pattern](../MODERN_APP_PATTERN.md). Experiment results apply to the
revision, workload and conditions recorded in each report.

## Current qualification records

- [Viewport-scoped collections — 2026-10-28](collection-windows-2026-10-28.md): bounded windows, structured messages, recovery and browser/frame deployment.
- [Brass app theme — 2026-10-28](brass-app-theme-2026-10-28.md): shared headers, navigation and visual qualification.
- [Frame keyboard — 2026-10-28](frame-keyboard-2026-10-28.md): native keyboard qualification and limitations.
- [Repository cleanup — 2026-10-28](repository-cleanup-2026-10-28.md): tooling, signed release verification and local artifact retention.

## Historical conclusions

| Topic | Reports |
| --- | --- |
| Early dashboard, physical-input confirmations and app cutovers | [Consolidated 2026-10-18–24 summary](early-dashboard-and-app-cutovers.md) |
| Merkle replication and ownership | [Subtree validation](merkle-subtree-validation.md), [adapter comparison](canvas-adapter-frame-benchmark.md), [runtime foundation](canvas-foundation.md), [retirement](runtime-retirement.md) |
| Transport and session evolution | [Replication increments](canvas-replication-pipeline.md), [packed transport](canvas-packed-wire.md), [unified session](unified-session-deployment.md) |
| Frame authority and load limits | [Authority qualification](frame-authority-2026-10-20.md), [complete-client burst tests](frame-burst-capacity-2026-10-20.md) |
| Graphics and text | [Direct display](direct-display-validation.md), [text rendering](text-rendering-performance.md), [retained text/SIMD](native-retained-simd-2026-10-27.md), [chunked documents](chunked-documents-2026-10-27.md) |
| Browser startup | [Frozen baseline](browser-startup-profile.md), [implemented improvements](startup-improvements.md) |
| Input and presentation | [Browser/native input](browser-native-input-2026-10-27.md), [retained presentation](retained-presentation-2026-10-27.md), [chat input and anchors](chat-input-and-anchors-2026-10-27.md) |

## Archived raw evidence

The documentation consolidation retired **170 older raw measurement and image
files (15,322,715 bytes)** from the working tree and replaced ten early reports
with the cutover summary. Concise outcomes, methods and limitations remain in the
reports above. The 22 raw qualification records/assets explicitly dated
2026-10-28 remain in the current tree.

Links labeled **Archived figure** open historical evidence without embedding old
screenshots in the current report. Older raw files and the full retired reports
are recoverable from
[revision b2355f4](https://github.com/otri/otrios/tree/b2355f443ce5f545a2f0f5dfe55ae358e5ce7b45/docs/experiments).
Retained reports link to specific files at that revision. Reproduce an older
measurement from its recorded revision and conditions. Removing files from the
working tree does not remove Git history or shrink an existing clone's history.

The [maintenance guide](../MAINTENANCE.md) describes routine cache cleanup and
the evidence to retain during future documentation consolidation.
