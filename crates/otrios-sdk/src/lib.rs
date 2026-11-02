// SPDX-License-Identifier: Apache-2.0
//! SDK contract exports. A client transport will be added with the Linux PoC.
//! This crate deliberately has no dependency on the covered runtime implementation.
#[cfg(feature = "oib")]
pub use otrios_interfaces::oib;
pub use otrios_interfaces::{ContentId, Generation, ObjectId, RevisionRef, SubscriptionEvent};
