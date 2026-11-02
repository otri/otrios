// SPDX-License-Identifier: LGPL-3.0-only
//! A drop captures both scopes before asking for an explicit, scoped decision.
use super::*;
use crate::canvas::host_ui::{MoveDraft, Pane};
use otrios_interfaces::spaces::{Change, Discovery, MoveMode, Role};

impl Client {
    pub(super) fn begin_host_move(
        &mut self,
        source: String,
        app: String,
        collection: Option<String>,
        destination: Option<String>,
    ) -> Result<bool> {
        self.host.moving = None;
        self.host.message.clear();
        let snapshot = self.spaces.snapshot().ok_or("Spaces unavailable")?;
        let generation = snapshot.generation;
        let cursor = snapshot.cursor.clone();
        let catalog = snapshot.catalog.clone();
        self.host_destination(
            source,
            Pane::BeginMove {
                app,
                collection,
                destination,
                generation,
                cursor,
                catalog,
            },
            None,
        )?;
        Ok(true)
    }
    pub(super) fn prepare_host_move(&mut self, snapshot: &Discovery) -> Result<()> {
        let Pane::BeginMove {
            app,
            collection,
            destination,
            generation,
            cursor,
            catalog,
        } = self.host.pane.clone()
        else {
            return Ok(());
        };
        let Some(detail) = &snapshot.selected else {
            return Ok(());
        };
        if snapshot.generation != generation
            || snapshot.cursor.authority != cursor.authority
            || snapshot.catalog != catalog
        {
            self.host.pane = Pane::Home;
            self.host.message = "The Spaces changed. Pick it up again.".into();
            return Ok(());
        }
        let Some(source) = snapshot
            .spaces
            .iter()
            .find(|s| s.id == detail.space && s.role == Some(Role::Admin))
        else {
            self.host.pane = Pane::Home;
            return Ok(());
        };
        let dataset = if let Some(cid) = collection {
            detail
                .retained
                .iter()
                .find(|d| d.app == app && d.collection == cid)
                .map(|d| (d.collection.clone(), d.title.clone(), false))
        } else {
            detail
                .apps
                .iter()
                .find(|a| a.app == app && a.attached && !source.archived)
                .map(|a| (a.collection.clone(), a.title.clone(), true))
        };
        let Some((collection, title, attached)) = dataset else {
            self.host.pane = Pane::Home;
            self.host.message = "The data changed. Pick it up again.".into();
            return Ok(());
        };
        let operation = state::writer_nonce()?
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        self.host.moving = Some(MoveDraft {
            source: source.id.clone(),
            destination: destination.clone(),
            app,
            collection,
            title,
            attached,
            generation: snapshot.generation,
            cursor: snapshot.cursor.clone(),
            catalog: snapshot.catalog.clone(),
            mode: if attached {
                MoveMode::AppAndData
            } else {
                MoveMode::DataOnly
            },
            operation,
        });
        self.host.page = 0;
        self.host.pane = Pane::MoveDestinations;
        if let Some(destination) = destination {
            self.host_destination(destination, Pane::MoveReview, None)?;
        }
        Ok(())
    }
    pub(super) fn confirm_host_move(&mut self) -> Result<bool> {
        let snapshot = self.spaces.snapshot().ok_or("Spaces unavailable")?;
        let Some(moving) = self.host.moving.clone() else {
            return Ok(false);
        };
        let destination = moving.destination.clone().ok_or("Select a destination")?;
        let detail = snapshot
            .selected
            .as_ref()
            .filter(|d| d.space == destination);
        let current = snapshot.generation == moving.generation
            && snapshot.cursor.authority == moving.cursor.authority
            && snapshot.catalog == moving.catalog
            && [&moving.source, &destination].into_iter().all(|id| {
                snapshot
                    .spaces
                    .iter()
                    .any(|s| &s.id == id && s.role == Some(Role::Admin))
            })
            && snapshot
                .spaces
                .iter()
                .any(|s| s.id == destination && !s.archived);
        if let Some(detail) = detail.filter(|_| current) {
            let destination_collection = detail
                .apps
                .iter()
                .find(|a| a.app == moving.app)
                .map(|a| a.collection.clone());
            let change = Change::Move {
                operation: moving.operation,
                source: moving.source,
                destination,
                app: moving.app,
                collection: moving.collection,
                destination_collection,
                mode: moving.mode,
            };
            if self.spaces.request(change).is_ok() {
                self.host.moving = None;
                self.host.pane = Pane::Home;
            } else {
                self.host.message = "Wait for the current change, then try again.".into();
            }
        } else {
            self.host.message = "The Spaces changed. Close and pick up the app again.".into();
        }
        self.build_host(true)?;
        Ok(true)
    }
}
