// SPDX-License-Identifier: LGPL-3.0-only
//! Trust and destination review for code intake; no activation or guest execution.
use super::*;
use crate::{
    bundle::{self, VerifiedBundle},
    canvas, spaces,
};
use otrios_interfaces::spaces::{Audience, Registry};

pub struct AppCandidate {
    bundle: VerifiedBundle,
}
impl AppCandidate {
    #[cfg(all(feature = "signed-bundles", not(target_arch = "wasm32")))]
    pub fn qualify(archive: &[u8], key: &[u8; 32], source: Option<&AppSource>) -> Result<Self> {
        let bundle = bundle::verify(archive::decode(archive::Kind::App, archive)?, key)?;
        bundle::qualify_install(&bundle)?;
        if bundle.summary().info.app.id == "home" {
            return Err("Home is owned by registry composition".into());
        }
        if let Some(source) = source {
            source.check_bundle(bundle.summary())?;
        }
        Ok(Self { bundle })
    }
    pub fn app(&self) -> &str {
        &self.bundle.summary().info.app.id
    }
    pub fn admit(
        self,
        target: AppImportTarget,
        registry: &Registry,
        actor: &str,
        host: &HostPolicy,
        previous: Option<&VerifiedBundle>,
        now: u64,
    ) -> Result<StagedApp> {
        // The adapter must compare the captured target to its current baseline
        // before admission, and do so again before consuming this stage.
        target
            .authorize(registry, actor, &target.catalog, &target.release)
            .map_err(canvas::err)?;
        let summary = self.bundle.summary();
        if summary.info.private && target.audience != Audience::Closed {
            return Err("Closed-only app cannot attach to this audience".into());
        }
        if let Some(previous) = previous {
            bundle::compatible_install(previous, &self.bundle)?;
        }
        let installed = previous.map(|b| b.summary().digest.clone());
        let changes_code = installed.as_deref() != Some(summary.digest.as_str());
        if changes_code && !host.allows(actor) {
            return Err("Host installation authorization required".into());
        }
        let mut nonce = [0; 16];
        getrandom::fill(&mut nonce).map_err(|_| "App stage randomness unavailable")?;
        let review = AppReview {
            stage: canvas::hex(&nonce),
            space: target.space.clone(),
            app: summary.info.app.id.clone(),
            digest: summary.digest.clone(),
            title: summary.info.app.title.clone(),
            revision: summary.info.revision,
            bytes: summary.bytes as u64,
            affected_spaces: if changes_code {
                registry
                    .spaces
                    .iter()
                    .filter(|s| {
                        !s.archived
                            && s.apps
                                .iter()
                                .any(|a| a.attached && a.policy.app == summary.info.app.id)
                    })
                    .count() as u32
            } else {
                0
            },
            installed,
            expected_generation: target.generation,
            catalog: target.catalog.clone(),
            release: target.release.clone(),
            audience: target.audience,
            expires_unix_ms: now
                .checked_add(10 * 60 * 1000)
                .ok_or("App stage expiry overflow")?,
        };
        Ok(StagedApp {
            actor: actor.into(),
            created: now,
            target,
            review,
            bundle: self.bundle,
        })
    }
}
pub struct StagedApp {
    actor: String,
    created: u64,
    target: AppImportTarget,
    review: AppReview,
    bundle: VerifiedBundle,
}
impl StagedApp {
    /// Policy for a newly installed single-root bundle. Hosts use their complete
    /// existing owner/root policy for updates with subsidiary screens.
    pub fn standalone_policy(&self) -> Result<otrios_interfaces::spaces::AppPolicy> {
        let info = &self.bundle.summary().info;
        let ui = canvas::decode_ui(self.bundle.file(otrios_interfaces::bundle::UI_PATH)?)?;
        let chat = matches!(
            ui.presentation,
            Some(otrios_interfaces::canvas::Presentation::Chat)
        );
        Ok(otrios_interfaces::spaces::AppPolicy {
            app: info.app.id.clone(),
            schema: spaces::collection_schema(&[otrios_interfaces::spaces::RootSchema {
                state_ref: info.app.state_ref.clone(),
                schema: ui.state_schema.clone(),
            }])
            .map_err(canvas::err)?,
            closed_only: info.private || info.app.id == "monitoring",
            editable: chat || ui.state_schema.fields.iter().any(|f| f.writable),
            comments: chat,
        })
    }
    pub fn review(&self) -> &AppReview {
        &self.review
    }
    pub fn expired(&self, now: u64) -> bool {
        now < self.created || now >= self.review.expires_unix_ms
    }
    pub fn owned_by(&self, actor: &str, stage: &str) -> bool {
        self.actor == actor && self.review.stage == stage
    }
    pub fn authorize(
        &self,
        registry: &Registry,
        actor: &str,
        host: &HostPolicy,
        catalog: &[u8],
        release: &str,
        now: u64,
    ) -> Result<()> {
        if actor != self.actor || self.expired(now) {
            return Err("App review expired or belongs to another actor".into());
        }
        self.target
            .authorize(registry, actor, catalog, release)
            .map_err(canvas::err)?;
        if self.review.installed.as_deref() != Some(self.review.digest.as_str())
            && !host.allows(actor)
        {
            return Err("Host installation authorization changed".into());
        }
        Ok(())
    }
    pub fn into_parts(self) -> (AppReview, VerifiedBundle) {
        (self.review, self.bundle)
    }
}
