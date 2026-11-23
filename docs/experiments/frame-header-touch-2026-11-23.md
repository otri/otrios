<!-- SPDX-License-Identifier: Apache-2.0 -->
# Frame header touch target — 2026-11-23

Physical taps on Monitoring's OTriOS link reached evdev but could land in the
header's unresponsive top gutter. A calibrated injected center tap opened Home;
the user also opened Brass Relay by tapping its Home tile. Input delivery was
working. First-row breadcrumb/account/menu controls now include the top gutter,
with their painted children left in exactly the same positions. Parent clipping
and modal input barriers remain in place.

Signed release: `38f14b74908be456c9d7edd488bb9d3ccb974513e88e8189305c3e710a20403a`.
Complete release signature/digest verification passed; all app bundles/contracts
and state protocol v42 are unchanged. Compatible preceding release `92d1e353511984348ecaf2d04a69dfb9aa94cab5e8b59a75ec43fa871bbc129b` was retained during development.

Validation: the existing local Monitoring core regression passed with checks for
hit targets at `[80,12]` and `[80,55]`; release Wasm and ARM display/collector/bootstrap
builds passed. After deployment, the user tapped OTriOS and confirmed: **Yes, Home
opens**. No broad suite or interactive browser qualification was run.

The controller was reloaded once using the existing accepted firmware/config/rule;
this did not resolve the header miss. An attended seven-target diagnostic measured
9.4px fit residual, but one edge failed its 24px threshold (30.9px; the other 22.6px).
The candidate was rejected and never installed. Accepted calibration and firmware
remain unchanged: rule SHA-256 `a8f9fc76ad8c66fe9a2c4d785846b04d3660d9871ed5a49fb3aa41fa1d6be3fa`,
config SHA-256 `723e0773d6b197d505ed58bc4310cf0af3dd6478a335bc0b9599a6aa0308825a`.
The native display resumed after measurement. Technical evidence is in the existing
Mac checks directory as `frame-header-touch.json` and
`frame-touch-measurement-rejected-20261123.json`.
