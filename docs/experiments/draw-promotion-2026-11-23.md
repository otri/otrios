<!-- SPDX-License-Identifier: Apache-2.0 -->
# Draw promotion — 2026-11-23

Touch Tracing is now **Draw**, launched from the top-level Public space on Home
at `/draw`. It is removed from Test Playground's child navigation. The source
Home UI also declares the new root route. The managed live Home keeps its signed
bundle and derives the new tile from the refreshed catalog.

The stable `touch-trace` app ID, UI/data references, schema, Public attachment and
collection are retained. This is a rename and route relocation, not a new dataset.
All runtime/Web assets, app update contracts and unrelated bundles remain exact.
Only the catalog and the Draw/Test Playground bundles change. The established
publisher signed the complete closure, verified before activation.

Signed release:
`58c5729802b5b6c30c11112c2dbc667ea541e6b40ae2c5b11a70b276ca75f2a8`.
Baseline:
`5826d7ed8cf0fd23bdd1d3cf2d56856b1023d2f9e7fb0bb3ef4695fc9bc65631`.

Validation was limited to the app packer build/qualification, signed bundle
revision and compatibility checks, complete release verification, and live
navigation. No host rebuild or broad test suite was needed. The existing
`space_map_probe --controls --native` rendered Draw in Public Home from the live
catalog and `/spaces` metadata; `--playground-controls` contained neither Draw nor
Touch Tracing. Frame opened `/draw` from Home with a live stream, confirmed state,
and the same 1,953 points/30 segments observed immediately before activation.

The current hot catalog guard rejects changed App metadata, including a title,
even when the signed update contract is unchanged. Frame therefore needed a
one-time clean reload of the already verified, identical native runtime. The
signed cached candidate was verified, input/confirmation checked, the service
stopped, and the prior last-good pointer retained while the normal bootstrap
accepted and recorded the new release. Existing browser tabs need to reopen
Home or `/draw`; no old-route alias is installed. General hot title updates are
a separate runtime follow-up.
