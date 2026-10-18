<!-- SPDX-License-Identifier: Apache-2.0 -->
# RFC 0005: Parent-local pins and springs/struts

Status: scoped Canvas prototype, requested by the user. Builds on RFC 0004.

Reusable controls carry a **design size** and parent-relative resizing rules.
An instance supplies its actual frame. The same bounded Rust resolver lays out
primitives for native rendering, browser rendering, semantics and hit testing.
UI description and state are inputs; computed geometry is local presentation
state and never a stream of CRDT writes.

## Rules

Each node can carry a `ResizeRule`: its parent's design size and one policy per
axis. A missing rule preserves existing frame semantics.

| Axis policy | Result |
| --- | --- |
| Fixed | Preserve the authored position and length |
| Leading pin | Preserve length; place at the specified leading gap |
| Trailing pin | Preserve length; place at parent length minus trailing gap and child length |
| Two pins | Stretch between the two specified gaps |
| Center | Preserve length; center in the current parent plus an optional offset |
| Springs | Distribute parent size change over selected leading margin, length and trailing margin |

A spring's share is proportional to that segment's **original nonnegative design
length**, not the last resolved frame. If all selected design segments have zero
weight, split the delta equally. Negative design margins have zero weight.
No selected springs means the frame stays fixed. Resizing never accumulates drift:
always start from authored geometry and the recorded design reference.

The horizontal and vertical axes are independent. A fixed-size centered Circle
stays square; stretching a Circle unequally is rejected by scene validation.
Pins and springs are alternative policies on an axis, not competing constraints.
The authoring builder rejects mixing them. Parent-local rules form a tree, so
there is no iterative solver, sibling dependency graph, priority ordering or
constraint cycle. Resolution is one bounded traversal.

## Authoring and reuse

```rust
component([460., 240.], vec![
    rounded_rect(CARD).pin_edges(0.),
    text("Reusable").pin_left(24.).pin_right(72.).pin_top(16.),
    circle(ACCENT)
        .frame([412., 16., 24., 24.])
        .pin_right(24.).pin_top(16.),
    canvas(body)
        .frame([24., 64., 412., 88.])
        .spring_width().spring_height(),
    button("Min width")
        .on_press(Event::press(width))
        .frame([24., 168., 412., 48.])
        .pin_left(24.).pin_right(24.).pin_bottom(24.),
])
```

The returned component can be placed at different frames. `component` retains its
design reference when an instance frame changes; ordinary canvases use their
authored frame as the reference. Child coordinate systems remain local to their
parent. Dynamic frame expressions are evaluated before descendant layout.
Bindings change geometry without rewriting the UI's design coordinates.

Layout Lab contains two calls to one app-local component definition, with
different width mappings and heights. Its shared width and corner-radius fields
resize both instances. Fixed status circles, flexible bodies, centered inner
circles and pinned buttons demonstrate the rules. Source-level component reuse
currently compiles into primitive trees; external component-resource loading and
automatic template factoring are separate work.

## Binary compatibility and failure policy

`otrios-layout-v2` adds an optional per-node resize descriptor to FlexBuffers.
Version 1 remains readable and cannot carry resize rules. Omitted descriptors
serialize exactly as before, verified against the checked-in default layout.
Old hosts reject v2; they must not silently render new rules as absolute frames.
The instance owns a template root's frame; resize descriptors belong below it.
The existing 1280×800 logical root canvas remains the surface contract for now.

Validate finite inputs, positive design/parent dimensions, bounded insets/offsets
and existing scene size/depth limits. A result with a nonpositive or oversized
length is an error, not an implicit clamp or a broken pin. Components must define
usable supported size ranges; this prototype does not negotiate intrinsic sizes,
minimum-size priorities, text wrapping, baseline alignment, sibling anchors,
breakpoints or aspect constraints. General resource reload failure semantics
remain those of RFC 0004.

## Acceptance

Tests cover pins, centers, proportional spring combinations, zero-length spring
weights, growth/shrink, every six-bit spring mask at the design size, malformed
rules and infeasible sizes. Integration checks round-trip binary rules, resolve
two reused instances, retain old snapshots, preserve node identity during state
resizing, and route pointer input against the newly resolved bounds. Confirm
legacy bytes remain identical and v1 cannot carry v2 rules. Verify the real server
process and browser viewer; distinguish those from physical frame acceptance.
