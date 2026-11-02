// SPDX-License-Identifier: LGPL-3.0-only
//! One shared panel for code publication, upload review and exact data replacement.
use super::*;
use crate::canvas::host_ui::transfers::{Purpose, Step};
use otrios_interfaces::{spaces::Audience, transfers::TransferTarget};
fn paragraph(b: &mut Builder<'_>, value: &str, x: f32, y: &mut f32, width: f32) {
    // Full bounded review/link text, rather than the map's two-line captions.
    let mut line = String::new();
    for word in value.split_whitespace() {
        if !line.is_empty()
            && (line.len() + word.len() + 1 > 64
                || b.font
                    .is_some_and(|f| f.text_width(&format!("{line} {word}"), 1.) > width))
        {
            b.nodes.push(text(&line, [x, *y, width, 28.], 1.));
            *y += 30.;
            line.clear();
        }
        if word.len() > 64 {
            if !line.is_empty() {
                b.nodes.push(text(&line, [x, *y, width, 28.], 1.));
                *y += 30.;
                line.clear();
            }
            for bytes in word.as_bytes().chunks(48) {
                // URLs and digests are ASCII.
                let chunk = String::from_utf8_lossy(bytes);
                b.nodes.push(text(&chunk, [x, *y, width, 28.], 1.));
                *y += 30.;
            }
        } else {
            if !line.is_empty() {
                line.push(' ');
            }
            line.push_str(word);
        }
    }
    if !line.is_empty() {
        b.nodes.push(text(&line, [x, *y, width, 28.], 1.));
        *y += 30.;
    }
}
fn audience(value: Audience) -> &'static str {
    match value {
        Audience::Closed => "Closed",
        Audience::PublicView => "Public view",
        Audience::PublicComment => "Public view and comment",
    }
}
pub(super) fn render(
    b: &mut Builder<'_>,
    snapshot: &Discovery,
    model: &mut Model,
    geometry: [f32; 4],
) -> Option<String> {
    let [left, top, width, _] = geometry;
    let Some(draft) = model.transfer.clone() else {
        return None;
    };
    let title = match draft.purpose {
        Purpose::Share => "Share App",
        Purpose::Upload => "Upload App",
        Purpose::Replace => "Replace Data",
        Purpose::Export => "Export Data",
    };
    b.nodes.push(text(title, [left, top, width, 40.], 2.));
    let mut y = top + 60.;
    let mut entry = None;
    let current = draft.current(snapshot);
    if !current
        && !matches!(
            draft.step,
            Step::Shared(_) | Step::Exported(_) | Step::Waiting { .. }
        )
    {
        paragraph(
            b,
            "The Space or app changed. Close this review and check the current details.",
            left,
            &mut y,
            width,
        );
    } else {
        match &draft.step {
            Step::Input => {
                paragraph(b, &format!("Space: {}", draft.name), left, &mut y, width);
                match draft.purpose {
                    Purpose::Upload => {
                        paragraph(
                            b,
                            "Choose an app payload or paste its shared HTTPS page URL. Receiving it does not install or trust code.",
                            left,
                            &mut y,
                            width,
                        );
                        let key = format!("transfer:url:{}", draft.space);
                        let value = if model.field_owner.as_deref() == Some(&key) {
                            None
                        } else {
                            model.field_owner = Some(key);
                            Some(String::new())
                        };
                        b.entry("Shared app URL", [left, y + 12., width, 64.]);
                        y += 88.;
                        b.flat(
                            "Review URL",
                            "Review URL",
                            [left, y, width, 56.],
                            HostAction::TransferUrl,
                        );
                        y += 64.;
                        entry = Some(value.unwrap_or_else(|| model.form_value.clone()));
                    }
                    Purpose::Replace => paragraph(
                        b,
                        "Choose a compatible .otdata file. The current data remains intact until you review and confirm the replacement.",
                        left,
                        &mut y,
                        width,
                    ),
                    Purpose::Export => {
                        paragraph(
                            b,
                            "Download a complete confirmed snapshot, including owned assets. Unconfirmed local drafts are excluded.",
                            left,
                            &mut y,
                            width,
                        );
                        b.flat(
                            "Download Data",
                            "Download Data",
                            [left, y + 12., width, 56.],
                            HostAction::TransferExport(
                                match &draft.target {
                                    TransferTarget::Data { dataset } => dataset.app.clone(),
                                    _ => String::new(),
                                },
                                match &draft.target {
                                    TransferTarget::Data { dataset } => dataset.collection.clone(),
                                    _ => String::new(),
                                },
                            ),
                        );
                        y += 76.;
                    }
                    _ => (),
                }
                if draft.purpose != Purpose::Export {
                    if model.transfer_caps.picker {
                        b.flat(
                            "Choose File",
                            "Choose File",
                            [left, y + 12., width, 56.],
                            HostAction::TransferPick,
                        );
                        y += 76.;
                    } else if draft.purpose == Purpose::Replace {
                        paragraph(
                            b,
                            "File selection is unavailable on this host. Open this Space in a browser to choose a data payload.",
                            left,
                            &mut y,
                            width,
                        );
                    }
                }
            }
            Step::Waiting { .. } => paragraph(
                b,
                if draft.purpose == Purpose::Export {
                    "Preparing confirmed data…"
                } else {
                    "Receiving and verifying the payload…"
                },
                left,
                &mut y,
                width,
            ),
            Step::Files { files, .. } => {
                paragraph(b, "Transfers", left, &mut y, width);
                if files.is_empty() {
                    paragraph(
                        b,
                        "No compatible transfer files are available here.",
                        left,
                        &mut y,
                        width,
                    );
                }
                model.page = model.page.min(files.len().saturating_sub(1) / 4);
                for file in files.iter().skip(model.page * 4).take(4) {
                    b.flat(
                        &format!("Choose {}", file.name),
                        &short(&file.name, 44),
                        [left, y, width, 56.],
                        HostAction::TransferFile(file.id.clone()),
                    );
                    y += 64.;
                }
                if model.page > 0 {
                    b.flat("Previous", "‹", [left, y, 80., 56.], HostAction::Page(-1));
                }
                if (model.page + 1) * 4 < files.len() {
                    b.flat("Next", "›", [left + 96., y, 80., 56.], HostAction::Page(1));
                }
                y += 64.;
            }
            Step::Share(code) | Step::Publishing(code) => {
                paragraph(b, &code.title, left, &mut y, width);
                paragraph(b, &code.app, left, &mut y, width);
                paragraph(
                    b,
                    "Anyone with the link can download this app's code. Space data, account credentials and access grants are excluded. This publishes an independent copy; changing Space access will not recall it.",
                    left,
                    &mut y,
                    width,
                );
                if matches!(draft.step, Step::Share(_)) {
                    b.flat(
                        "Publish App",
                        "Publish App",
                        [left, y + 16., width, 56.],
                        HostAction::TransferCommit,
                    );
                    y += 80.;
                } else {
                    paragraph(b, "Publishing…", left, &mut y, width);
                }
            }
            Step::Shared(url) => {
                paragraph(
                    b,
                    "Anyone with this link can download the app. Data is excluded.",
                    left,
                    &mut y,
                    width,
                );
                paragraph(b, url, left, &mut y, width);
                if model.transfer_caps.clipboard {
                    b.flat(
                        "Copy link",
                        "Copy link",
                        [left, y + 12., width, 56.],
                        HostAction::TransferCopy,
                    );
                    y += 76.;
                }
            }
            Step::App(review) => {
                paragraph(b, &review.title, left, &mut y, width);
                paragraph(
                    b,
                    &format!("{} · revision {}", review.app, review.revision),
                    left,
                    &mut y,
                    width,
                );
                paragraph(
                    b,
                    &format!(
                        "Destination: {} · {}",
                        draft.name,
                        audience(review.audience)
                    ),
                    left,
                    &mut y,
                    width,
                );
                if review.installed.as_deref() == Some(review.digest.as_str()) {
                    paragraph(
                        b,
                        "Reuse verified installed code and attach it to this Space. Existing data is preserved.",
                        left,
                        &mut y,
                        width,
                    );
                } else {
                    paragraph(
                        b,
                        if review.installed.is_some() {
                            "This is a host-wide app update. Confirm this version change for every Space using the installed app. Existing data and other attachments are preserved."
                        } else {
                            "Install this verified app code and attach it to the destination Space. This does not grant access to any other Space."
                        },
                        left,
                        &mut y,
                        width,
                    );
                    if review.affected_spaces > 0 {
                        paragraph(
                            b,
                            &format!("Affected Spaces: {}", review.affected_spaces),
                            left,
                            &mut y,
                            width,
                        );
                    }
                }
                b.flat(
                    "Confirm Import",
                    "Confirm Import",
                    [left, y + 12., width, 56.],
                    HostAction::TransferCommit,
                );
                y += 76.;
            }
            Step::Data(review) => {
                paragraph(b, &review.replacement.app, left, &mut y, width);
                paragraph(
                    b,
                    &format!(
                        "Destination: {} · {}",
                        draft.name,
                        audience(review.audience)
                    ),
                    left,
                    &mut y,
                    width,
                );
                paragraph(
                    b,
                    "The selected dataset remains available as retained data. The imported content receives a fresh collection identity; former mounts cannot write into it.",
                    left,
                    &mut y,
                    width,
                );
                b.flat(
                    "Confirm Replacement",
                    "Confirm Replacement",
                    [left, y + 16., width, 56.],
                    HostAction::TransferCommit,
                );
                y += 80.;
            }
            Step::Exported(name) => {
                paragraph(b, name, left, &mut y, width);
            }
        }
    }
    b.flat(
        "Close",
        "Close",
        [left, y + 24., 120., 56.],
        HostAction::TransferCancel,
    );
    entry
}
