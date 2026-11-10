<!-- SPDX-License-Identifier: Apache-2.0 -->
# OIB component behavior ABI 1

This freezes the bounded wire contract selected by
[RFC 0036 L2](rfcs/0036-upgradable-oib-component-libraries.md#l2-component-behavior-boundary).
The feature identifier is `otrios-component-behavior-v1`. It is independent of
Canvas app behavior ABI 1. Public record types and the canonical codec live in
`otrios_interfaces::oib::component_behavior`; execution and admission belong in
the LGPL runtime. Well-formed bytes, a content hash and a publisher signature do
not grant app writes or input authority.

## Wasm exports and call sequence

A component module exports:

| Name | Type | Meaning |
| --- | --- | --- |
| `memory` | Wasm memory | One bounded 64 KiB memory; record buffers are ranges within it |
| `abi_version` | `() -> i32` | Returns `1` |
| `input_buffer` | `() -> i32` | Returns the byte offset of an 8192-byte input window |
| `dispatch` | `(i32, i32) -> i64` | Receives input offset/length and returns packed output offset/length |

The return value is interpreted as an unsigned 64-bit bit pattern: the low
32 bits are the output offset and the high 32 bits are its byte length. Thus
`result = (length << 32) | offset`; negative signed `i64` representations do not
change the bit order. The host admits the complete 8192-byte input window at load time and bounds-checks
both actual record ranges against exported memory with checked arithmetic. Each input/output is at most **8192
bytes**, and output decoding must consume its exact length.

The host writes a complete canonical input, invokes `dispatch`, copies the
bounded result out of guest memory, then validates the complete command set
against the importing app's admitted contracts before applying any effect.
Buffers may overlap: output is read only after dispatch returns. Guest memory
is scratch; declared host-owned local properties are resumable component state.

The L2 runtime admits at most 64 KiB module bytes, no imports/WASI or tables,
one 64 KiB memory, recursion depth 32, stack height 1024 and one cumulative 10,000-fuel budget covering start,
`abi_version`, `input_buffer` and `dispatch` in each fresh-store invocation. Mutable stores are occurrence-local. Whole-app live and
staged-store budgets, replacement barriers, traps and fuel exhaustion are runtime
responsibilities, not permissions inferred from this byte codec.

## Input record

All integers and float bit patterns are little-endian. There is no alignment
padding beyond the explicit zero bytes below. The fixed header is **48 bytes**.

| Offset | Width | Field |
| ---: | ---: | --- |
| 0 | 8 | Literal bytes `OTCBIN1\0` (`4f 54 43 42 49 4e 31 00`) |
| 8 | 1 | Phase: Down `0`, Move `1`, Up `2`, Cancel `3`, Activate `4` |
| 9 | 1 | Reserved; zero |
| 10 | 2 | Normalized local input `a`, inclusive 0–1000 |
| 12 | 2 | Normalized local input `b`, inclusive 0–1000 |
| 14 | 2 | Effective property count, at most 64 |
| 16 | 2 | Host-owned local property count, at most 64 |
| 18 | 1 | Semantic reference-path count, at most 16 |
| 19 | 1 | Reserved; zero |
| 20 | 4 | Host pointer identity, scoped to the current input target |
| 24 | 8 | Stable outer instance identity; nonzero |
| 32 | 8 | Stable definition-node identity; nonzero |
| 40 | 8 | Stable signed behavior-entry identity within its library; nonzero |

The behavior identity distinguishes entries in a shared module even when
component definitions reuse node identities. It is not a renderer control or app
field number. The host selects the entry from the verified attachment, never
from an untrusted event.

After the header come, in order:

1. Reference-path entries: that many nonzero `u64` node identities, outside-in.
2. Effective exposed property fields.
3. Host-owned local property fields.

The header, reference path, effective properties and local properties share one
8192-byte envelope. Individual field/count limits do not provide separate byte
budgets. Candidate admission checks current effective properties with declared
local defaults; a staged Client also checks its current properties together with
resumed host locals before activation. Each local-property result must leave the
complete next context within this same bound before any effect is applied.

Each field is a nonzero `u64` declared property identity followed by one Value.
Each field list must be strictly ordered by identity, with no duplicates. The two
lists are distinct scopes; an identity can occur once in each. The complete
semantic occurrence address is the existing `ObjectAddress` tuple of outer
instance, reference path and final node. Property names, renderer IDs and package
revision hashes do not replace that address.

Pointer Down/Move/Up/Cancel records carry normalized local x/y in `a`/`b`,
using the host's admitted capture/local-coordinate rules. Activate carries the
normalized activation payload (for example an accessibility value), rather than
a pointer position. The codec checks only the inclusive numeric domain; input ownership, phase,
current generation and handler validation remain host-owned.

## Value record

Each Value begins with `tag:u8`, one reserved zero byte, and `payload_length:u16`.
The payload follows immediately. Only these exact tag/length forms are valid:

| Tag | Type | Payload |
| ---: | --- | --- |
| 1 | Number | Exactly 4 bytes: finite IEEE-754 binary32 bits |
| 2 | Boolean | Exactly 1 byte: `0` or `1` |
| 3 | Text | 0–4096 bytes of valid UTF-8, with no NUL byte |
| 4 | Color | Exactly 4 bytes: red, green, blue, alpha |
| 5 | Series | 0–2016 consecutive binary32 values; length is a multiple of 4 |
| 6 | TimedSeries | 0–2016 pairs of consecutive binary32 values; length is a multiple of 8 |

All Number/Series/TimedSeries scalars must be finite and must not encode negative
zero (`0x80000000`). Positive zero is the only zero representation. There is no
Unicode normalization step; the UTF-8 bytes are preserved. An aggregate 8192-byte
record limit can admit fewer than the per-Value maxima. Domain constraints such
as a property's range or a TimedSeries time/value interval are checked against
its admitted property/outlet contract by the runtime.

## Output record

The fixed header is **16 bytes**:

| Offset | Width | Field |
| ---: | ---: | --- |
| 0 | 8 | Literal bytes `OTCBOUT1` (`4f 54 43 42 4f 55 54 31`) |
| 8 | 2 | Command count, at most 16 |
| 10 | 6 | Reserved; all zero |

Each command starts with a **16-byte command header**: `tag:u8`, seven reserved
zero bytes, then `target:u64`. The payload, when present, is one Value.

| Tag | Command | Target and payload |
| ---: | --- | --- |
| 1 | SetLocal | Nonzero declared local property identity, followed by Value |
| 2 | Emit | Nonzero declared typed outlet identity, followed by Value |
| 3 | Capture | Zero target, no payload; current admitted occurrence/input target only |
| 4 | ReleaseCapture | Zero target, no payload; current admitted occurrence/input target only |

Commands retain their vector order. Each local property can be assigned at most
once and each outlet can emit at most once in a result. At most one Capture or
ReleaseCapture command is allowed across the complete result. The two identity
scopes are separate, so SetLocal and Emit can use the same numeric identity.
An empty command result is valid.

The guest cannot name an app-state field, another occurrence or an arbitrary
capture target. The host resolves outlets through the app's admitted binding.
An undeclared target, wrong type/range, disallowed capture, malformed record,
trap or exhausted resource budget rejects the complete result before any local,
app-state or capture effect is applied. No partial-result semantics are provided.

## Canonical vectors

Whitespace separates hexadecimal groups for readability; it is not part of the
bytes. These are complete records, including every reserved byte. They are
normative codec vectors, not signature, Wasmi or live-deployment qualification.

The input is behavior `11`, occurrence `(instance 20, references [2], node 3)`,
Move pointer `7`, local coordinates `(750, 250)`, effective Number property
`1 = 400`, and local Boolean property `2 = true`. Its exact length is **85 bytes**:

```text
4f544342494e3100 0100ee02fa00010001000100 07000000
1400000000000000 0300000000000000 0b00000000000000 0200000000000000
0100000000000000 01000400 0000c843
0200000000000000 02000100 01
```

The output sets local property `2 = true`, emits Number `750` on outlet `9`, and
requests capture. Its exact length is **77 bytes**:

```text
4f5443424f555431 0300000000000000
0100000000000000 0200000000000000 02000100 01
0200000000000000 0900000000000000 01000400 00803b44
0300000000000000 0000000000000000
```

The empty output is **16 bytes**:

```text
4f5443424f555431 0000000000000000
```

Reject every strict prefix of a valid record, any trailing byte, nonzero reserved
byte, unsupported tag/phase, invalid payload length, duplicate/unordered input
field, duplicate command target in its scope, zero semantic/declared identity,
negative zero/nonfinite float, invalid UTF-8/Boolean, or an over-budget count or
record. A decoder must not repair, reorder or silently ignore an invalid form.
