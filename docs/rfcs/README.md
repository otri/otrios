<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC index

These records describe bounded designs, implementation and qualification at the time stated in each document. Follow links for scope, subsequent changes and evidence. Current operational guidance lives in the [dashboard guide](../DASHBOARD.md) and [deployment guide](../FRAME_HOSTING.md).

Numbers 0005 and 0006 each identify two distinct RFCs; use the full title when referring to either. Existing filenames and links are preserved. Assign a unique unused number to each new RFC.

| RFC | Recorded status |
| --- | --- |
| [RFC 0001: Own the Linux display and GLES2 renderer](0001-direct-linux-gles2.md) | Accepted; graphics boundary |
| [RFC 0002: Incremental CRDT hierarchy and immutable presentation](0002-incremental-crdt-hierarchy.md) | Design with bounded implementation |
| [RFC 0003: Module roots, typed selectors and scoped subscriptions](0003-module-roots-and-state-selectors.md) | Module isolation implemented; general selectors remain partial |
| [RFC 0004: Canvas mounts, executable behavior and app packages](0004-canvas-behavior-and-app-packages.md) | App cutover implemented |
| [RFC 0005: Bounded incremental Merkle hierarchy and subtree transport](0005-merkle-subtree-prototype.md) | Bounded library implemented |
| [RFC 0005: Parent-local pins and springs/struts](0005-pins-and-struts-layout.md) | Scoped prototype |
| [RFC 0006: Durable Canvas Merkle state adapter](0006-durable-canvas-merkle-adapter.md) | Deployed adapter |
| [RFC 0006: Font resources and text presentation](0006-font-resources-and-text-presentation.md) | Scoped font implementation |
| [RFC 0007: Bounded allocation and batched replication](0007-bounded-replication-pipeline.md) | Increments implemented |
| [RFC 0008: stable release hello and upgrade events](0008-release-control.md) | Hello/upgrade implemented; topology superseded by RFC 0009 |
| [RFC 0009: one server and negotiated client session](0009-unified-server-session.md) | Implemented and deployed |
| [RFC 0010: Practical Canvas runtime foundation](0010-runtime-foundation.md) | Implemented, qualified and deployed |
| [RFC 0011: Bounded packed Canvas transport](0011-packed-canvas-transport.md) | Deployed and qualified |
| [RFC 0012: Modular screen composition and local viewports](0012-modular-screen-composition.md) | Implemented |
| [RFC 0013: Independent app UI documents](0013-app-ui-updates.md) | Implemented and qualified |
| [RFC 0014: Independent Canvas behavior replacement](0014-app-behavior-updates.md) | Implemented and qualified |
| [RFC 0015: Editable Home and local reflow](0015-editable-home-reflow.md) | Historical design; see document |
| [RFC 0016: Public UI reuse and local navigation timings](0016-ui-cache-and-navigation-timings.md) | Implemented |
| [RFC 0017: Retained text viewports](0017-retained-text-viewports.md) | Implemented |
| [RFC 0018: Fixed point layout and SIMD stretch milestone](0018-fixed-point-layout-simd.md) | Evaluation complete; pilot retired |
| [RFC 0019: Native rendering optimization and SIMD evaluation](0019-native-fixed-simd-pipeline.md) | Evaluation complete; selected improvements retained |
| [RFC 0020: Reflowed documents with retained text chunks](0020-chunked-text-documents.md) | First delivery implemented |
| [RFC 0021: Timestamped touch history, lift-off and repeated strokes](0021-touch-history-and-momentum.md) | Implemented and qualified |
| [RFC 0022: A playful shared chat room](0022-playful-shared-chat-room.md) | Implementation qualified |
| [RFC 0023: Retained presentation and structured documents](0023-retained-presentation-foundation.md) | First delivery deployed |
| [RFC 0024: Viewport-scoped collection synchronization](0024-viewport-scoped-collections.md) | Implemented and deployed |
| [RFC 0025: Standalone app bundles](0025-standalone-app-bundles.md) | Signed bounded Canvas app installation, discovery, native/browser execution, upgrades and rollback qualified on Frame |
| [RFC 0026: Target-specific Wasm compilation](0026-target-wasm-compilation.md) | Proposed experiment; no execution backend selected or integrated |
| [RFC 0027: Spaces, membership and app data](0027-spaces-and-app-data.md) | Draft; Member/Admin roles, public audiences, Space-owned collections and app/data deletion lifecycle; replaces the proposed PrivateApplications namespace |
