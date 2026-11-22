<!-- SPDX-License-Identifier: Apache-2.0 -->
# Home and Space administration — 2026-11-22

Home now starts directly with its Space cards. Space titles and card backgrounds
have no tap action outside arrangement. Sign in opens the login panel directly;
the signed-in Profile avatar opens account settings directly.

Admins have a three-line hamburger beside Profile with Add Space, Review Deleted,
and Appearance (System/Light/Dark). Each Space they administer has an Edit pencil
opening a labelled editing panel. Arrange apps enables arrangement for that Space
only; its check icon finishes editing. Empty administered Spaces remain reachable.
Members and visitors do not get these management controls. Current discovery
revokes the menu and arrangement when Admin access disappears. Designer leave
confirmation also applies when opening the menu's management destinations.

Arrangement saves a pending Space name and resets panel scrolling. A short-screen
regression caught an inherited scroll offset that could hide the Name field in
later app settings; that leak is fixed. New Edit/Done labels use the existing
bounded UTF-8 truncation so maximum-length Space names remain renderable.

## Validation and deployment

Implementation commit: `4cd4291`.

- `cargo test --locked -p otrios-core --features canvas --test spaces_ui`:
  47 passed. Existing management, transfer, drag, scrolling and phone mounting
  journeys were adapted to the explicit per-Space editing flow. The role/menu
  regression covers direct sign-in/Profile, inert title taps, local theme changes,
  per-Space isolation, role revocation and 128-byte Space names.
- `cargo test --locked -p otrios-core --features canvas --lib
  shell_appearance_and_account_preserve_invalid_draft_and_preview_mode`:
  1 passed; 590 filtered out. The menu preserves the invalid Designer draft and
  Profile still uses the leave guard.
- Built Mac packer/checker, release browser Wasm, Linux aarch64 authority/checker,
  and ARMv7 direct host using Rust 1.90 and the existing lockfile. The browser build
  retains three existing unused-code warnings. No broad preflight suite was run.

Signed release:
`163c71272e5c050cff3ebd1a0e4f6e81e218de26c89b4d57addb0e1645105003`.

Full signature/digest and nested bundle verification passed on Mac and Linux
before locked atomic activation on the existing OrbStack/Web host. All 85
independent authority files, app contracts and catalog bytes are unchanged from
`27ea8edf0466114c6e571d8b2fee28199078658cdd21b3546544eb72787f6bb9`.
That signed release and its compatible authority executable remain available for
rollback. The restored Mac publisher pointer matches the active release.

The real public browser automatically loaded the new signed release at a
390 × 720 CSS viewport. Home exposes its three app tiles and Sign in, without a
Space-title button or admin controls. Sign in directly opens the email/password
login panel without an intermediate Visitor/Appearance popup; cancelling returns
to Home. The Brass Relay opens from its tile with the shared Home breadcrumb,
composer and 390 × 720 canvas, then returns to Home without an application error.
Authority, tunnel and Monitoring timer remain active. The disposable Docker
builder is stopped after the completed builds.

Admin behavior is covered by authenticated-role fixtures; the live admin login
was not repeated. These browser checks use a desktop engine at phone dimensions,
not physical iPhone Safari. Logs and acceptance metadata are retained under
`~/Library/Application Support/OTriOSOrbStack/`.
