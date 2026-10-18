// SPDX-License-Identifier: Apache-2.0
// Native-only ownership experiment; no Swift object layout crosses the ABI.
final class Payload {
    let value: UInt32
    init(_ value: UInt32) { self.value = value }
}

@_cdecl("handle_create")
public func handleCreate(_ value: UInt32) -> UnsafeMutableRawPointer {
    Unmanaged.passRetained(Payload(value)).toOpaque()
}

@_cdecl("handle_retain")
public func handleRetain(_ pointer: UnsafeMutableRawPointer) -> UnsafeMutableRawPointer {
    _ = Unmanaged<Payload>.fromOpaque(pointer).retain()
    return pointer
}

@_cdecl("handle_release")
public func handleRelease(_ pointer: UnsafeMutableRawPointer) {
    Unmanaged<Payload>.fromOpaque(pointer).release()
}

@_cdecl("handle_value")
public func handleValue(_ pointer: UnsafeMutableRawPointer) -> UInt32 {
    Unmanaged<Payload>.fromOpaque(pointer).takeUnretainedValue().value
}
