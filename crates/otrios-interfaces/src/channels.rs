// SPDX-License-Identifier: Apache-2.0
//! Outer channel framing, independent of release-control serialization.
pub const STATE: u8 = 1;
pub const INK: u8 = 2;
pub fn frame(channel: u8, mut bytes: Vec<u8>) -> Vec<u8> {
    bytes.insert(0, channel);
    bytes
}
/// Managed auxiliary channels carry their admitted Canvas mount epoch. The
/// epoch references the socket's current activation; it is not an access grant.
pub fn scoped(epoch: u32, bytes: Vec<u8>) -> Vec<u8> {
    let mut framed = epoch.to_le_bytes().to_vec();
    framed.extend(bytes);
    framed
}
pub fn scope(bytes: &[u8]) -> Result<(u32, &[u8]), &'static str> {
    let prefix: [u8; 4] = bytes
        .get(..4)
        .ok_or("Missing auxiliary mount epoch")?
        .try_into()
        .map_err(|_| "Invalid auxiliary mount epoch")?;
    let epoch = u32::from_le_bytes(prefix);
    if epoch == 0 {
        return Err("Invalid auxiliary mount epoch");
    }
    Ok((epoch, &bytes[4..]))
}
