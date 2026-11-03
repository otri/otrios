<!-- SPDX-License-Identifier: Apache-2.0 -->
# O1 migration fixture

`oib-o1-ui.lz4` is the size-prepended LZ4 compression of the exact signed Live UI
blueprint from release 802dba22acd4be013d2d21109bcab12f86368b1b59644610391bc429f8583831,
`authority/bundles/live-ui.otapp/UI/main.flex`. The uncompressed SHA-256 is
5a89e88f9e00a6b3471c2c147c81a64c83d4d19fbaff7fe9e0b858c5e7384702.
It contains app-authored declarative UI data, not runtime implementation or user
state, and retains the app authoring data's Apache-2.0 license. The OIB test checks
this exact digest and verifies the explicit O1-to-O2 upgrade and journal recovery.
