// SPDX-License-Identifier: LGPL-3.0-only
//! Create a new disposable durable room through the real single publication
//! owner. Never reads or overwrites an existing directory or live journal.
use otrios_core::{canvas::chat, collection};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Instant, SystemTime, UNIX_EPOCH},
};
fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let directory = PathBuf::from(
        args.next()
            .ok_or("Supply a NEW directory and record count")?,
    );
    let count = args
        .next()
        .ok_or("Supply record count")?
        .parse::<u64>()
        .map_err(|e| e.to_string())?;
    if args.next().is_some() || !(64..=50_000).contains(&count) {
        return Err("Record count must be 64..=50000".into());
    }
    std::fs::create_dir(&directory).map_err(|e| format!("Create NEW fixture directory: {e}"))?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis() as u64;
    let first = now.checked_sub(count * 1000).ok_or("Fixture clock range")?;
    let registry = Arc::new(Mutex::new(collection::Channel::new(24)));
    let started = Instant::now();
    let path = directory.join("brass-relay.journal");
    let mut room = chat::Room::open_in(Some(path.clone()), first, registry.clone())?;
    room.join(1, None, false, first, true)?;
    room.handle(
        1,
        chat::Command::Profile {
            name: "Clockwork Archivist".into(),
            avatar: 7,
        },
        first,
    )?;
    for i in 1..=count {
        let at = first + i * 1000;
        let nonce = format!("{i:032x}");
        let events=room.handle(1,chat::Command::Send {epoch:at/600_000,nonce:nonce.clone(),text:format!("Dispatch {i:06}. The brass relay retains immutable source anchors.\nTwo independent paragraphs.")},at)?;
        if !events
            .iter()
            .any(|e| matches!(&e.event,chat::Event::Receipt {nonce:n,..} if *n==nonce))
        {
            return Err(format!("Fixture send {i} was not acknowledged: {events:?}"));
        }
        if i % 1000 == 0 {
            eprintln!("Fixture committed {i}/{count}");
        }
    }
    room.leave(1, now)?;
    let collection = registry
        .lock()
        .map_err(|_| "Fixture registry poisoned")?
        .collection(chat::COLLECTION)
        .map_err(|e| format!("{e:?}"))?
        .clone();
    let report = serde_json::json!({"records_requested":count,"records_retained":collection.len(),"canonical_bytes":collection.canonical_bytes(),"journal_bytes":std::fs::metadata(path).map_err(|e|e.to_string())?.len(),"elapsed_ms":started.elapsed().as_millis(),"root":otrios_core::canvas::hex(&collection.revision()),"fixture_only":true});
    drop(room); // Release the stable owner lock before starting another process.
    std::fs::write(directory.join("fixture.json"), report.to_string())
        .map_err(|e| e.to_string())?;
    println!("{report}");
    Ok(())
}
