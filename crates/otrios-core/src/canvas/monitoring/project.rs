// SPDX-License-Identifier: LGPL-3.0-only
//! Shared chart projection from the Monitoring app's typed data.
use super::*;
use crate::dashboard::layout::Layout;
use otrios_interfaces::layout::*;
#[path = "../../../../../apps/monitoring/view_templates.rs"]
mod view_templates;
use otrios_interfaces::theme::{BRASS as GREEN, COPPER as BLUE, MUTED, TEXT as WHITE};
const AMBER: [u8; 4] = [235, 174, 120, 255];
fn literal(v: Value) -> Expr {
    Expr::Literal(v)
}
fn number(v: f32) -> Expr {
    literal(Value::Number(v))
}
fn color(v: [u8; 4]) -> Expr {
    literal(Value::Color(v))
}
struct View {
    blueprint: Blueprint,
    next: u32,
}
impl View {
    fn new() -> Self {
        let mut blueprint = view_templates::blueprint();
        blueprint.templates.push(Template {
            id: 6,
            name: "Trend line".into(),
            parameters: vec![
                Parameter {
                    key: 1,
                    name: "samples".into(),
                    value_type: ValueType::Series,
                    default: literal(Value::Series(vec![])),
                },
                Parameter {
                    key: 2,
                    name: "color".into(),
                    value_type: ValueType::Color,
                    default: color(GREEN),
                },
            ],
            root: TemplateNode {
                reference_root: false,
                layout: None,
                resize: None,
                id: 0,
                frame: [number(0.), number(0.), Expr::Width, Expr::Height],
                visible: literal(Value::Boolean(true)),
                shape: Shape::Canvas { clip: true },
                responder: None,
                semantics: None,
                children: vec![TemplateNode {
                    reference_root: false,
                    layout: None,
                    resize: None,
                    id: 1,
                    frame: [number(0.), number(0.), Expr::Width, Expr::Height],
                    visible: literal(Value::Boolean(true)),
                    shape: Shape::SeriesPath {
                        series: Expr::Parameter(1),
                        width: 2.,
                        color: Expr::Parameter(2),
                    },
                    responder: None,
                    semantics: None,
                    children: vec![],
                }],
            },
        });
        blueprint.templates.push(Template {
            id: 7,
            name: "Isolated sample".into(),
            parameters: vec![Parameter {
                key: 2,
                name: "color".into(),
                value_type: ValueType::Color,
                default: color(GREEN),
            }],
            root: TemplateNode {
                reference_root: false,
                layout: None,
                resize: None,
                id: 0,
                frame: [number(0.), number(0.), Expr::Width, Expr::Height],
                visible: literal(Value::Boolean(true)),
                shape: Shape::Canvas { clip: true },
                responder: None,
                semantics: None,
                children: vec![TemplateNode {
                    reference_root: false,
                    layout: None,
                    resize: None,
                    id: 1,
                    frame: [number(0.), number(0.), Expr::Width, Expr::Height],
                    visible: literal(Value::Boolean(true)),
                    shape: Shape::Circle {
                        color: Expr::Parameter(2),
                    },
                    responder: None,
                    semantics: None,
                    children: vec![],
                }],
            },
        });
        blueprint.root.children.clear();
        Self { blueprint, next: 1 }
    }
    fn add(&mut self, template: u16, frame: [f32; 4], properties: Vec<Property>) {
        self.blueprint.root.children.push(Instance {
            id: self.next,
            template,
            frame,
            properties,
            children: vec![],
        });
        self.next += 1;
    }
    fn text(&mut self, x: f32, y: f32, w: f32, s: &str, scale: f32, paint: [u8; 4]) {
        self.add(
            2,
            [x, y, w, 8. * scale],
            vec![
                Property {
                    key: 1,
                    value: literal(Value::Text(s.into())),
                },
                Property {
                    key: 2,
                    value: color(paint),
                },
                Property {
                    key: 3,
                    value: number(scale),
                },
            ],
        );
    }
    fn panel(&mut self, x: f32, y: f32, w: f32, h: f32) {
        self.add(3, [x, y, w, h], vec![]);
    }
    /// Missing slots split paths; each segment keeps its original position on the common time axis.
    fn plot(&mut self, frame: [f32; 4], values: &[Option<f64>], max: f64, paint: [u8; 4]) {
        let [x, y, w, h] = frame;
        let mut i = 0;
        while i < values.len() {
            if values[i].is_none() {
                i += 1;
                continue;
            }
            let start = i;
            while i < values.len() && values[i].is_some() {
                i += 1;
            }
            let count = i - start;
            if count < 2 {
                let px = x + w * start as f32 / (values.len() - 1) as f32;
                let py = y + h - (values[start].unwrap() / max) as f32 * h;
                self.add(
                    7,
                    [px - 2., py - 2., 4., 4.],
                    vec![Property {
                        key: 2,
                        value: color(paint),
                    }],
                );
                continue;
            }
            let left = w * start as f32 / (values.len() - 1) as f32;
            let width = w * (count - 1) as f32 / (values.len() - 1) as f32;
            let samples = values[start..i]
                .iter()
                .map(|v| (v.unwrap() / max * 100.).clamp(0., 100.) as f32)
                .collect();
            self.add(
                6,
                [x + left, y, width, h],
                vec![
                    Property {
                        key: 1,
                        value: literal(Value::Series(samples)),
                    },
                    Property {
                        key: 2,
                        value: color(paint),
                    },
                ],
            );
        }
    }
}
fn formatted(v: Option<f64>, suffix: &str) -> String {
    v.map(|v| format!("{v:.0}{suffix}"))
        .unwrap_or_else(|| "NO DATA".into())
}
pub fn build(d: &Data) -> Result<Vec<u8>> {
    validate_data(d)?;
    let stamp = &d.window.collected_label;
    let start = &d.window.start_label;
    let end = &d.window.end_label;
    let mut v = View::new();
    v.text(32., 24., 1216., "SPD & M3D / PROMETHEUS", 2., MUTED);
    v.text(32., 58., 800., "Service overview", 4., WHITE);
    v.text(880., 58., 368., "LAST 7 DAYS", 2., GREEN);
    v.text(880., 82., 368., &format!("AS OF {stamp}"), 1., MUTED);
    let health = [
        ("prod_health", "GALLERY PRODUCTION"),
        ("rc_health", "GALLERY RC"),
        ("staging_health", "GALLERY STAGING"),
        ("wordpress_health", "WORDPRESS"),
    ];
    v.panel(32., 254., 1216., 174.);
    v.text(
        56.,
        274.,
        840.,
        "SERVICE HEALTH / WORST SCRAPE EACH HOUR",
        2.,
        WHITE,
    );
    for (i, (key, label)) in health.iter().enumerate() {
        let x = 32. + i as f32 * 308.;
        v.panel(x, 120., 292., 114.);
        v.text(x + 20., 138., 252., label, 1., MUTED);
        let c = d.current(key)?;
        let up = c == Some(1.);
        v.text(
            x + 20.,
            160.,
            252.,
            if up {
                "UP"
            } else if c.is_some() {
                "DOWN"
            } else {
                "NO DATA"
            },
            3.,
            if up { GREEN } else { AMBER },
        );
        let samples = d.history(key)?;
        let known = samples.iter().flatten().count();
        let healthy = samples.iter().flatten().filter(|n| **n == 1.).count();
        let note = if known == 0 {
            "No history".into()
        } else {
            format!("{healthy}/{known} hourly windows clear")
        };
        v.text(x + 20., 208., 252., &note, 1., MUTED);
        let y = 312. + i as f32 * 22.;
        v.text(56., y, 192., label, 1., MUTED);
        v.plot([256., y + 1., 968., 12.], &samples, 1., GREEN);
    }
    v.text(256., 410., 400., start, 1., MUTED);
    v.text(1000., 410., 224., end, 1., MUTED);
    for (i, (key, title, unit, paint)) in [
        ("bundles", "GALLERY / BUNDLES", "", GREEN),
        ("memory", "GALLERY / HEAP USED", " MiB", BLUE),
    ]
    .iter()
    .enumerate()
    {
        let x = 32. + i as f32 * 616.;
        v.panel(x, 448., 600., 220.);
        v.text(x + 24., 466., 552., title, 2., MUTED);
        let values = d
            .history(key)?
            .into_iter()
            .map(|v| v.map(|n| if *key == "memory" { n / 1048576. } else { n }))
            .collect::<Vec<_>>();
        let latest = d
            .current(key)?
            .map(|n| if *key == "memory" { n / 1048576. } else { n });
        let max = values
            .iter()
            .flatten()
            .copied()
            .fold(latest.unwrap_or(0.), f64::max)
            .max(1.)
            * 1.1;
        v.text(x + 24., 494., 552., &formatted(latest, unit), 3., WHITE);
        v.text(
            x + 24.,
            534.,
            552.,
            &format!(
                "0 - {max:.0}{unit} / {}",
                if *key == "memory" {
                    "hourly mean"
                } else {
                    "hourly samples"
                }
            ),
            1.,
            MUTED,
        );
        v.plot([x + 24., 562., 552., 68.], &values, max, *paint);
        v.text(x + 24., 644., 400., start, 1., MUTED);
        v.text(x + 440., 644., 136., end, 1., MUTED);
    }
    let db = d.current("database")?;
    let en = d.current("posts_en")?;
    let ja = d.current("posts_ja")?;
    let details = [
        (
            "WORDPRESS DATABASE",
            if db == Some(1.) {
                "UP".into()
            } else if db.is_some() {
                "DOWN".into()
            } else {
                "NO DATA".into()
            },
        ),
        ("APACHE BUSY WORKERS", formatted(d.current("workers")?, "")),
        (
            "POSTS / EN + JA",
            format!("{} + {}", formatted(en, ""), formatted(ja, "")),
        ),
        (
            "DAYS SINCE BACKUP",
            d.current("backup_days")?
                .map(|n| format!("{n:.2}"))
                .unwrap_or_else(|| "NO DATA".into()),
        ),
    ];
    for (i, (label, value)) in details.iter().enumerate() {
        let x = 32. + i as f32 * 308.;
        v.panel(x, 688., 292., 74.);
        v.text(x + 16., 700., 260., label, 1., MUTED);
        v.text(x + 16., 724., 260., value, 2., WHITE);
    }
    v.text(
        32.,
        780.,
        1216.,
        "Hourly query grid / gaps = no sample / last collection above / refresh every 5 min",
        1.,
        MUTED,
    );
    let bytes = Layout::encode(&v.blueprint)?;
    Layout::from_bytes(&bytes)?.resolve_slots(&std::collections::BTreeMap::new())?;
    Ok(bytes)
}

pub fn empty() -> Result<Vec<u8>> {
    let mut v = View::new();
    v.text(32., 58., 1200., "Waiting for Monitoring data", 3., MUTED);
    Layout::encode(&v.blueprint)
}
