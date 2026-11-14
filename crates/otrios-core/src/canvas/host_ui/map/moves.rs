// SPDX-License-Identifier: LGPL-3.0-only
//! One review for a cross-Space drop; keyboard pickup uses the same decision.
use super::*;
use otrios_interfaces::spaces::{Audience, MoveMode};

pub(super) fn render(
    b: &mut Builder,
    snapshot: &Discovery,
    model: &mut Model,
    area: [f32; 4],
    font: Option<&crate::dashboard::font::Font>,
) {
    let [left, top, width, _] = area;
    b.flat(
        "Cancel",
        "Cancel",
        [left + width - 100., top, 100., 48.],
        HostAction::Pane(Pane::Home),
    );
    let Some(moving) = &model.moving else {
        b.nodes
            .push(text("Loading app…", [left, top, width, 40.], 2.));
        return;
    };
    let valid = snapshot.generation == moving.generation
        && snapshot.cursor.authority == moving.cursor.authority
        && snapshot.catalog == moving.catalog
        && snapshot
            .spaces
            .iter()
            .any(|s| s.id == moving.source && s.role == Some(Role::Admin));
    if !valid {
        b.nodes.push(text(
            "The Spaces changed. Close and pick it up again.",
            [left, top + 64., width, 96.],
            2.,
        ));
        return;
    }
    b.nodes
        .push(text(&moving.title, [left, top, width - 112., 56.], 2.));
    if model.pane == Pane::MoveDestinations {
        let destinations = snapshot
            .spaces
            .iter()
            .filter(|s| s.id != moving.source && !s.archived && s.role == Some(Role::Admin))
            .collect::<Vec<_>>();
        b.nodes
            .push(text("Move to", [left, top + 64., width, 32.], 1.5));
        for (i, space) in destinations.iter().enumerate() {
            b.flat(
                &format!("Move to {}", space.name),
                &space.name,
                [left, top + 108. + i as f32 * 64., width, 56.],
                HostAction::MoveDestination(space.id.clone()),
            );
        }
        if destinations.is_empty() {
            b.nodes.push(text(
                "No other Spaces you administer.",
                [left, top + 108., width, 64.],
                1.5,
            ));
        }

        return;
    }
    let Some(destination) = snapshot.spaces.iter().find(|s| {
        Some(&s.id) == moving.destination.as_ref() && !s.archived && s.role == Some(Role::Admin)
    }) else {
        return;
    };
    let Some(detail) = snapshot
        .selected
        .as_ref()
        .filter(|d| d.space == destination.id)
    else {
        return;
    };
    let source = snapshot
        .spaces
        .iter()
        .find(|s| s.id == moving.source)
        .unwrap();
    let audience = match destination.audience {
        Audience::Closed => "Private",
        Audience::PublicView => "Public · read only",
        Audience::PublicComment => "Public · signed-in comments",
        Audience::PublicEdit => "Public · Anyone can edit",
    };
    let mut y = top + 64.;
    for line in caption_lines(
        &format!("{} to {}", source.name, destination.name),
        width,
        font,
    ) {
        b.nodes.push(text(&line, [left, y, width, 28.], 1.5));
        y += 30.;
    }
    b.nodes
        .push(text(audience, [left, y + 4., width, 32.], 1.5));
    y += 48.;
    let modes: &[(MoveMode, &str)] = if moving.attached {
        &[
            (MoveMode::AppAndData, "App and data"),
            (MoveMode::AppOnly, "App only"),
            (MoveMode::DataOnly, "Data only"),
        ]
    } else {
        &[(MoveMode::DataOnly, "Data only")]
    };
    for (mode, caption) in modes {
        b.choice(
            caption,
            moving.mode == *mode,
            [left, y, width, 48.],
            HostAction::MoveMode(*mode),
        );
        y += 52.;
    }
    let occupied = detail.apps.iter().any(|a| a.app == moving.app);
    let mut notes = vec![match moving.mode {
        MoveMode::AppAndData => "The app and its data leave the source Space.",
        MoveMode::AppOnly => "Source data stays as a ghost. Destination data is kept.",
        MoveMode::DataOnly if moving.attached => "The source app starts with empty data.",
        MoveMode::DataOnly => "This dataset leaves the source Space.",
    }];
    if occupied && moving.mode != MoveMode::AppOnly {
        notes.push("Destination data will be kept as a backup.");
    }
    let public_data = destination.audience != Audience::Closed && moving.mode != MoveMode::AppOnly;
    if public_data {
        notes.push("This data will become publicly readable.");
    }
    for note in notes {
        for line in caption_lines(note, width, font) {
            b.nodes.push(text(&line, [left, y, width, 28.], 1.3));
            y += 28.;
        }
        y += 6.;
    }
    let caption = if public_data {
        "Move and publish data"
    } else {
        "Move"
    };
    b.flat(
        caption,
        caption,
        [left, y + 12., width, 56.],
        HostAction::MoveConfirm,
    );
}
