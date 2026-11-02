// SPDX-License-Identifier: LGPL-3.0-only
//! Retained-data controls pin every destructive decision to reviewed metadata.
use super::*;
pub(super) fn is_confirmation(pane: &Pane) -> bool {
    matches!(
        pane,
        Pane::RemoveApp(_) | Pane::DeleteDataset(_) | Pane::RestoreDataset(_) | Pane::DeleteSpace
    )
}
impl Builder<'_> {
    fn destructive(&mut self, label: &str, frame: [f32; 4], action: HostAction) {
        self.nodes.push(
            a::canvas(vec![
                a::rounded_rect([187, 57, 51, 255])
                    .frame([0., 0., frame[2], frame[3]])
                    .corner_radius(14.0.into()),
                text(label, [14., 14., frame[2] - 28., 32.], 2.).color(WHITE),
            ])
            .frame(frame)
            .semantic_label(label)
            .on_press(a::Event::back()),
        );
        self.actions.push(action);
    }
    pub(super) fn minus(&mut self, label: &str, frame: [f32; 4], action: HostAction) {
        self.nodes.push(
            a::canvas(vec![
                a::circle([187, 57, 51, 255]).frame([6., 6., 32., 32.]),
                a::rectangle(WHITE).frame([14., 21., 16., 2.]),
            ])
            .frame(frame)
            .semantic_label(&short(label, 94))
            .on_press(a::Event::back()),
        );
        self.actions.push(action);
    }
}
pub(super) fn ghost_tile(
    app: &str,
    title: &str,
    width: f32,
    font: Option<&crate::dashboard::font::Font>,
) -> Vec<a::Element> {
    let mut nodes = app_tile(app, title, width, false, font);
    nodes.insert(
        2,
        a::rounded_rect([247, 246, 240, 160])
            .frame([(width - 72.) / 2., 0., 72., 72.])
            .corner_radius(20.0.into()),
    );
    nodes
}
pub(super) fn render(
    b: &mut Builder,
    snapshot: &Discovery,
    model: &mut Model,
    area: [f32; 4],
    font: Option<&crate::dashboard::font::Font>,
) {
    let [left, top, width, h] = area;
    let Some(detail) = &snapshot.selected else {
        return;
    };
    let Some(space) = snapshot
        .spaces
        .iter()
        .find(|s| s.id == detail.space && s.role == Some(Role::Admin))
    else {
        return;
    };
    let pane = model.pane.clone();
    if pane == Pane::Datasets {
        b.nodes
            .push(text("Retained data", [left, top, width, 40.], 2.));
        model.page = model
            .page
            .min(detail.retained.len().saturating_sub(1) / PAGE);
        for (index, data) in detail
            .retained
            .iter()
            .skip(model.page * PAGE)
            .take(PAGE)
            .enumerate()
        {
            let tile = width / 2.;
            b.nodes.push(
                a::canvas(ghost_tile(
                    &data.app,
                    &format!("{} (data)", data.title),
                    tile,
                    font,
                ))
                .frame([
                    left + (index % 2) as f32 * tile,
                    top + 64. + (index / 2) as f32 * 152.,
                    tile,
                    144.,
                ])
                .semantic_label(&short(&format!("Data: {}", data.title), 94))
                .on_press(a::Event::back()),
            );
            b.actions
                .push(HostAction::Pane(Pane::Dataset(data.collection.clone())));
        }
        if detail.retained.is_empty() {
            b.nodes
                .push(text("No retained data.", [left, top + 72., width, 40.], 2.));
        }
        if model.page > 0 {
            b.flat(
                "Previous",
                "‹",
                [left, top + 380., 48., 48.],
                HostAction::Page(-1),
            );
        }
        if (model.page + 1) * PAGE < detail.retained.len() {
            b.flat(
                "Next",
                "›",
                [left + 56., top + 380., 48., 48.],
                HostAction::Page(1),
            );
        }
        return;
    }
    let collection = match &pane {
        Pane::RemoveApp(app) => detail
            .apps
            .iter()
            .find(|a| a.app == *app && a.attached)
            .map(|a| a.collection.clone()),
        Pane::Dataset(cid) | Pane::DeleteDataset(cid) | Pane::RestoreDataset(cid) => detail
            .retained
            .iter()
            .find(|d| d.collection == *cid)
            .map(|d| d.collection.clone()),
        Pane::DeleteSpace => None,
        _ => return,
    };
    if model.review.is_none() || matches!(pane, Pane::Dataset(_)) {
        model.review = Some(Review {
            space: space.id.clone(),
            generation: snapshot.generation,
            collection: collection.clone(),
        });
    }
    let review = model.review.as_ref().unwrap();
    let current = (collection.is_some() || pane == Pane::DeleteSpace)
        && review.space == space.id
        && review.generation == snapshot.generation
        && review.collection == collection;
    if !current {
        b.nodes.push(text(
            "The Space changed. Close and review it again.",
            [left, top, width, 80.],
            2.,
        ));
        b.flat(
            "Close",
            "Close",
            [left, (h - 88.).min(top + 544.), 100., 56.],
            HostAction::Pane(Pane::Home),
        );
        return;
    }
    let generation = review.generation;
    let button_y = (h - 160.).min(top + 360.);
    match pane {
        Pane::RemoveApp(app) => {
            let Some(data) = detail.apps.iter().find(|a| a.app == app && a.attached) else {
                return;
            };
            b.nodes
                .push(text("Delete App", [left, top, width, 40.], 3.));
            b.nodes
                .push(text(&data.title, [left, top + 72., width, 40.], 2.));
            b.flat(
                "Remove Data",
                if model.remove_data {
                    "●  Remove Data"
                } else {
                    "○  Remove Data"
                },
                [left, top + 144., width, 56.],
                HostAction::ToggleRemoveData,
            );
            let change = if model.remove_data {
                Change::DeleteData {
                    space: space.id.clone(),
                    app,
                    collection: data.collection.clone(),
                    remove: true,
                }
            } else {
                Change::Remove {
                    space: space.id.clone(),
                    app,
                }
            };
            b.flat(
                "Cancel",
                "Cancel",
                [left, button_y, 100., 56.],
                HostAction::Pane(Pane::Home),
            );
            b.destructive(
                "Delete",
                [left + width - 100., button_y, 100., 56.],
                HostAction::Reviewed(change, generation),
            );
        }
        Pane::Dataset(cid) => {
            let data = detail
                .retained
                .iter()
                .find(|d| d.collection == cid)
                .unwrap();
            b.name([left, top, width, 64.]);
            b.nodes.push(
                text(
                    &format!("{} · data", data.app),
                    [left + 8., top + 74., width - 16., 32.],
                    1.3,
                )
                .color(SOFT),
            );
            b.stats(data.stored_bytes, Some(0), [left, top + 132., width, 76.]);
            b.info("Space", &space.name, [left, top + 244., width, 40.]);
            b.divider([left, top + 306., width, 1.]);
            if !space.archived && data.availability == AppAvailability::Available {
                b.flat(
                    "Restore App",
                    "Restore App",
                    [left, top + 328., width, 56.],
                    HostAction::Pane(Pane::RestoreDataset(cid.clone())),
                );
            } else {
                b.nodes.push(text(
                    if space.archived {
                        "Restore the Space first."
                    } else {
                        availability(data.availability)
                    },
                    [left, top + 328., width, 64.],
                    1.3,
                ));
            }
            b.flat(
                "Export Data",
                "Export Data",
                [left, top + 388., width, 56.],
                HostAction::TransferExport(data.app.clone(), data.collection.clone()),
            );
            b.flat(
                "Replace Data",
                "Replace Data",
                [left, top + 448., width, 56.],
                HostAction::TransferReplace(data.app.clone(), data.collection.clone()),
            );
        }
        Pane::DeleteDataset(cid) | Pane::RestoreDataset(cid) => {
            let data = detail
                .retained
                .iter()
                .find(|d| d.collection == cid)
                .unwrap();
            let restore = matches!(model.pane, Pane::RestoreDataset(_));
            b.nodes.push(text(
                if restore {
                    "Restore Data"
                } else {
                    "Delete Data"
                },
                [left, top, width, 40.],
                3.,
            ));
            b.nodes
                .push(text(&data.title, [left, top + 72., width, 64.], 2.));
            let outgoing = detail.apps.iter().find(|a| a.app == data.app);
            let backup_notice = outgoing.map(|a| format!("{} will be kept as a backup.", a.title));
            b.nodes.push(text(
                if restore && data.origin.is_some() && outgoing.is_some() {
                    backup_notice.as_deref().unwrap()
                } else if restore {
                    "Restore this app with its saved data."
                } else {
                    "Remove this dataset from this Space."
                },
                [left, top + 152., width, 64.],
                1.5,
            ));
            if restore && space.audience != otrios_interfaces::spaces::Audience::Closed {
                b.nodes.push(text(
                    "Data will use this Space’s public access.",
                    [left, top + 232., width, 64.],
                    1.5,
                ));
            }
            let change = if restore {
                Change::RestoreDataset {
                    space: space.id.clone(),
                    app: data.app.clone(),
                    collection: cid.clone(),
                }
            } else {
                Change::DeleteDataset {
                    space: space.id.clone(),
                    app: data.app.clone(),
                    collection: cid.clone(),
                }
            };
            b.flat(
                "Cancel",
                "Cancel",
                [left, button_y, 100., 56.],
                HostAction::Pane(Pane::Dataset(cid)),
            );
            if !restore || !space.archived && data.availability == AppAvailability::Available {
                let label = if restore { "Restore" } else { "Delete" };
                if restore {
                    b.flat(
                        label,
                        label,
                        [left + width - 100., button_y, 100., 56.],
                        HostAction::Reviewed(change, generation),
                    );
                } else {
                    b.destructive(
                        label,
                        [left + width - 100., button_y, 100., 56.],
                        HostAction::Reviewed(change, generation),
                    );
                }
            }
        }
        Pane::DeleteSpace => {
            b.nodes
                .push(text("Delete Space", [left, top, width, 40.], 3.));
            b.nodes
                .push(text(&space.name, [left, top + 72., width, 40.], 2.));
            b.flat(
                "Remove Data",
                if model.remove_data {
                    "●  Remove Data"
                } else {
                    "○  Remove Data"
                },
                [left, top + 144., width, 56.],
                HostAction::ToggleRemoveData,
            );
            b.flat(
                "Cancel",
                "Cancel",
                [left, button_y, 100., 56.],
                HostAction::Pane(Pane::SpaceSettings),
            );
            b.destructive(
                "Delete",
                [left + width - 100., button_y, 100., 56.],
                HostAction::Reviewed(
                    Change::DeleteSpace {
                        space: space.id.clone(),
                        remove_data: model.remove_data,
                    },
                    generation,
                ),
            );
        }
        _ => {}
    }
}
