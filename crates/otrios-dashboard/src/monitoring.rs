// SPDX-License-Identifier: Apache-2.0
//! Prometheus JSON is an adapter input only. Hosts receive a complete typed binary primitive view.
use otrios_core::dashboard::{Replica, layout::Layout};
use otrios_interfaces::layout::*;
use serde_json::Value as Json;
use std::{fs, io::Read};
mod layout_seed;
const WHITE: [u8; 4] = [239, 244, 236, 255];
const MUTED: [u8; 4] = [150, 171, 185, 255];
const GREEN: [u8; 4] = [185, 238, 143, 255];
const BLUE: [u8; 4] = [112, 194, 244, 255];
const AMBER: [u8; 4] = [255, 199, 112, 255];
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
        let mut blueprint = layout_seed::blueprint();
        blueprint.templates.retain(|t| [1, 2, 3].contains(&t.id));
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
                id: 0,
                frame: [number(0.), number(0.), Expr::Width, Expr::Height],
                visible: literal(Value::Boolean(true)),
                shape: Shape::Canvas { clip: true },
                responder: None,
                semantics: None,
                children: vec![TemplateNode {
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
                id: 0,
                frame: [number(0.), number(0.), Expr::Width, Expr::Height],
                visible: literal(Value::Boolean(true)),
                shape: Shape::Canvas { clip: true },
                responder: None,
                semantics: None,
                children: vec![TemplateNode {
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
fn envelope<'a>(d: &'a Json, key: &str) -> Result<&'a Json, String> {
    let e = &d["queries"][key];
    if e["status"] != "success" || e["data"]["resultType"] != "matrix" {
        return Err(format!("Invalid query {key}"));
    }
    if e.get("warnings")
        .and_then(Json::as_array)
        .is_some_and(|v| !v.is_empty())
    {
        return Err(format!("Prometheus warning for {key}"));
    }
    Ok(e)
}
fn grid(d: &Json, key: &str) -> Result<Vec<Option<f64>>, String> {
    let start = d["start"].as_i64().ok_or("Missing start")?;
    let end = d["end"].as_i64().ok_or("Missing end")?;
    if d["step"] != 3600 || end - start != 7 * 86400 {
        return Err("Expected a seven-day hourly window".into());
    }
    let rows = envelope(d, key)?["data"]["result"]
        .as_array()
        .ok_or("Missing matrix")?;
    if rows.len() > 1 {
        return Err(format!(
            "Ambiguous series for {key}; select an instance explicitly"
        ));
    }
    let mut values = vec![None; 169];
    if let Some(row) = rows.first() {
        for p in row["values"].as_array().ok_or("Missing values")? {
            let t = p[0].as_f64().ok_or("Invalid timestamp")?;
            let n = p[1]
                .as_str()
                .ok_or("Invalid value")?
                .parse::<f64>()
                .map_err(|e| e.to_string())?;
            let offset = t - start as f64;
            if !(0. ..=604800.).contains(&offset) || offset % 3600. != 0. {
                return Err("Off-grid sample".into());
            }
            if n.is_finite() && n >= 0. {
                values[(offset / 3600.) as usize] = Some(n);
            }
        }
    }
    Ok(values)
}
fn current(d: &Json, key: &str) -> Result<Option<f64>, String> {
    let Some(e) = d.get("current").and_then(|v| v.get(key)) else {
        return Ok(None);
    };
    if e["status"] != "success"
        || e["data"]["resultType"] != "vector"
        || e.get("warnings")
            .and_then(Json::as_array)
            .is_some_and(|v| !v.is_empty())
    {
        return Err(format!("Invalid current query {key}"));
    }
    let rows = e["data"]["result"]
        .as_array()
        .ok_or("Missing current vector")?;
    if rows.len() > 1 {
        return Err(format!("Ambiguous current series {key}"));
    }
    let Some(row) = rows.first() else {
        return Ok(None);
    };
    let timestamp = row["value"][0]
        .as_f64()
        .ok_or("Missing current timestamp")?;
    let generated = d["generated"].as_i64().ok_or("Missing collection time")? as f64;
    if (timestamp - generated).abs() > 120. {
        return Err("Current query timestamp differs from collection time".into());
    }
    let n = row["value"][1]
        .as_str()
        .ok_or("Missing current value")?
        .parse::<f64>()
        .map_err(|e| e.to_string())?;
    Ok((n.is_finite() && n >= 0.).then_some(n))
}
fn formatted(v: Option<f64>, suffix: &str) -> String {
    v.map(|v| format!("{v:.0}{suffix}"))
        .unwrap_or_else(|| "NO DATA".into())
}
fn build(d: &Json) -> Result<Vec<u8>, String> {
    let stamp = d["collected_label"]
        .as_str()
        .ok_or("Missing collection label")?;
    let start = d["start_label"].as_str().ok_or("Missing start label")?;
    let end = d["end_label"].as_str().ok_or("Missing end label")?;
    if [stamp, start, end]
        .iter()
        .any(|v| v.len() > 32 || !v.is_ascii())
    {
        return Err("Invalid date label".into());
    }
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
        let c = current(d, key)?;
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
        let samples = grid(d, key)?;
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
        let values = grid(d, key)?
            .into_iter()
            .map(|v| v.map(|n| if *key == "memory" { n / 1048576. } else { n }))
            .collect::<Vec<_>>();
        let latest = current(d, key)?.map(|n| if *key == "memory" { n / 1048576. } else { n });
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
    let db = current(d, "database")?;
    let en = current(d, "posts_en")?;
    let ja = current(d, "posts_ja")?;
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
        ("APACHE BUSY WORKERS", formatted(current(d, "workers")?, "")),
        (
            "POSTS / EN + JA",
            format!("{} + {}", formatted(en, ""), formatted(ja, "")),
        ),
        (
            "DAYS SINCE BACKUP",
            current(d, "backup_days")?
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
    let mut replica = Replica::new()?;
    replica.set_presentation(Some(&bytes))?;
    Ok(bytes)
}
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let output = args
        .next()
        .ok_or("Usage: dashboard-monitoring OUTPUT.flex < prometheus-window.json")?;
    if args.next().is_some() {
        return Err("Unexpected argument".into());
    }
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("Monitoring input too large".into());
    }
    let data: Json = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    fs::write(output, build(&data)?).map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn sparse_data_keeps_time_positions_and_ambiguity_is_rejected() {
        let mut d = json!({"start":0,"end":604800,"step":3600,"queries":{"x":{"status":"success","data":{"resultType":"matrix","result":[{"values":[[0,"0"],[7200,"NaN"],[10800,"9"]]}]}}}});
        let g = grid(&d, "x").unwrap();
        assert_eq!(g[0], Some(0.));
        assert_eq!(g[1], None);
        assert_eq!(g[2], None);
        assert_eq!(g[3], Some(9.));
        d["queries"]["x"]["data"]["result"]
            .as_array_mut()
            .unwrap()
            .push(json!({"values":[]}));
        assert!(grid(&d, "x").is_err());
    }
    #[test]
    fn an_unavailable_metric_is_not_replaced_by_demo_or_old_values() {
        let mut d = json!({"start":0,"end":604800,"step":3600,"generated":604800,
            "collected_label":"Jan 08 00:00 UTC","start_label":"Jan 01 00:00 UTC","end_label":"Jan 08 00:00 UTC","queries":{},"current":{}});
        for key in [
            "prod_health",
            "rc_health",
            "staging_health",
            "wordpress_health",
            "bundles",
            "memory",
        ] {
            d["queries"][key] =
                json!({"status":"success","data":{"resultType":"matrix","result":[]}});
        }
        let bytes = build(&d).unwrap();
        let mut replica = Replica::new().unwrap();
        replica.set_presentation(Some(&bytes)).unwrap();
        fn gather(node: &otrios_interfaces::scene::SceneNode, text: &mut Vec<String>) {
            if let otrios_interfaces::scene::Primitive::Text { value, .. } = &node.primitive {
                text.push(value.clone());
            }
            for child in &node.children {
                gather(child, text);
            }
        }
        let mut text = Vec::new();
        gather(replica.snapshot().scene().root(), &mut text);
        assert!(text.iter().any(|s| s == "NO DATA"));
        assert!(!text.iter().any(|s| s == "UP"));
    }
    #[test]
    fn gaps_split_geometry_without_compressing_the_time_axis() {
        let mut v = View::new();
        v.plot(
            [10., 20., 100., 50.],
            &[Some(0.), Some(1.), None, Some(2.), Some(3.)],
            4.,
            GREEN,
        );
        assert_eq!(v.blueprint.root.children.len(), 2);
        assert_eq!(v.blueprint.root.children[0].frame, [10., 20., 25., 50.]);
        assert_eq!(v.blueprint.root.children[1].frame, [85., 20., 25., 50.]);
    }
}
