// SPDX-License-Identifier: LGPL-3.0-only
//! A code publication needs both host policy and current Space administration.
use super::*;
use crate::spaces::{self, Error};
use otrios_interfaces::spaces::{Audience, Command, InstalledApp, Registry, Role};

pub fn authorize_share(
    registry: &Registry,
    actor: &str,
    host: &HostPolicy,
    command: &Command,
    installed: &InstalledApp,
    catalog: &[u8],
) -> std::result::Result<(), Error> {
    spaces::encode_command(command)?;
    let Command::ShareApp {
        space,
        app,
        digest,
        expected_generation,
        catalog: reviewed,
        ..
    } = command
    else {
        return Err(Error::Invalid);
    };
    if !host.allows(actor) {
        return Err(Error::Denied);
    }
    let selected = registry
        .spaces
        .iter()
        .find(|s| s.id == *space && !s.archived)
        .filter(|s| spaces::role(s, Some(actor)) == Some(Role::Admin))
        .ok_or(Error::Denied)?;
    if *expected_generation != registry.generation
        || reviewed != catalog
        || catalog.len() != 32
        || installed.policy.app != *app
        || installed.bundle_sha256.as_deref() != Some(digest)
    {
        return Err(Error::Conflict);
    }
    if !selected
        .apps
        .iter()
        .any(|a| a.attached && a.policy.app == *app && a.policy.schema == installed.policy.schema)
    {
        return Err(Error::Unavailable);
    }
    Ok(())
}

#[derive(Clone)]
pub struct AppImportTarget {
    pub space: String,
    pub generation: u64,
    pub catalog: Vec<u8>,
    pub release: String,
    pub audience: Audience,
}
impl AppImportTarget {
    pub fn authorize(
        &self,
        registry: &Registry,
        actor: &str,
        catalog: &[u8],
        release: &str,
    ) -> std::result::Result<(), spaces::Error> {
        let space = registry
            .spaces
            .iter()
            .find(|s| s.id == self.space && !s.archived)
            .filter(|s| spaces::role(s, Some(actor)) == Some(Role::Admin))
            .ok_or(spaces::Error::Denied)?;
        if self.generation != registry.generation
            || self.catalog.len() != 32
            || self.catalog != catalog
            || self.release != release
            || release.len() != 64
            || self.audience != space.audience
        {
            return Err(spaces::Error::Conflict);
        }
        Ok(())
    }
}
