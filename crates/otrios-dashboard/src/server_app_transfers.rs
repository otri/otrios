// SPDX-License-Identifier: Apache-2.0
//! Storage adapters for covered portable codecs and dataset admission.
use super::*;
use otrios_core::transfers::{self as core, DatasetRef, PinnedCollection, PinnedRoot, RootPolicy};
pub(super) struct PendingData {
    stage: core::StagedReplacement,
    _credit: tokio::sync::OwnedSemaphorePermit,
}
pub(super) struct PendingApp {
    stage: core::StagedApp,
    _credit: tokio::sync::OwnedSemaphorePermit,
}

impl Hub {
    pub(crate) fn import_app_stage(
        &mut self,
        actor: &str,
        host: &core::HostPolicy,
        stage: &str,
    ) -> std::result::Result<core::AppImportState, otrios_core::spaces::Error> {
        use otrios_core::spaces::Error;
        let pending = self
            .staged_app
            .as_ref()
            .filter(|p| p.stage.owned_by(actor, stage))
            .ok_or(Error::Denied)?;
        let policy = self
            .app_policy(&pending.stage.review().app)
            .or_else(|_| pending.stage.standalone_policy())
            .map_err(|_| Error::Unavailable)?;
        let authority = core::InstallAuthority {
            host,
            catalog: &self.active,
            release: &self.active_release,
            now: chat::now(),
        };
        let pending = self.staged_app.take().ok_or(Error::Unavailable)?;
        let cursor = otrios_core::spaces::advance_discovery(&self.discovery)?;
        let state = self
            .spaces
            .as_mut()
            .ok_or(Error::Unavailable)?
            .queue_app_import(actor, pending.stage, &policy, &authority)?;
        self.discovery = cursor;
        Ok(state)
    }
    pub(crate) fn import_status(
        &self,
        actor: &str,
        operation: &str,
    ) -> std::result::Result<Option<core::AppImportState>, otrios_core::spaces::Error> {
        self.spaces
            .as_ref()
            .ok_or(otrios_core::spaces::Error::Unavailable)?
            .app_import_status(actor, operation)
    }
    pub(crate) fn pending_install(
        &self,
    ) -> std::result::Result<Option<core::AppInstallJob>, otrios_core::spaces::Error> {
        self.spaces
            .as_ref()
            .map_or(Ok(None), |s| s.pending_app_install())
    }
    pub(crate) fn operator_import_state(
        &self,
        operation: &str,
    ) -> std::result::Result<Option<core::AppImportState>, otrios_core::spaces::Error> {
        self.spaces
            .as_ref()
            .ok_or(otrios_core::spaces::Error::Unavailable)?
            .operator_app_import_state(operation)
    }
    pub(crate) fn reject_install(
        &mut self,
        operation: &str,
    ) -> std::result::Result<core::AppImportState, otrios_core::spaces::Error> {
        let state = self
            .spaces
            .as_mut()
            .ok_or(otrios_core::spaces::Error::Unavailable)?
            .reject_queued_app_import(operation)?;
        self.discovery = otrios_core::spaces::advance_discovery(&self.discovery)?;
        Ok(state)
    }
    pub(crate) fn install_archive(
        &self,
        operation: &str,
    ) -> std::result::Result<Vec<u8>, otrios_core::spaces::Error> {
        self.spaces
            .as_ref()
            .ok_or(otrios_core::spaces::Error::Unavailable)?
            .app_install_archive(
                operation,
                &self
                    .publisher
                    .ok_or(otrios_core::spaces::Error::Unavailable)?,
            )
    }
    pub(crate) fn arm_install(
        &mut self,
        host: &core::HostPolicy,
        operation: &str,
        release: &str,
    ) -> std::result::Result<core::AppImportState, otrios_core::spaces::Error> {
        let authority = core::InstallAuthority {
            host,
            catalog: &self.active,
            release: &self.active_release,
            now: chat::now(),
        };
        let result = self
            .spaces
            .as_mut()
            .ok_or(otrios_core::spaces::Error::Unavailable)?
            .arm_app_import(operation, release, &authority);
        self.discovery = otrios_core::spaces::advance_discovery(&self.discovery)?;
        result
    }
    pub(crate) fn finish_install(&mut self) -> Result<()> {
        let Some(job) = self.pending_install().map_err(|e| e.to_string())? else {
            return Ok(());
        };
        if job.state.phase != core::AppImportPhase::Armed
            || self.active_release == job.review.release
        {
            return Ok(());
        }
        let digest = self
            .bundles
            .get(&self.active)
            .and_then(|b| b.packages.values().find(|p| p.app.id == job.review.app))
            .and_then(|p| p.bundle_sha256.as_deref())
            .ok_or("Installed import code missing")?;
        if self
            .spaces
            .as_mut()
            .ok_or("Spaces unavailable")?
            .finish_app_import(&self.active_release, digest)
            .map_err(|e| e.to_string())?
            .is_some()
        {
            self.discovery = otrios_core::spaces::advance_discovery(&self.discovery)
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
    pub(crate) fn app_import_target(
        &self,
        actor: &str,
        space: &str,
    ) -> Result<core::AppImportTarget> {
        let registry = self.registry().map_err(|e| e.to_string())?;
        let audience = registry
            .spaces
            .iter()
            .find(|s| s.id == space)
            .ok_or("Space unavailable")?
            .audience;
        let target = core::AppImportTarget {
            space: space.into(),
            generation: registry.generation,
            catalog: self.active.clone(),
            release: self.active_release.clone(),
            audience,
        };
        target
            .authorize(registry, actor, &self.active, &self.active_release)
            .map_err(|e| e.to_string())?;
        Ok(target)
    }
    pub(crate) fn expire_app_stage(&mut self, now: u64) {
        if self
            .staged_app
            .as_ref()
            .is_some_and(|p| p.stage.expired(now))
        {
            self.staged_app = None;
        }
    }
    pub(crate) fn cancel_app_stage(
        &mut self,
        actor: &str,
        id: &str,
    ) -> std::result::Result<(), otrios_core::spaces::Error> {
        if !self
            .staged_app
            .as_ref()
            .is_some_and(|p| p.stage.owned_by(actor, id))
        {
            self.spaces
                .as_mut()
                .ok_or(otrios_core::spaces::Error::Unavailable)?
                .cancel_app_import(actor, id)?;
            return Ok(());
        }
        self.staged_app = None;
        Ok(())
    }
    pub(crate) fn admit_app_stage(
        &mut self,
        actor: &str,
        host: &core::HostPolicy,
        target: core::AppImportTarget,
        candidate: core::AppCandidate,
        credit: tokio::sync::OwnedSemaphorePermit,
        releases: &Path,
    ) -> Result<core::AppReview> {
        self.expire_app_stage(chat::now());
        if self.staged_app.is_some() {
            return Err("Finish or cancel the current app review".into());
        }
        let registry = self.registry().map_err(|e| e.to_string())?;
        target
            .authorize(registry, actor, &self.active, &self.active_release)
            .map_err(|e| e.to_string())?;
        let installed = self
            .bundles
            .get(&self.active)
            .and_then(|b| b.packages.values().find(|p| p.app.id == candidate.app()));
        let previous = if let Some(installed) = installed {
            let tree = super::super::bundle_files::read_tree(
                &releases
                    .join(&self.active_release)
                    .join("authority/bundles")
                    .join(format!("{}.otapp", candidate.app())),
            )?;
            let verified = otrios_core::bundle::verify(
                tree,
                &self.publisher.ok_or("Publisher policy unavailable")?,
            )?;
            if installed.bundle_sha256.as_deref() != Some(verified.summary().digest.as_str()) {
                return Err("Installed code differs from the reviewed release".into());
            }
            Some(verified)
        } else {
            None
        };
        let stage = candidate.admit(
            target,
            registry,
            actor,
            host,
            previous.as_ref(),
            chat::now(),
        )?;
        let review = stage.review().clone();
        self.staged_app = Some(PendingApp {
            stage,
            _credit: credit,
        });
        Ok(review)
    }
    pub(crate) fn share_candidate(
        &self,
        actor: &str,
        host: &core::HostPolicy,
        command: &otrios_interfaces::spaces::Command,
        releases: &Path,
    ) -> std::result::Result<otrios_core::bundle::BundleTree, otrios_core::spaces::Error> {
        use otrios_core::spaces::Error;
        let otrios_interfaces::spaces::Command::ShareApp { app, digest, .. } = command else {
            return Err(Error::Invalid);
        };
        let package = self
            .bundles
            .get(&self.active)
            .and_then(|b| b.packages.values().find(|p| p.app.id == *app))
            .ok_or(Error::Unavailable)?;
        let installed = otrios_interfaces::spaces::InstalledApp {
            bundle_sha256: package.bundle_sha256.clone(),
            code_bytes: package.code_bytes,
            revision: package.revision,
            policy: package.policy.clone(),
            title: package.app.title.clone(),
            ui_ref: package.app.ui_ref.clone(),
            state_ref: package.app.state_ref.clone(),
        };
        core::authorize_share(
            self.registry()?,
            actor,
            host,
            command,
            &installed,
            &self.active,
        )?;
        if self.active_release.len() != 64 {
            return Err(Error::Unavailable);
        }
        let tree = super::super::bundle_files::read_tree(
            &releases
                .join(&self.active_release)
                .join("authority/bundles")
                .join(format!("{app}.otapp")),
        )
        .map_err(|_| Error::Unavailable)?;
        let inspected = otrios_core::bundle::inspect(&tree).map_err(|_| Error::Invalid)?;
        if inspected.digest != *digest || inspected.info.app.id != *app {
            return Err(Error::Conflict);
        }
        Ok(tree)
    }
    fn portable_packages(&self, app: &str) -> Result<BTreeMap<String, &Package>> {
        let bundle = self
            .bundles
            .get(&self.active)
            .ok_or("Installed catalog unavailable")?;
        let mut packages = BTreeMap::<String, &Package>::new();
        for package in bundle.packages.values().filter(|p| p.policy.app == app) {
            if canvas::decode_ui(&package.ui).is_ok_and(|ui| canvas::oib::seed(&ui).is_some()) {
                return Err("Live UI data transfers are not available yet".into());
            }
            if package.aggregate_view
                || self
                    .collector
                    .as_ref()
                    .is_some_and(|s| s.binding.app == app)
            {
                return Err("This app has no qualified portable codec".into());
            }
            if let Some(previous) = packages.insert(package.app.state_ref.clone(), package)
                && (previous.schema != package.schema || previous.chat != package.chat)
            {
                return Err("Installed root declarations conflict".into());
            }
        }
        if packages.is_empty() {
            return Err("Installed app unavailable".into());
        }
        Ok(packages)
    }
    pub(crate) fn replacement_target(
        &self,
        actor: &str,
        space: &str,
        app: &str,
        collection: &str,
    ) -> Result<core::ReplacementTarget> {
        let dataset = self.export_selection(actor, space, app, collection)?;
        let packages = self.portable_packages(app)?;
        let primary = packages
            .values()
            .next()
            .ok_or("No installed roots")?
            .primary_ref
            .clone();
        let roots = packages
            .values()
            .map(|p| RootPolicy {
                state_ref: p.app.state_ref.clone(),
                schema: p.schema.clone(),
                chat: p.chat,
            })
            .collect();
        let registry = self.registry().map_err(|e| e.to_string())?;
        let target = core::ReplacementTarget {
            dataset,
            policy: self.app_policy(app)?,
            primary,
            roots,
            catalog: self.active.clone(),
            generation: registry.generation,
            audience: registry
                .spaces
                .iter()
                .find(|s| s.id == space)
                .ok_or("Space unavailable")?
                .audience,
        };
        target
            .authorize(registry, actor, &self.active)
            .map_err(|e| e.to_string())?;
        Ok(target)
    }
    pub(crate) fn expire_data_stage(&mut self, now: u64) {
        if self
            .staged_data
            .as_ref()
            .is_some_and(|p| p.stage.expired(now))
        {
            self.staged_data = None;
        }
    }
    pub(crate) fn admit_data_stage(
        &mut self,
        actor: &str,
        candidate: core::DataCandidate,
        credit: tokio::sync::OwnedSemaphorePermit,
    ) -> std::result::Result<core::DataReview, otrios_core::spaces::Error> {
        self.expire_data_stage(chat::now());
        if self.staged_data.is_some() {
            return Err(otrios_core::spaces::Error::Capacity);
        }
        let stage = candidate.admit(self.registry()?, actor, &self.active, chat::now())?;
        let review = stage.review().clone();
        self.staged_data = Some(PendingData {
            stage,
            _credit: credit,
        });
        Ok(review)
    }
    pub(crate) fn cancel_data_stage(
        &mut self,
        actor: &str,
        id: &str,
    ) -> std::result::Result<(), otrios_core::spaces::Error> {
        if !self
            .staged_data
            .as_ref()
            .is_some_and(|p| p.stage.owned_by(actor, id))
        {
            return Err(otrios_core::spaces::Error::Denied);
        }
        self.staged_data = None;
        Ok(())
    }
    pub(crate) fn replace_data_stage(
        &mut self,
        actor: &str,
        id: &str,
    ) -> std::result::Result<u64, otrios_core::spaces::Error> {
        use otrios_core::spaces::Error;
        let pending = self
            .staged_data
            .as_ref()
            .filter(|p| p.stage.owned_by(actor, id))
            .ok_or(Error::Denied)?;
        pending
            .stage
            .authorize(self.registry()?, actor, &self.active, chat::now())?;
        let review = &pending.stage.review().replacement;
        if self
            .collector
            .as_ref()
            .is_some_and(|s| s.binding.app == review.app)
            || self.rooms.iter().any(|(key, r)| {
                key.split(':').next() == Some(review.collection.as_str()) && r.failed
            })
            || self.chat_rooms.iter().any(|(key, r)| {
                key.split(':').next() == Some(review.collection.as_str()) && !r.healthy()
            })
        {
            return Err(Error::Unavailable);
        }
        let cursor = otrios_core::spaces::advance_discovery(&self.discovery)?;
        let pending = self.staged_data.take().ok_or(Error::Unavailable)?;
        let (review, target, data) = pending.stage.into_parts();
        // Keep the single transfer credit until the complete durable operation
        // returns, including a persistence error whose outcome needs a read.
        let result = self
            .spaces
            .as_mut()
            .ok_or(Error::Unavailable)?
            .replace_portable(
                actor,
                &review.replacement,
                &target.policy,
                &target.primary,
                &data,
            );
        if result.is_ok() {
            self.discovery = cursor;
        }
        result
    }
    pub(crate) fn replacement_outcome(
        &self,
        actor: &str,
        space: &str,
        replacement: &str,
    ) -> std::result::Result<core::ReplacementOutcome, otrios_core::spaces::Error> {
        self.spaces
            .as_ref()
            .ok_or(otrios_core::spaces::Error::Unavailable)?
            .replacement_committed(actor, space, replacement)
    }
    pub(crate) fn export_selection(
        &self,
        actor: &str,
        space: &str,
        app: &str,
        collection: &str,
    ) -> Result<DatasetRef> {
        let policy = self.app_policy(app)?;
        let selected = DatasetRef {
            space: space.into(),
            app: app.into(),
            collection: collection.into(),
            schema: policy.schema.clone(),
        };
        core::authorize_dataset(
            self.registry().map_err(|e| e.to_string())?,
            actor,
            &selected,
            &policy,
        )?;
        Ok(selected)
    }
    pub(crate) fn check_export(&self, actor: &str, selected: &DatasetRef) -> Result<()> {
        core::authorize_dataset(
            self.registry().map_err(|e| e.to_string())?,
            actor,
            selected,
            &self.app_policy(&selected.app)?,
        )
    }
    /// Call under the shared admission lock. Core pins immutable accepted roots;
    /// no code executes and no unconfirmed client RAM is included.
    pub(crate) fn pin_export(
        &self,
        actor: &str,
        selected: &DatasetRef,
    ) -> Result<PinnedCollection> {
        self.check_export(actor, selected)?;
        let packages = self.portable_packages(&selected.app)?;
        let mut policies = vec![];
        let mut roots = vec![];
        let mut owned = BTreeMap::new();
        let assets = canvas::chat::assets::Assets::new(
            self.state
                .as_deref()
                .ok_or("Durable export storage required")?,
            &selected.collection,
        )?;
        for package in packages.values() {
            let key = if package.app.state_ref == package.primary_ref {
                selected.collection.clone()
            } else {
                format!(
                    "{}:{}",
                    selected.collection,
                    canvas::hex(&canvas::hash(package.app.state_ref.as_bytes()))
                )
            };
            let document = if let Some(room) = self.rooms.get(&key) {
                if room.failed {
                    return Err("Confirmed app root requires recovery".into());
                }
                room.doc.clone()
            } else {
                let file = self.collection_path(&selected.collection, package)?;
                if file.exists() {
                    store::load(
                        &file,
                        Document::load(
                            package.schema.clone(),
                            &package.app.state_ref,
                            &spaces::private_bytes(&file, canvas::MAX_BYTES)?,
                        )?,
                    )?
                } else {
                    if file.with_extension("merkle.log").exists() {
                        return Err("Missing confirmed collection checkpoint".into());
                    }
                    Document::new(package.schema.clone(), &package.app.state_ref)?
                }
            };
            let chat = if package.chat {
                let pin = if let Some(room) = self.chat_rooms.get(&key) {
                    room.chat.run_inline(|room| room.pin_portable_history())?
                } else {
                    let path = self.chat_path(&selected.collection, package)?;
                    if self.missing_owned_chat(&path, package)? {
                        return Err("Missing confirmed chat history".into());
                    }
                    self.collection_parent(&path)?;
                    let room = space_chat::Hub::new(path)?;
                    room.chat.run_inline(|room| room.pin_portable_history())?
                };
                for (hash, asset) in pin.assets()? {
                    if !owned.contains_key(&hash) {
                        if owned.len() == canvas::chat::assets::MAX_ASSETS {
                            return Err("Export owned asset closure exceeds its budget".into());
                        }
                        owned.insert(hash, assets.read(&asset)?);
                    }
                }
                Some(pin)
            } else {
                None
            };
            policies.push(RootPolicy {
                state_ref: package.app.state_ref.clone(),
                schema: package.schema.clone(),
                chat: package.chat,
            });
            roots.push(PinnedRoot { document, chat });
        }
        PinnedCollection::pin(&selected.app, &selected.schema, &policies, roots, owned)
    }
}
