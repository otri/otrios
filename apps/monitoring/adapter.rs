// SPDX-License-Identifier: Apache-2.0
//! External Prometheus adapter; committed Monitoring data uses the shared runtime.
use otrios_core::canvas::monitoring::{self, Data, Metric, Window};
use serde_json::Value as Json;
use std::{fs, io::Read};
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
    if d["step"] != 3600 || end.checked_sub(start) != Some(7 * 86400) {
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
pub fn data(d: &Json) -> Result<Data, String> {
    let integer = |key: &str| d[key].as_i64().ok_or_else(|| format!("Missing {key}"));
    let label = |key: &str| {
        d[key]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| format!("Missing {key}"))
    };
    let result = Data {
        window: Window {
            generated: integer("generated")?,
            start: integer("start")?,
            end: integer("end")?,
            step: u32::try_from(integer("step")?).map_err(|_| "Invalid step")?,
            collected_label: label("collected_label")?,
            start_label: label("start_label")?,
            end_label: label("end_label")?,
        },
        metrics: monitoring::METRICS
            .iter()
            .map(|key| {
                Ok(Metric {
                    current: current(d, key)?,
                    history: grid(d, key)?,
                })
            })
            .collect::<Result<_, String>>()?,
    };
    monitoring::validate_data(&result)?;
    Ok(result)
}
pub fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--publish") {
        return super::monitoring_publish::run(&args[1..]);
    }
    if args.len() == 2 && args[0] == "--inspect" {
        let s = monitoring::store::load(std::path::Path::new(&args[1]))?;
        println!(
            "{}",
            serde_json::json!({"root":otrios_core::canvas::hex(&s.root()),"revision":s.head().revision,"generated":s.head().window.generated,"feed":s.head().feed,"producer":s.head().producer})
        );
        return Ok(());
    }

    if args.len() != 1 && !(args.len() == 4 && args[0] == "--collect") {
        return Err("Use dashboard-monitoring OUTPUT.flex or --collect SNAPSHOT FEED PRODUCER < prometheus-window.json; --publish SNAPSHOT SESSION IDENTITY".into());
    }
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("Monitoring input bound".into());
    }
    let input: Json = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let data = data(&input)?;
    if args.len() == 4 {
        let snapshot =
            monitoring::store::collect(std::path::Path::new(&args[1]), &args[2], &args[3], data)?;
        println!(
            "COMMITTED monitoring revision={} root={} generated={}",
            snapshot.head().revision,
            otrios_core::canvas::hex(&snapshot.root()),
            snapshot.head().window.generated
        );
        Ok(())
    } else {
        fs::write(&args[0], monitoring::project(&data)?).map_err(|e| e.to_string())
    }
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
        for key in monitoring::METRICS {
            d["queries"][key] =
                json!({"status":"success","data":{"resultType":"matrix","result":[]}});
        }
        let bytes = monitoring::project(&data(&d).unwrap()).unwrap();
        let scene = otrios_core::dashboard::layout::Layout::from_bytes(&bytes)
            .unwrap()
            .resolve_slots(&std::collections::BTreeMap::new())
            .unwrap();
        fn gather(node: &otrios_interfaces::scene::SceneNode, text: &mut Vec<String>) {
            if let otrios_interfaces::scene::Primitive::Text { value, .. } = &node.primitive {
                text.push(value.clone());
            }
            for child in &node.children {
                gather(child, text);
            }
        }
        let mut text = Vec::new();
        gather(scene.root(), &mut text);
        assert!(text.iter().any(|s| s == "NO DATA"));
        assert!(!text.iter().any(|s| s == "UP"));
    }
}
