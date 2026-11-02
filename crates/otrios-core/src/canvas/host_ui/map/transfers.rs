// SPDX-License-Identifier: LGPL-3.0-only
//! One shared panel for code publication, upload review and exact data replacement.
use super::*;
use crate::canvas::host_ui::transfers::{Purpose, Step};
use otrios_interfaces::{spaces::Audience, transfers::TransferTarget};
fn paragraph(b: &mut Builder<'_>, value: &str, x: f32, y: &mut f32, width: f32) {
    // Full bounded review/link text, rather than the map's two-line captions.
    for line in wrapped(value, |s| {
        s.chars().count() <= 90
            && b.font
                .map_or(s.chars().count() as f32 * 12., |f| f.text_width(s, 1.))
                <= width
    }) {
        b.nodes.push(text(&line, [x, *y, width, 28.], 1.));
        *y += 30.;
    }
}
fn wrapped(value: &str, fits: impl Fn(&str) -> bool) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in value.split_whitespace() {
        let candidate = if line.is_empty() {
            word.into()
        } else {
            format!("{line} {word}")
        };
        if fits(&candidate) {
            line = candidate;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        for ch in word.chars() {
            if !line.is_empty() && !fits(&format!("{line}{ch}")) {
                lines.push(std::mem::take(&mut line));
            }
            line.push(ch);
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
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
    let draft = model.transfer.clone()?;
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
                        if model.transfer_caps.export {
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
                                        TransferTarget::Data { dataset } => {
                                            dataset.collection.clone()
                                        }
                                        _ => String::new(),
                                    },
                                ),
                            );
                        }
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
                for file in files {
                    b.flat(
                        &format!("Choose {}", file.name),
                        &short(&file.name, 44),
                        [left, y, width, 56.],
                        HostAction::TransferFile(file.id.clone()),
                    );
                    y += 64.;
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

#[cfg(test)]
mod tests {
    use super::wrapped;
    #[test]
    fn complete_review_words_and_long_unicode_titles_fit_without_splitting_utf8() {
        let title = "共有データ🗺️".repeat(16);
        let url = format!("https://example.org/apps/counter/{}", "a".repeat(64));
        for value in [title, url, "Canonical name and reviewed destination".into()] {
            let lines = wrapped(&value, |s| s.chars().count() <= 19);
            assert!(
                lines
                    .iter()
                    .all(|s| !s.is_empty() && s.chars().count() <= 19)
            );
            assert_eq!(
                lines
                    .concat()
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>(),
                value
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
            );
            assert!(lines.iter().all(|s| !s.contains('\u{fffd}')));
        }
    }
}
