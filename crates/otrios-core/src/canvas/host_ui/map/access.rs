// SPDX-License-Identifier: LGPL-3.0-only
//! One draft for the Space audience and explicitly edited membership rows.
use super::*;
use otrios_interfaces::spaces::Audience;

fn audience(value: Audience) -> &'static str {
    match value {
        Audience::Closed => "Private",
        Audience::PublicView => "Public · Read only",
        Audience::PublicComment => "Public · Signed-in comments",
        Audience::PublicEdit => "Public · Anyone can edit",
    }
}
pub(super) fn render(
    b: &mut Builder,
    snapshot: &Discovery,
    model: &mut Model,
    entry: &str,
    area: [f32; 4],
    font: Option<&crate::dashboard::font::Font>,
) -> Option<String> {
    let [left, top, width, _] = area;
    let selected = snapshot.selected.as_ref()?;
    let summary = snapshot
        .spaces
        .iter()
        .find(|s| s.id == selected.space && s.role == Some(Role::Admin))?;
    if model.access.is_none() {
        let members = selected.members.clone()?;
        model.access = Some(AccessDraft {
            space: selected.space.clone(),
            generation: snapshot.generation,
            cursor: snapshot.cursor.clone(),
            catalog: snapshot.catalog.clone(),
            audience: summary.audience,
            original_audience: summary.audience,
            original_members: members.clone(),
            members,
        });
    }
    let draft = model.access.as_ref()?;
    if draft.space != selected.space
        || draft.generation != snapshot.generation
        || draft.cursor.authority != snapshot.cursor.authority
        || draft.catalog != snapshot.catalog
    {
        b.nodes.push(text(
            "The Space changed. Close and review access again.",
            [left, top, width, 80.],
            2.,
        ));
        b.flat(
            "Close",
            "Close",
            [left + width - 100., top + 112., 100., 56.],
            HostAction::Pane(Pane::Home),
        );
        return None;
    }
    match model.pane {
        Pane::AccessAudience => {
            b.nodes.push(text("Access", [left, top, width, 40.], 2.));
            for (i, value) in [
                Audience::Closed,
                Audience::PublicView,
                Audience::PublicComment,
                Audience::PublicEdit,
            ]
            .into_iter()
            .enumerate()
            {
                b.choice(
                    audience(value),
                    value == draft.audience,
                    [left, top + 72. + i as f32 * 64., width, 56.],
                    HostAction::AccessAudience(value),
                );
            }
            b.flat(
                "Cancel",
                "Cancel",
                [left, top + 356., 100., 56.],
                HostAction::Pane(Pane::Members),
            );
            None
        }
        Pane::AudienceConfirm { .. } => {
            b.nodes
                .push(text("Change access?", [left, top, width, 40.], 2.));
            let notice = match draft.audience {
                Audience::Closed => "Only members will be able to enter.",
                Audience::PublicView => "Anyone will be able to read this Space.",
                Audience::PublicComment => "Anyone can read. Signed-in visitors can comment.",
                Audience::PublicEdit => "Anyone can read and edit this Space without signing in.",
            };
            let mut y = top + 72.;
            for line in caption_lines(notice, width, font) {
                b.nodes.push(text(&line, [left, y, width, 32.], 1.5));
                y += 32.;
            }
            b.nodes
                .push(text(&summary.name, [left, y + 24., width, 40.], 1.5).color(SOFT));
            b.flat(
                "Cancel",
                "Cancel",
                [left, y + 96., 100., 56.],
                HostAction::Pane(Pane::Members),
            );
            b.flat(
                "Apply",
                "Apply",
                [left + width - 100., y + 96., 100., 56.],
                HostAction::AccessApply,
            );
            None
        }
        _ => {
            b.nodes
                .push(text("People & access", [left, top, width, 40.], 2.));
            b.nodes
                .push(text("Access", [left, top + 64., width, 28.], 1.3).color(SOFT));
            b.flat(
                "Access",
                audience(draft.audience),
                [left, top + 96., width, 56.],
                HostAction::Pane(Pane::AccessAudience),
            );
            b.divider([left, top + 176., width, 1.]);
            model.page = model
                .page
                .min(draft.members.len().saturating_sub(1) / MEMBER_PAGE);
            let rows = draft
                .members
                .iter()
                .skip(model.page * MEMBER_PAGE)
                .take(MEMBER_PAGE)
                .collect::<Vec<_>>();
            for (index, member) in rows.iter().enumerate() {
                let y = top + 200. + index as f32 * 68.;
                for (line, caption) in caption_lines(&member.identity, (width - 160.).max(1.), font)
                    .iter()
                    .take(2)
                    .enumerate()
                {
                    b.nodes.push(text(
                        caption,
                        [left, y + line as f32 * 24., (width - 160.).max(1.), 24.],
                        1.3,
                    ));
                }
                b.flat(
                    &format!("Role: {}", member.identity),
                    if member.role == Role::Admin {
                        "Admin"
                    } else {
                        "Member"
                    },
                    [left + width - 152., y, 104., 48.],
                    HostAction::AccessRole(member.identity.clone()),
                );
                b.flat(
                    &format!("Remove: {}", member.identity),
                    "−",
                    [left + width - 44., y, 44., 48.],
                    HostAction::AccessRemove(member.identity.clone()),
                );
            }
            let mut y = top + 200. + rows.len() as f32 * 68.;
            if model.page > 0 {
                b.flat("Previous", "<", [left, y, 48., 48.], HostAction::Page(-1));
            }
            if (model.page + 1) * MEMBER_PAGE < draft.members.len() {
                b.flat("Next", ">", [left + 56., y, 48., 48.], HostAction::Page(1));
            }
            if draft.members.len() > MEMBER_PAGE {
                y += 56.;
            }
            b.entry("Person to add", [left, y + 8., width - 56., 64.]);
            b.flat(
                "Add person",
                "+",
                [left + width - 48., y + 16., 48., 48.],
                HostAction::AccessAdd,
            );
            b.flat(
                "Cancel",
                "Cancel",
                [left + width - 216., y + 104., 100., 56.],
                HostAction::Pane(Pane::Home),
            );
            b.flat(
                "Save",
                "Save",
                [left + width - 100., y + 104., 100., 56.],
                HostAction::AccessSave,
            );
            let key = format!("{}:access", selected.space);
            if model.field_owner.as_ref() != Some(&key) {
                model.field_owner = Some(key);
                Some(String::new())
            } else {
                Some(entry.into())
            }
        }
    }
}
