// SPDX-License-Identifier: LGPL-3.0-only
//! Explicit device ownership, distinct from remote viewers' revocable RAM leases.
use super::*;
#[derive(Clone)]
pub(super) struct Owned {
    ui: Vec<u8>,
    snapshot: super::super::monitoring::Snapshot,
}
#[derive(Clone, Debug, serde::Serialize)]
pub struct Usage {
    pub root: String,
    pub revision: u64,
    pub generated: i64,
    pub local: bool,
}
impl Client {
    /// This view is confirmed against the durable local producer, independently
    /// of the remote authority connection. Ordinary remote leases stay online-only.
    pub fn owns_monitoring_view(&self) -> bool {
        self.local
            && self.owned_monitoring.is_some()
            && matches!(self.presentation(), Some(Presentation::Monitoring { .. }))
    }
    /// Only the configured native owner calls this. UI still binds to the signed
    /// installed catalog; a remote viewing grant never calls this entry point.
    pub fn own_monitoring(&mut self, ui: &[u8], bytes: &[u8]) -> Result<bool> {
        let snapshot = super::super::monitoring::Snapshot::from_bytes(bytes)?;
        let description = decode_ui(ui)?;
        if description.ui_ref != "app:monitoring/ui"
            || !matches!(
                description.presentation,
                Some(Presentation::Monitoring { .. })
            )
            || description.behavior.is_some()
            || !self.catalog.as_ref().is_some_and(|c| {
                c.apps
                    .iter()
                    .any(|e| e.app.id == "monitoring" && e.ui_hash == hash(ui))
            })
        {
            return Err("Local Monitoring UI differs from signed installed catalog".into());
        }
        if let Some(old) = &self.owned_monitoring {
            if old.snapshot.head().feed != snapshot.head().feed
                || old.snapshot.head().producer != snapshot.head().producer
                || old.snapshot.head().revision > snapshot.head().revision
                || old.snapshot.head().revision == snapshot.head().revision
                    && old.snapshot.root() != snapshot.root()
            {
                return Err("Local Monitoring ownership/revision changed".into());
            }
        }
        let changed = self
            .owned_monitoring
            .as_ref()
            .is_none_or(|o| o.snapshot.root() != snapshot.root() || o.ui != ui);
        self.owned_monitoring = Some(Owned {
            ui: ui.into(),
            snapshot,
        });
        if changed
            && self.local
            && let Some(m) = &mut self.active
            && matches!(m.ui.presentation, Some(Presentation::Monitoring { .. }))
        {
            m.install_view(bytes)?;
            m.invalidate();
            return Ok(true);
        }
        Ok(false)
    }
    /// Poll independently of network readiness, including cold offline startup.
    pub fn poll_owned_monitoring(&mut self) -> Result<bool> {
        if self.input_busy()
            || self.local_uninstall
            || !self.local_apps.reference("app:monitoring/ui")
        {
            return Ok(false);
        }
        let Some(owned) = self.owned_monitoring.clone() else {
            return Ok(false);
        };
        let requested = self
            .navigation
            .as_ref()
            .is_some_and(|p| p.0 == "app:monitoring/ui")
            || self
                .requested
                .as_ref()
                .is_some_and(|r| r.pair.0 == "app:monitoring/ui")
            || !self.local
                && self
                    .active
                    .as_ref()
                    .is_some_and(|m| m.ui.ui_ref == "app:monitoring/ui");
        if !requested {
            return Ok(false);
        }
        if self
            .active
            .as_ref()
            .is_some_and(|m| m.doc.pending_count() != 0)
        {
            return Ok(false);
        }
        if !self.local
            && let Some(m) = &self.active
        {
            self.host_wire
                .push_back(ClientMessage::Leave { epoch: m.epoch });
        }
        self.navigation = None;
        self.requested = None;
        self.decision = None;
        self.host_requested = false;
        let scope = self
            .catalog
            .as_ref()
            .and_then(|c| c.apps.iter().find(|e| e.app.id == "monitoring"))
            .ok_or("Monitoring not installed")?
            .app
            .state_ref
            .clone();
        self.mount_local(&owned.ui, &scope, &[], &[])?;
        let m = self
            .active
            .as_mut()
            .ok_or("Missing local Monitoring activation")?;
        m.install_view(&owned.snapshot.bytes()?)?;
        m.invalidate();
        Ok(true)
    }
    pub fn monitoring_ui(&self) -> Result<Option<Vec<u8>>> {
        self.active
            .as_ref()
            .filter(|m| matches!(m.ui.presentation, Some(Presentation::Monitoring { .. })))
            .map(|m| encode_ui(&m.ui))
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::monitoring::{self as data, Data, Metric, Snapshot, Window};
    #[test]
    fn locally_owned_monitoring_opens_updates_and_restarts_without_server() {
        let description = Ui {
            format: "otrios-canvas-v2".into(),
            ui_ref: "app:monitoring/ui".into(),
            title: "Monitoring".into(),
            state_schema: StateSchema {
                id: "monitoring-state-v2".into(),
                fields: vec![],
            },
            layout: data::empty_projection().unwrap(),
            selectors: vec![],
            handlers: vec![],
            navigation: vec![],
            behavior: None,
            presentation: Some(Presentation::Monitoring {
                frame: [24., 112., 1232., 680.],
            }),
        };
        let ui = encode_ui(&description).unwrap();
        let catalog = Catalog {
            format: "otrios-app-catalog-v1".into(),
            apps: vec![CatalogEntry {
                app: App {
                    id: "monitoring".into(),
                    title: "Monitoring".into(),
                    ui_ref: description.ui_ref.clone(),
                    state_ref: "app:monitoring/state".into(),
                    status: "canvas-app".into(),
                    dependencies: vec![],
                },
                path: String::new(),
                ui_hash: hash(&ui),
                private: true,
            }],
        };
        let mut d = Data {
            window: Window {
                generated: 604800,
                start: 0,
                end: 604800,
                step: 3600,
                collected_label: "Jan 08 00:00 UTC".into(),
                start_label: "Jan 01 00:00 UTC".into(),
                end_label: "Jan 08 00:00 UTC".into(),
            },
            metrics: data::METRICS
                .iter()
                .map(|_| Metric {
                    current: None,
                    history: vec![None; 169],
                })
                .collect(),
        };
        let first = Snapshot::collect(
            None,
            "11111111111111111111111111111111",
            "collector",
            d.clone(),
        )
        .unwrap();
        let mut client = Client::default();
        client.managed_transport = true;
        client
            .admit_catalog(&encode_catalog(&catalog).unwrap())
            .unwrap();
        client.own_monitoring(&ui, &first.bytes().unwrap()).unwrap();
        client.open("app:monitoring/ui", "app:monitoring/state");
        assert!(client.poll_owned_monitoring().unwrap());
        assert!(client.owns_monitoring_view());
        assert!(client.confirmed());
        assert_eq!(
            client.resource_usage().monitoring.unwrap().root,
            hex(&first.root())
        );
        client.reset_connection();
        assert!(client.owns_monitoring_view());
        assert!(client.confirmed());
        assert_eq!(client.ui_ref(), "app:monitoring/ui");
        d.window.generated += 1;
        d.metrics[4].current = Some(2302.);
        let next = Snapshot::collect(Some(&first), &first.head().feed, "collector", d).unwrap();
        assert!(client.own_monitoring(&ui, &next.bytes().unwrap()).unwrap());
        assert_eq!(client.resource_usage().monitoring.unwrap().revision, 2);
        assert!(client.own_monitoring(&ui, &first.bytes().unwrap()).is_err());
        let mut wrong = description;
        wrong.title = "unadmitted".into();
        assert!(
            client
                .own_monitoring(&encode_ui(&wrong).unwrap(), &next.bytes().unwrap())
                .is_err()
        );
    }
}
