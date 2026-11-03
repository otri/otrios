# Frame touch range correction — 2026-11-03

The native Live UI sliders stopped near 14–15% while the finger remained down
and continued to the physical left edge. This is fixed on Frame. The user
confirmed the live result: “Hey, we got it working!”

## Cause and correction

The recorded [original drag](data/frame-touch-range-2026-11-03/before-drag.json)
held at evdev X=794, mapping to logical X≈125.49. The slider track starts at
X=48. Core input capture remained active; it was not an overlapping control.

The Linux-side vendor algorithm in PhotoFrame's
`porting/linux-kiosk/touch/gsl_point_id.c`, `ScreenResolution()` (lines
1374–1425), scales the first coordinate using 15 sensor cells, applies legacy
gain/offset, then clips at `800 - 5 - 1 = 794`. Offline firmware analysis found
a 16-cell axis: the centroid routine at 0x4900 clips against
`byte[0x17f] * 64 - 1`, with the loaded byte equal to 16.

The host config now matches 16×10 cells. Its gain/offset is neutral, leaving the
existing native affine calibration responsible for screen mapping. Only four
little-endian config words change: 0x12/0x13 become `0x000a0010`, and 0x23/0x24
become `0x10000000`. The existing five-/seven-unit safety margins remain.
Neither the controller firmware nor Linux kernel was modified.

The accepted calibration was composed with the old-to-new normalized evdev
coordinate transform. This retains its interior mapping instead of requiring a
new physical fit. The [offline generator](../../scripts/prepare-frame-touch-mapping.py)
reproduces the installed config and udev rule byte for byte, rejecting any
unexpected original configuration. It writes files only and does not deploy.

Two fresh finger fits were rejected before this conversion was selected. One
missed an edge threshold; the repeat failed the center holdout. Their temporary
configurations were rolled back, and neither fitted matrix was applied. The
diagnostic now samples the median held contact, excluding acquisition/lift
samples when a sufficiently long sample history is available.

## Live acceptance

The [corrected recording](data/frame-touch-range-2026-11-03/after-drags.json)
contains 269 read-only evdev samples, including navigation and both sliders.
The final volume drag spans logical X≈16.11–650.59; brightness spans
X≈15.27–597.60. Both pass the track's X=48 left endpoint and X=568 right endpoint.
The [contact summary](data/frame-touch-range-2026-11-03/after-contacts.json)
records per-contact extents. This is physical input evidence, not injected input.

The [accepted receipt](data/frame-touch-range-2026-11-03/accepted.json) verifies:

- Config SHA-256:
  `723e0773d6b197d505ed58bc4310cf0af3dd6478a335bc0b9599a6aa0308825a`.
- Paired udev rule SHA-256:
  `a8f9fc76ad8c66fe9a2c4d785846b04d3660d9871ed5a49fb3aa41fa1d6be3fa`.
- Original controller firmware SHA-256 remains
  `990532d949b2bf3bbc7d00c30cb4f8e49ed6202d51ae8456bd2076353d635ae9`.
- Native, authority and tunnel services are active; native touch is calibrated,
  input is released, and state is confirmed.

Only the touch config and paired rule were installed. The existing I2C driver
`moonka-touch` device `2-0040` was unbound/rebound while the native display was
stopped, then the existing display service resumed. No reboot or new service was
needed. App state, protocol v21 and the existing signed release
`802dba22acd4be013d2d21109bcab12f86368b1b59644610391bc429f8583831`
remain in place. No new app release was built or activated for this correction.

PhotoFrame commit `23bf648` on `fix/touch-coordinate-range` preserves the
correction in its shared image-builder path, including the paired rootfs rule.
The archived vendor header stays unchanged; stock input and corrected output
digests are verified around the manifest's explicit host-word overrides.

## Focused validation

- `cargo test --locked --offline -p otrios-core --features canvas --test oib
  slider_captures_reach_both_endpoints_even_outside_the_track`: passed; volume and
  brightness reach both endpoints, including movement outside the track, in
  landscape and portrait layouts.
- `cargo test --locked --offline -p otrios-dashboard --features direct --example
  touch_calibration`: 2 tests passed, covering affine fitting/degenerate input
  and held-contact sampling.
- `cargo clippy --locked --offline -p otrios-dashboard --features direct
  --example touch_calibration --tests -- -D warnings`: passed.
- ARMv7 release build of the temporary `touch_calibration` example: passed and
  exercised on Frame. The example measures only; its caller owns config changes.
- PhotoFrame `python3 tools/test_touch_firmware.py`: 4 tests passed, including
  exact stock firmware, only four host-word differences, and override hash guards.
- PhotoFrame `tools/probe_touch_mapping.c`: actual vendor `ScreenResolution()`
  evaluated at 465,801 interior sensor pairs; maximum converted-vs-original
  mapping difference 2.718 logical pixels from integer quantization. The same
  probe verifies continued movement through the formerly saturated region.
- All three PhotoFrame firmware/config output digests and the OTriOS offline
  generator's equality with the installed pair: passed.

Broad repository checks, a complete PhotoFrame image build, reboot acceptance,
browser qualification and Linux IPC acceptance were not repeated for this
configuration-only hardware correction. The two failed attended fits were
rejected measurements, not acceptance passes.

## Rollback

Frame retains the exact original pair and a hash-guarded rollback script under
`/var/lib/otrios-dashboard/touch-mapping-2026-11-03/`. To restore both together:

```sh
ssh frame python3 /var/lib/otrios-dashboard/touch-mapping-2026-11-03/rollback.py
```

The script stops native rendering, restores the original config and rule,
rebinds the same touch device and resumes the existing display service. Both
failed calibration attempts exercised restoration of that original pair. The
saved final rollback script was syntax checked and transferred with a verified
hash; it was not executed after physical acceptance. Temporary calibration
executables/output files were removed after local evidence was retained.
