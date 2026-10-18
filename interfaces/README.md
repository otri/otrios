# OTriOS interface design requirements

The public interface is Apache-2.0 so independent clients and accelerator implementations can share contracts. There is no generated IDL or stable wire format in this scaffold.

Before choosing a FlatBuffers-like format, write an RFC and a small codec spike covering:

1. Versioned object envelopes with canonical hashing, endianness, bounds checks, and unknown-field behavior.
2. Interface definitions for stage, publish, acquire snapshot, subscribe, advance cursor, and resynchronize.
3. Capability requirements and authority-side enforcement, including handle ownership and revocation semantics.
4. Shared-memory offsets, alignment, lifetime, maximum message sizes, and compatibility across processes and architectures.
5. Golden byte fixtures and conformance tests usable without linking the LGPL runtime.

Generated SDK and schema code stays permissively licensed only when generator templates and dependencies permit it. The future runtime must validate all inputs independently of generated client validation. Defer the accelerator protocol until a reference CRDT and its semantics exist.
