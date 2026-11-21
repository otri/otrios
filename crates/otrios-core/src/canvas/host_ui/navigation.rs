// SPDX-License-Identifier: LGPL-3.0-only
//! Launcher placement never changes attachment, admission or dataset identity.
use super::*;
use otrios_interfaces::spaces::SpaceSummary;

impl Model {
    pub fn parent<'a>(&'a self, space: &'a SpaceSummary, app: &str) -> Option<&'a str> {
        let path = self.routes.get(app)?;
        space
            .apps
            .iter()
            .filter(|candidate| candidate.app != app)
            .filter_map(|candidate| {
                let parent = self.routes.get(&candidate.app)?;
                (!parent.is_empty()
                    && path
                        .strip_prefix(parent)
                        .is_some_and(|tail| tail.starts_with('/')))
                .then_some((parent.len(), candidate.app.as_str()))
            })
            .max_by_key(|(length, _)| *length)
            .map(|(_, app)| app)
    }

    pub fn visible(&self, space: &SpaceSummary, app: &str) -> bool {
        if app == "home" && self.routes.get(app).is_some_and(String::is_empty) {
            return false;
        }
        let parent = self.collection.as_ref().map(|(_, app)| app.as_str());
        self.parent(space, app) == parent
    }

    pub fn is_collection(&self, app: &str) -> bool {
        self.routes.get(app).is_some_and(|path| {
            !path.is_empty()
                && self.routes.values().any(|child| {
                    child
                        .strip_prefix(path)
                        .is_some_and(|tail| tail.starts_with('/'))
                })
        })
    }

    pub fn launcher(&mut self, snapshot: &Discovery) -> Discovery {
        if self.collection.as_ref().is_some_and(|(space, app)| {
            !snapshot
                .spaces
                .iter()
                .any(|s| s.id == *space && !s.archived && s.apps.iter().any(|a| a.app == *app))
        }) {
            self.collection = None;
            self.scroll = 0.;
        }
        let mut shown = snapshot.clone();
        shown.spaces.retain(|space| {
            self.collection
                .as_ref()
                .is_none_or(|(id, _)| *id == space.id)
        });
        for space in &mut shown.spaces {
            let original = snapshot.spaces.iter().find(|s| s.id == space.id).unwrap();
            space.apps.retain(|app| self.visible(original, &app.app));
            if let Some((_, parent)) = &self.collection {
                if let Some(app) = original.apps.iter().find(|a| a.app == *parent) {
                    space.name = format!("{} · {}", space.name, app.title);
                }
                space.retained.clear();
                space.attached_count = 1;
            }
        }
        shown
    }
}
