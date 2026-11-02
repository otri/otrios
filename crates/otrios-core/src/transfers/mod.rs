// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded portable containers. Decoding never grants trust, access or installation.
pub mod archive;
mod collection;
mod data;
mod export;
mod http;
mod sharing;
mod source;
mod staging;
pub use collection::{OwnedRoot, QualifiedCollection, RootPolicy, encode_collection};
pub use data::{InspectedData, encode_data, inspect_data};
pub use export::{PinnedCollection, PinnedRoot, authorize_dataset};
pub use http::{AppResponse, TransferHttpResponse, read_app_response, read_transfer_response};
pub use otrios_interfaces::transfers::*;
pub use sharing::{AppImportTarget, authorize_share};
pub use source::{AppSource, public_address};
pub use staging::{DataCandidate, ReplacementTarget, StagedReplacement};
type Result<T> = std::result::Result<T, String>;

/// Explicit host installation/publication rights, independent of Space roles.
#[derive(Default)]
pub struct HostPolicy(std::collections::BTreeSet<String>);
impl HostPolicy {
    pub fn new(identities: Vec<String>) -> Result<Self> {
        if identities.len() > 64 {
            return Err("Host operator budget".into());
        }
        let mut entries = std::collections::BTreeSet::new();
        for identity in identities {
            if !crate::spaces::valid_identity(&identity) || !entries.insert(identity) {
                return Err("Invalid/duplicate host operator".into());
            }
        }
        Ok(Self(entries))
    }
    pub fn allows(&self, identity: &str) -> bool {
        self.0.contains(identity)
    }
}

#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
mod publication;
#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
pub use publication::{PublicationStore, PublishedApp};
#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
mod import;
#[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
pub use import::{AppCandidate, StagedApp};

/// Current host admission captured under the authority's management lock.
pub struct InstallAuthority<'a> {
    pub host: &'a HostPolicy,
    pub catalog: &'a [u8],
    pub release: &'a str,
    pub now: u64,
}
