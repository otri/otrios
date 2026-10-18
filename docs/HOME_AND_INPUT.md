# Home Screen and shared input

The per-device shell launches three current demos: **Monitoring**, **Live Controls** and **Input Lab**. A persistent top bar provides Back and Home. All visible UI, including launcher cards, navigation, text fields and keyboard keys, uses the shared primitive compositor on both native GLES2 and browser WebGL.

## Navigation and composition

Navigation lives in `otrios-core::dashboard::shell::Shell`, separately from `Replica`. A shared state or metrics update never changes another device's selected screen. Monitoring displays the latest replaceable presentation; Live Controls displays the saved editable layout and shared CRDT state. Both remain available together.

Back first hides an open keyboard without blurring its field. Back again returns to Home; on other demos it returns directly to Home. Home goes straight to the launcher. Native Escape follows Back rather than terminating the kiosk. Home's redundant navigation buttons are disabled in browser semantics. Browser hash routes support direct entry and ordinary Back/Forward, including the keyboard's visible layer; an app Back to the preceding route pops history instead of creating another entry. Each browser tab owns its route.

The native host stores only its selected screen in a small sidecar beside the CRDT state, so code reload resumes the screen. The browser retains its route in the URL across code reload. Input Lab's demonstration text is deliberately memory-only: it survives switching demos in the same process, but is reset by a code/page/process restart. No typed text or selection is inserted into the CRDT, sent to the server or written to local persistence by this demo.

The shell reserves an 80-unit navigation bar. Existing 1280×800 app views fit beneath it in a centered 0.9-scale `Viewport`, a clipped container primitive. Painting scales positions, text pixels, radii and stroke widths; hit testing, responder-local coordinates and browser semantic bounds use the same scale. Views remain legible without covering a graph or changing the original binary layouts. A composed immutable Scene is cached by application snapshot and local UI revision. Existing `snapshot`/`layout` IDs still identify the shared application input, not a hash of a device's local route or typed text; hosts report the local screen separately.

Home, navigation, editor and keyboard templates/instances are committed FlexBuffers assets under `crates/otrios-core/src/dashboard/shell-assets/`. Runtime reads those assets. `dashboard-shell-assets` is an Apache-licensed authoring fixture that regenerates them from typed Rust contracts; a fixture test checks exact binary equality. The shell supplies bounded typed `Binding::Local(u16)` values for local labels, key labels/colours and field display. The public responder contract contains explicit navigation, focused-field and keyboard-key actions; there is no arbitrary callback or executable expression payload. The keyboard template receives its key ID as a validated numeric parameter. Key/card children bubble through their enclosing Canvas responder.

```sh
cargo run --locked -p otrios-dashboard --bin dashboard-shell-assets -- \
  crates/otrios-core/src/dashboard/shell-assets
```

The system assets ship with the signed code release. Editing the existing `/layout` register changes Live Controls, not the shell catalog. A live shell-catalog editor and arbitrary downloaded demo modules are future work. System node IDs occupy a reserved range above the current app-layout ID limit, so app actions and navigation controls cannot share a node identity. The existing scene count/depth bounds still apply to the composed scene.

## Core keyboard component

The Linux photo-frame project's accepted keyboard was inspected read-only: its README, event contract and pure C model informed this implementation. Its useful boundary is preserved: **the keyboard emits literal text or editing commands; the focused application owns the text, caret and selection**. The existing Wayland/Cairo/Pango keyboard, Weston configuration, kernel and browser profile remain unchanged.

The portable Rust component is `otrios-core::dashboard::keyboard::Keyboard`; output uses the Apache interface `otrios_interfaces::keyboard::Edit`. It has no Wayland, SDL, DOM, field-text or persistence dependency. `Shell` routes output synchronously to the focused demo field. Focus changes, screen changes, cancellation and window blur cancel pending contacts and repeat; there is no commit on unfocus. A future asynchronous/system input-method adapter must add its own context freshness and protocol acknowledgement boundary.

Implemented behaviour:

- English QWERTY, a numbers/symbols page and an additional symbols page covering printable ASCII punctuation, with explicit ABC return. Backspace, Tab and Enter keep their positions across pages.
- Shift cycles off → one-shot → locked → off. Accepted letters and Shift-Tab consume one-shot; symbols/digits do not. Lock survives field changes. No autocorrection, smart punctuation, artificial spaces or long-lived preedit buffer.
- One accepted touch at a time. Letters commit once on release; sliding to another letter changes the active key. Leaving the key area cancels the pending character. Canceled or stale contacts do not commit into a newly focused field.
- Backspace and left/right arrows act immediately and repeat after 450 ms at 125 ms intervals. A delayed tick emits at most one edit, not a catch-up burst. Sliding onto/from a repeating editing key cancels the gesture.
- Hide changes the keyboard's presentation only; it does not blur, Tab or synthesize an unrelated click. Focusing either field reopens it. The two demo fields sit above the keyboard; there is no general scroll-to-caret/occlusion engine yet.
- Physical Backspace, left/right, selection extension, Select All, Tab, Enter and text are routed to the same app-side editor. Browser semantic buttons keep their native keyboard activation rather than having Space/Enter intercepted as field input.

The native Wayland host stops SDL's text-input activation and uses a small US-ASCII physical-key adapter while its own OSK is in use, so it does not open the compositor's separate input panel. Other SDL desktop backends can use SDL text events. Browser pointer capture and the existing native calibrated mouse/touch path still supply input; no global key injection, uinput device, keyboard grab, root input service or second Wayland input method was introduced.

`TextBuffer` is a separate bounded example application editor, not a keyboard composition buffer. It replaces selections, navigates/deletes at Unicode grapheme boundaries and caps each field at 256 UTF-8 bytes. It reuses the already locked/transitive `unicode-segmentation` 1.13.3 dependency explicitly rather than implementing ad hoc UTF-8 deletion. Display text is bounded and scrolls around the caret. The compositor's bitmap font remains primarily ASCII; correct buffer boundaries are not a claim of full script rendering. Input Lab uses ordinary visible fields, not a password implementation.

## Scope and validation

This is an OTriOS input component and demonstration, not a replacement system-wide Linux input method. IME/preedit/candidate UI, international physical-key layout handling on Wayland, password-field policy, rich selection visuals, general focus traversal and caret occlusion are deferred. There is no measured end-to-end keyboard latency or typing-loss guarantee yet. The existing native event loop is reused; the keyboard model does not introduce a render worker or claim the separate C keyboard's threading acceptance results.

Tests cover canceled/sliding/multiple contacts, repeat timing and focus cancellation, Shift/Shift-Tab/punctuation, physical ASCII shift mapping, grapheme selection/deletion and size limits, local navigation without CRDT edits, back/hide behaviour, embedded binary fixtures and shared viewport paint/hit coordinates. Real browser checks additionally exercise launcher navigation, independent fields and back navigation. Native GLES2 readiness establishes the installed host/render path. On 2026-10-19, the user confirmed the physical-frame Input Lab test: entering `abc` in the first field and `xyz` in the second, then using Backspace and Back. This is user-reported physical touch acceptance; it does not establish measured input latency.

Motion Lab and 3D Lab extend this catalog; see [presentation ownership](ANIMATION.md). State/network/storage now run on host workers, while the presentation owner retains its clock and GPU resources.
