// SPDX-License-Identifier: Apache-2.0
//! Stable bootstrap control. Keep v1 readable when app/state protocols change.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Release {
    pub id: String,
    pub web_entry: String,
    pub sync_protocol: String,
}
impl Release {
    pub fn valid(&self) -> bool {
        valid_hash(&self.id)
            && self.web_entry == format!("/releases/{}/web/index.html", self.id)
            && valid_protocol(&self.sync_protocol)
    }
}
pub fn valid_hash(id: &str) -> bool {
    id.len() == 64
        && id
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
pub fn valid_protocol(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.bytes().all(|c| c.is_ascii_graphic())
}
