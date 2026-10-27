// SPDX-License-Identifier: Apache-2.0
//! Thin offline driver over the shared runtime trace player. Never opens a device
//! or authority; each repetition mounts fresh local packages and RAM-only state.
use otrios_core::{
    canvas::{Client, Result, decode_catalog, decode_ui, touch_trace::Trace},
    dashboard::{font::Font, font_reference},
};
use std::{io::Read, path::PathBuf, sync::Arc, time::Instant};
fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(
        args.next()
            .ok_or("Supply TRACE.json [APPS_DIRECTORY] [REPEATS]")?,
    );
    let root = PathBuf::from(args.next().unwrap_or("apps".into()));
    let repeats: usize = args
        .next()
        .unwrap_or("20".into())
        .parse()
        .map_err(|_| "Invalid repeat count")?;
    if !(1..=100).contains(&repeats) {
        return Err("Repeat limit".into());
    }
    let mut raw = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut raw)
        .map_err(|e| e.to_string())?;
    if raw.len() > 2 * 1024 * 1024 {
        return Err("Trace byte limit".into());
    }
    let trace: Trace = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    trace.validate()?;
    let catalog = std::fs::read(root.join("catalog.flex")).map_err(|e| e.to_string())?;
    let entry = decode_catalog(&catalog)?
        .apps
        .into_iter()
        .find(|e| e.path == trace.screen)
        .ok_or("Trace screen is absent from this catalog")?;
    let package = root.join(&entry.app.id).join("package");
    let ui = std::fs::read(package.join("ui.flex")).map_err(|e| e.to_string())?;
    let code = if decode_ui(&ui)?.behavior.is_some() {
        std::fs::read(package.join("behavior.wasm")).map_err(|e| e.to_string())?
    } else {
        vec![]
    };
    let font = Arc::new(Font::from_zstd(
        include_bytes!("../../../web/shared/fonts/Roboto-Regular.otq.zst"),
        font_reference::RESOURCE_SHA256,
    )?);
    let mut results = vec![];
    let mut control_bounds = vec![];
    for batch_ms in [0, 16, 64] {
        let mut times = vec![];
        let mut output = None;
        for _ in 0..repeats {
            let mut client = Client::default();
            client.set_viewport(trace.viewport)?;
            client.admit_catalog(&catalog)?;
            client.mount_local(&ui, &entry.app.state_ref, &code, &[])?;
            client.set_font(font.clone());
            for _ in 0..100_000 {
                client.tick(0, 0)?;
                if client
                    .resource_usage()
                    .text_document
                    .as_ref()
                    .is_none_or(|s| s.extent_exact && !s.loading)
                {
                    break;
                }
            }
            if control_bounds.is_empty() {
                let scene = client.scene()?;
                control_bounds=scene.controls().iter().filter_map(|c|c.node.semantics.as_ref().map(|s|serde_json::json!({"label":s.label,"bounds":[c.origin[0],c.origin[1],c.node.frame.width*c.scale,c.node.frame.height*c.scale]}))).collect();
            }
            let start = Instant::now();
            let replay = trace.replay(&mut client, batch_ms)?;
            times.push(start.elapsed().as_micros() as u64);
            output = Some(replay);
        }
        times.sort_unstable();
        results.push(serde_json::json!({"batch_ms":batch_ms,"repeats":repeats,"cpu_p50_us":times[times.len()/2],"cpu_p95_us":times[(times.len()-1)*95/100],"replay":output}));
    }
    println!("{}",serde_json::to_string_pretty(&serde_json::json!({"screen":trace.screen,"control":trace.control,"source":trace.source,"results":results,"control_bounds":control_bounds})).map_err(|e|e.to_string())?);
    Ok(())
}
