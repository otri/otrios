// SPDX-License-Identifier: Apache-2.0
// Allocation-free Embedded Swift kernels. Inputs are validated by the harness.
@_cdecl("kernel")
public func kernel(_ seed: UInt32, _ count: UInt32) -> UInt32 {
    var x = seed
    for _ in 0..<count {
        x = (x &* 1664525) &+ 1013904223
        x ^= x >> 13
    }
    return x
}

@_cdecl("scan")
public func scan(_ data: UnsafePointer<UInt32>, _ count: UInt32, _ rounds: UInt32) -> UInt32 {
    var value: UInt32 = 2166136261
    for round in 0..<rounds {
        for index in 0..<count {
            value = (value ^ data[Int(index)] ^ round) &* 16777619
        }
    }
    return value
}

@_cdecl("noop")
public func noop(_ value: UInt32) -> UInt32 { value }
