// SPDX-License-Identifier: LGPL-3.0-only
//! One scoped code-install decision in the existing latest-decision ledger.
//! Queued work permits metadata edits. Armed activation fences management until
//! the exact release and prepared attachment are published together. Data writes
//! continue through their existing admitted bindings; no signer lives here.
use super::*;
use crate::transfers::{
    AppImportPhase as Phase, AppImportState, AppInstallJob, AppReview, InstallAuthority,
};
const MAX_INSTALLS: usize = 64;
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Install {
    actor: String,
    submitted: u64,
    review: AppReview,
    phase: Phase,
    release: Option<String>,
    attachment: AppData,
}
impl Install {
    fn state(&self) -> AppImportState {
        AppImportState {
            operation: self.review.stage.clone(),
            phase: self.phase,
            release: self.release.clone(),
        }
    }
    fn active(&self) -> bool {
        matches!(self.phase, Phase::Queued | Phase::Armed)
    }
    fn authorize(&self, registry: &Registry, authority: &InstallAuthority<'_>) -> Result<()> {
        if authority.now < self.submitted || authority.now >= self.review.expires_unix_ms {
            return Err(Error::Unavailable);
        }
        let target = crate::transfers::AppImportTarget {
            space: self.review.space.clone(),
            generation: self.review.expected_generation,
            catalog: self.review.catalog.clone(),
            release: self.review.release.clone(),
            audience: self.review.audience,
        };
        target.authorize(registry, &self.actor, authority.catalog, authority.release)?;
        if self.review.installed.as_deref() != Some(self.review.digest.as_str())
            && !authority.host.allows(&self.actor)
        {
            return Err(Error::Denied);
        }
        Ok(())
    }
}
fn digest(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) fn validate_installs(record: &Record) -> Result<()> {
    if record.imports.len() > MAX_INSTALLS
        || record.imports.iter().filter(|i| i.active()).count() > 1
    {
        return Err(Error::Capacity);
    }
    let mut ids = BTreeSet::new();
    for install in &record.imports {
        let r = &install.review;
        if !valid_identity(&install.actor)
            || !valid_id(&r.stage)
            || !ids.insert(&r.stage)
            || !valid_id(&r.space)
            || !digest(&r.digest)
            || !digest(&r.release)
            || r.catalog.len() != 32
            || !valid_name(&r.title)
            || r.title.len() > 96
            || r.bytes > crate::bundle::MAX_TOTAL_BYTES as u64
            || r.revision == 0
            || r.installed.as_deref().is_some_and(|v| !digest(v))
            || !valid_policy(&install.attachment.policy)
            || install.attachment.policy.app != r.app
            || r.app == "home"
            || r.app.contains('/')
            || !valid_id(&install.attachment.collection)
            || !install.attachment.attached
            || r.expected_generation.checked_add(1) != Some(install.attachment.attachment_epoch)
            || r.expires_unix_ms <= install.submitted
            || r.affected_spaces > MAX_SPACES as u32
            || install.release.as_deref().is_some_and(|s| !digest(s))
            || matches!(install.phase, Phase::Armed | Phase::Applied) && install.release.is_none()
            || install.phase == Phase::Queued && install.release.is_some()
        {
            return Err(Error::Invalid);
        }
        if install.phase == Phase::Armed {
            attached_registry(&record.registry, install)?;
        }
    }
    if flexbuffers::to_vec(&record.imports)
        .map_err(io_error)?
        .len()
        > 120 * 1024
    {
        return Err(Error::Capacity);
    }
    Ok(())
}
fn attached_registry(registry: &Registry, install: &Install) -> Result<Registry> {
    if registry.generation != install.review.expected_generation {
        return Err(Error::Conflict);
    }
    let mut next = registry.clone();
    let space = next
        .spaces
        .iter_mut()
        .find(|s| s.id == install.review.space)
        .ok_or(Error::Unavailable)?;
    if let Some(slot) = space
        .apps
        .iter_mut()
        .find(|a| a.policy.app == install.review.app)
    {
        *slot = install.attachment.clone();
    } else {
        space.apps.push(install.attachment.clone());
    }
    next.generation = next.generation.checked_add(1).ok_or(Error::Capacity)?;
    validate(&next)?;
    Ok(next)
}
pub(super) fn allow_transition(current: &Record, next: &Record) -> Result<()> {
    let Some(armed) = current.imports.iter().find(|i| i.phase == Phase::Armed) else {
        return Ok(());
    };
    if current.registry == next.registry {
        return Ok(());
    }
    let Some(applied) = next
        .imports
        .iter()
        .find(|i| i.review.stage == armed.review.stage)
    else {
        return Err(Error::Unavailable);
    };
    let mut expected = armed.clone();
    expected.phase = Phase::Applied;
    if applied != &expected || next.registry != attached_registry(&current.registry, armed)? {
        return Err(Error::Unavailable);
    }
    Ok(())
}
impl Store {
    pub(crate) fn check_install_fence(&self) -> Result<()> {
        self.registry()?;
        if self.current.imports.iter().any(|i| i.phase == Phase::Armed) {
            return Err(Error::Unavailable);
        }
        Ok(())
    }
    pub fn pending_app_install(&self) -> Result<Option<AppInstallJob>> {
        self.registry()?;
        Ok(self
            .current
            .imports
            .iter()
            .find(|i| i.active())
            .map(|i| AppInstallJob {
                review: i.review.clone(),
                state: i.state(),
            }))
    }
    /// Read-only operator recovery, used only by the trusted local bridge.
    pub fn operator_app_import_state(&self, operation: &str) -> Result<Option<AppImportState>> {
        self.registry()?;
        if !valid_id(operation) {
            return Err(Error::Invalid);
        }
        Ok(self
            .current
            .imports
            .iter()
            .find(|i| i.review.stage == operation)
            .map(Install::state))
    }
    pub fn reject_queued_app_import(&mut self, operation: &str) -> Result<AppImportState> {
        self.check_install_fence()?;
        let index = self
            .current
            .imports
            .iter()
            .position(|i| i.review.stage == operation && i.phase == Phase::Queued)
            .ok_or(Error::Conflict)?;
        let mut next = self.current.clone();
        next.imports[index].phase = Phase::Rejected;
        let status = next.imports[index].state();
        self.publish(next, |_| {})?;
        Ok(status)
    }
    pub fn app_import_status(
        &self,
        actor: &str,
        operation: &str,
    ) -> Result<Option<AppImportState>> {
        self.registry()?;
        if !valid_identity(actor) || !valid_id(operation) {
            return Err(Error::Invalid);
        }
        let Some(i) = self
            .current
            .imports
            .iter()
            .find(|i| i.review.stage == operation)
        else {
            return Ok(None);
        };
        if i.actor != actor {
            return Err(Error::Denied);
        }
        let space = self
            .current
            .registry
            .spaces
            .iter()
            .find(|s| s.id == i.review.space)
            .ok_or(Error::Denied)?;
        admin(space, actor)?;
        Ok(Some(i.state()))
    }
    /// A queued decision can be cancelled by its original actor even after
    /// revocation. Armed decisions need the serialized operator recovery path.
    pub fn cancel_app_import(&mut self, actor: &str, operation: &str) -> Result<AppImportState> {
        self.registry()?;
        let index = self
            .current
            .imports
            .iter()
            .position(|i| i.review.stage == operation && i.actor == actor)
            .ok_or(Error::Denied)?;
        if self.current.imports[index].phase != Phase::Queued {
            return Err(Error::Conflict);
        }
        let mut next = self.current.clone();
        next.imports[index].phase = Phase::Rejected;
        let status = next.imports[index].state();
        self.publish(next, |_| {})?;
        Ok(status)
    }
    /// Revalidate after composition, immediately before the trusted deployer
    /// switches the immutable code pointer. No app state is copied or replaced.
    pub fn arm_app_import(
        &mut self,
        operation: &str,
        release: &str,
        authority: &InstallAuthority<'_>,
    ) -> Result<AppImportState> {
        self.check_install_fence()?;
        if !digest(release) || release == authority.release {
            return Err(Error::Invalid);
        }
        let index = self
            .current
            .imports
            .iter()
            .position(|i| i.review.stage == operation && i.phase == Phase::Queued)
            .ok_or(Error::Unavailable)?;
        if let Err(error) = self.current.imports[index].authorize(&self.current.registry, authority)
        {
            let mut next = self.current.clone();
            next.imports[index].phase = Phase::Rejected;
            self.publish(next, |_| {})?;
            return Err(error);
        }
        let mut next = self.current.clone();
        next.imports[index].phase = Phase::Armed;
        next.imports[index].release = Some(release.into());
        let state = next.imports[index].state();
        self.publish(next, |_| {})?;
        Ok(state)
    }
    /// Run after verified catalog admission, including recovery after a process
    /// exit. A mismatched release/digest leaves the fence in place for recovery.
    pub fn finish_app_import(
        &mut self,
        release: &str,
        installed_digest: &str,
    ) -> Result<Option<AppImportState>> {
        self.registry()?;
        let Some(index) = self
            .current
            .imports
            .iter()
            .position(|i| i.phase == Phase::Armed)
        else {
            return Ok(None);
        };
        let install = &self.current.imports[index];
        if install.review.release == release {
            return Ok(None);
        }
        if install.release.as_deref() != Some(release) || install.review.digest != installed_digest
        {
            return Err(Error::Conflict);
        }
        let mut next = self.current.clone();
        next.registry = attached_registry(&self.current.registry, install)?;
        next.imports[index].phase = Phase::Applied;
        let state = next.imports[index].state();
        self.publish(next, |_| {})?;
        Ok(Some(state))
    }
}

#[cfg(all(test, feature = "signed-bundles"))]
#[path = "app_import_tests.rs"]
mod tests;

#[cfg(feature = "signed-bundles")]
impl Store {
    pub fn queue_app_import(
        &mut self,
        actor: &str,
        staged: crate::transfers::StagedApp,
        policy: &AppPolicy,
        authority: &InstallAuthority<'_>,
    ) -> Result<AppImportState> {
        self.check_install_fence()?;
        if self.current.imports.iter().any(|i| i.active()) {
            return Err(Error::Capacity);
        }
        staged
            .authorize(
                &self.current.registry,
                actor,
                authority.host,
                authority.catalog,
                authority.release,
                authority.now,
            )
            .map_err(|_| Error::Denied)?;
        if !valid_policy(policy) || policy.app != staged.review().app {
            return Err(Error::Invalid);
        }
        let (review, bundle) = staged.into_parts();
        let registry = apply(
            &self.current.registry,
            actor,
            &Request {
                version: VERSION,
                expected_generation: review.expected_generation,
                change: Change::Attach {
                    space: review.space.clone(),
                    app: review.app.clone(),
                },
            },
            Some(policy),
        )?;
        let attachment = registry
            .spaces
            .iter()
            .find(|s| s.id == review.space)
            .and_then(|s| s.apps.iter().find(|a| a.policy.app == review.app))
            .ok_or(Error::Invalid)?
            .clone();
        let reuse = review.installed.as_deref() == Some(review.digest.as_str());
        let install = Install {
            actor: actor.into(),
            submitted: authority.now,
            review,
            attachment,
            phase: if reuse { Phase::Applied } else { Phase::Queued },
            release: reuse.then(|| authority.release.into()),
        };
        let mut next = self.current.clone();
        while next.imports.len() >= MAX_INSTALLS {
            next.imports.remove(0);
        }
        if reuse {
            next.registry = registry;
        } else {
            let tree = bundle
                .files()
                .map(|(p, b)| (p.to_string(), b.to_vec()))
                .collect();
            let bytes =
                crate::transfers::archive::encode(crate::transfers::archive::Kind::App, &tree)
                    .map_err(io_error)?;
            write(&suffix(&self.path, ".app-import.zip"), &bytes)?;
        }
        let state = install.state();
        next.imports.push(install);
        self.publish(next, |_| {})?;
        Ok(state)
    }
    /// Trusted bridge delivery only. Recheck the original pinned publisher and
    /// exact candidate identity on every read, including after authority restart.
    pub fn app_install_archive(&self, operation: &str, key: &[u8; 32]) -> Result<Vec<u8>> {
        self.registry()?;
        let install = self
            .current
            .imports
            .iter()
            .find(|i| i.review.stage == operation && i.active())
            .ok_or(Error::Unavailable)?;
        let bytes = read(
            &suffix(&self.path, ".app-import.zip"),
            crate::transfers::MAX_APP_ARCHIVE_BYTES,
            true,
        )?
        .ok_or(Error::Unavailable)?;
        let tree = crate::transfers::archive::decode(crate::transfers::archive::Kind::App, &bytes)
            .map_err(|_| Error::Invalid)?;
        let bundle = crate::bundle::verify(tree, key).map_err(|_| Error::Denied)?;
        if bundle.summary().digest != install.review.digest
            || bundle.summary().info.app.id != install.review.app
        {
            return Err(Error::Conflict);
        }
        Ok(bytes)
    }
}
