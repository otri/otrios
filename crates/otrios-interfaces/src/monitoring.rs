// SPDX-License-Identifier: Apache-2.0
//! Bounded single-producer Monitoring app data; identity/grants remain server policy.
use serde::{Deserialize, Serialize};
pub const CHANNEL: u8 = 6;
pub const FORMAT: &str = "otrios-monitoring-data-v1";
pub const MAX_SNAPSHOT_BYTES: usize = 128 * 1024;
pub const MAX_PACKET_BYTES: usize = 256 * 1024 + 1024;
pub const METRICS: [&str; 11] = [
    "prod_health",
    "rc_health",
    "staging_health",
    "wordpress_health",
    "bundles",
    "memory",
    "workers",
    "posts_en",
    "posts_ja",
    "backup_days",
    "database",
];
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Window {
    pub generated: i64,
    pub start: i64,
    pub end: i64,
    pub step: u32,
    pub collected_label: String,
    pub start_label: String,
    pub end_label: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metric {
    pub current: Option<f64>,
    /// Exactly 169 inclusive hourly slots; missing samples stay missing.
    pub history: Vec<Option<f64>>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Data {
    pub window: Window,
    /// Fixed METRICS order, not client-selected PromQL or a dynamic field schema.
    pub metrics: Vec<Metric>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Head {
    pub format: String,
    pub feed: String,
    pub producer: String,
    pub revision: u64,
    pub window: Window,
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Command {
    Offer {
        feed: String,
        root: Vec<u8>,
    },
    Transfer {
        feed: String,
        root: Vec<u8>,
        base: Option<Vec<u8>>,
        bytes: Vec<u8>,
    },
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Event {
    Need { base: Option<Vec<u8>> },
    Ack { root: Vec<u8>, revision: u64 },
    Rejected,
}

impl Data {
    fn metric(&self, key: &str) -> Result<&Metric, String> {
        METRICS
            .iter()
            .position(|k| *k == key)
            .and_then(|i| self.metrics.get(i))
            .ok_or_else(|| "Unknown Monitoring metric".into())
    }
    pub fn current(&self, key: &str) -> Result<Option<f64>, String> {
        Ok(self.metric(key)?.current)
    }
    pub fn history(&self, key: &str) -> Result<Vec<Option<f64>>, String> {
        Ok(self.metric(key)?.history.clone())
    }
}
