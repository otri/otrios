<!-- SPDX-License-Identifier: Apache-2.0 -->
# Five-second ghost trails

Touch Pad drawings are local presentation state plus a separate `/demo/ink`
WebSocket subscription. They never enter the Automerge document, Monitoring
stream, snapshot files or browser document cache. Only the final pad position
is published as lasting app intent on release. Arrow keys also draw short
segments for keyboard access.

Each connection gets a server-assigned anonymous ghost identity and colour.
Down/up boundaries keep letters separate. A stroke fades over five seconds
after its latest point. Replayed points carry server age, so joining does not
restart their lifetime. Clients retain at most 24 strokes with 192 points each;
the server retains at most 1,024 events, expires idle history, permits 16 ink
connections and limits each to 64 points per second. Slow readers reconnect
instead of building an unbounded queue. Coordinates are typed 0..1000 integers
in the versioned binary contract in `otrios-interfaces::ink`.

The relay never reads app replicas, private presentations or credentials. The
public listener requires an allowed browser Origin; native clients use the
trusted listener. The module proxy exposes this one extra demo route. All
authorization and private-stream boundaries remain in force.

Tests cover two real public/native WebSocket clients, assigned identities,
replay, expiry, rejected Origin, unchanged app storage/private view, separate
letters, fade age, memory limits and final-position-only CRDT publication.
