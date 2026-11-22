// SPDX-License-Identifier: Apache-2.0
//! Admission/HTTP-host glue only. Typed validation and Merkle replication live in core.
use super::*;
use otrios_core::canvas::monitoring::{self as data, Command, Event, Snapshot};
impl Hub {
    fn cache_monitoring(&mut self, feed: &str, snapshot: &Snapshot) -> Result<()> {
        if self.collection_views.len() >= 128
            && !self.collection_views.contains_key(feed)
            && let Some(key) = self.collection_views.keys().next().cloned()
        {
            self.collection_views.remove(&key);
        }
        self.collection_views
            .insert(feed.into(), (snapshot.root().to_vec(), snapshot.bytes()?));
        Ok(())
    }

    fn monitoring_grant(&self, identity: &str, feed: &str) -> Result<()> {
        if self.monitoring_failed {
            return Err("Monitoring persistence requires restart".into());
        }
        let grant = self.collector.as_ref().ok_or("Collector unavailable")?;
        if grant.binding.collection != feed
            || !self
                .bundles
                .get(&self.active)
                .is_some_and(|b| b.packages.values().any(|p| p.monitoring))
        {
            return Err("Wrong Monitoring collection/contract".into());
        }
        otrios_core::spaces::authorize_service_installed(
            self.registry().map_err(|e| e.to_string())?,
            identity,
            grant,
            &self.app_policy("monitoring")?,
        )
        .map_err(|e| e.to_string())
    }
    fn monitoring_path(&self, feed: &str) -> Result<PathBuf> {
        if !otrios_core::spaces::valid_id(feed) {
            return Err("Invalid Monitoring collection".into());
        }
        Ok(self
            .state
            .as_ref()
            .ok_or("Durable Monitoring storage required")?
            .join(format!("collection-{feed}.data"))
            .join("monitoring.merkle"))
    }
    fn current_monitoring(&self, feed: &str) -> Result<Option<Snapshot>> {
        let path = self.monitoring_path(&feed)?;
        if !path.exists() {
            return Ok(None);
        }
        let snapshot = data::store::load(&path)?;
        if snapshot.head().feed != feed {
            return Err("Wrong stored Monitoring collection".into());
        }
        Ok(Some(snapshot))
    }
    pub(super) fn load_monitoring_data(&mut self, feed: &str) -> Result<()> {
        if let Some(snapshot) = self.current_monitoring(&feed)? {
            self.cache_monitoring(feed, &snapshot)?;
        } else {
            self.collection_views.remove(feed);
        }
        Ok(())
    }
    pub(crate) fn monitoring_publication(
        &mut self,
        identity: &str,
        command: Command,
    ) -> Result<Event> {
        let feed = match &command {
            Command::Offer { feed, .. } | Command::Transfer { feed, .. } => feed.clone(),
        };
        self.monitoring_grant(identity, &feed)?;
        let current = self.current_monitoring(&feed)?;
        match command {
            Command::Offer { root, .. } => {
                if root.len() != 32 {
                    return Err("Invalid Monitoring root".into());
                }
                if let Some(current) = current.filter(|p| p.root().as_slice() == root) {
                    Ok(Event::Ack {
                        root,
                        revision: current.head().revision,
                    })
                } else {
                    Ok(Event::Need {
                        base: self.current_monitoring(&feed)?.map(|p| p.root().to_vec()),
                    })
                }
            }
            Command::Transfer {
                root, base, bytes, ..
            } => {
                let next = Snapshot::receive(
                    current.as_ref(),
                    base.as_deref(),
                    &bytes,
                    &root,
                    &feed,
                    identity,
                )?;
                if current.as_ref().is_none_or(|p| p.root() != next.root()) {
                    let path = self.monitoring_path(&feed)?;
                    self.collection_parent(&path)?;
                    if let Err(e) = persist(&path, &next.bytes()?) {
                        self.monitoring_failed = true;
                        return Err(e.to_string());
                    }
                }
                self.cache_monitoring(&feed, &next)?;
                Ok(Event::Ack {
                    root,
                    revision: next.head().revision,
                })
            }
        }
    }
}
