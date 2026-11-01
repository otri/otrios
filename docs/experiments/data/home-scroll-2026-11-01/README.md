<!-- SPDX-License-Identifier: Apache-2.0 -->
# Home scrolling acceptance evidence

- `populated-browser.json`: four synthetic Spaces, fourteen apps, repeated scroll
  reversals, fade/footer pixel checks and first-attempt sign-in across resize.
- `populated-map.png`: synthetic fixture screenshot at the map's upper bound.
- `spaces-browser.json`: broader isolated browser regression. Its `release` is
  the disposable fixture release; `wasmSha256` binds it to the deployed Web code.
- `backup-summary.json`: aggregate private backup verification, without inventory.
- `frame-ready.json`: final signed release, renderer and service readiness.
- `frame-input.json`: three scroll round trips and top-left Brass Relay return
  through the physical reader; synthetic contacts, no app writes or pixel export.
- `public-conformance.json`: 171 public HTTPS checks; production login not tested.
- `data-safety.json`: safety roots, auth configuration, calibration and baseline
  state-file preservation, expressed as aggregates.
- `confirmed-data.json`: private-copy document/receipt comparison accounting for
  a retained live confirmed update and generated monitoring presentation.
- `mac-promoted.json`: independently verified Mac/Frame pointer agreement.

See [the qualification record](../../home-scroll-2026-11-01.md) for commands,
runtime/release hashes and the limits of automated physical-device acceptance.
