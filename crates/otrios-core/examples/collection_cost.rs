// SPDX-License-Identifier: LGPL-3.0-only
//! Bounded-window cost probe with independently allocated record bodies and a
//! separate replica process. RSS is sampled with ps; canonical bytes != RSS.
use otrios_core::{
    collection::*,
    merkle::{Edit, Snapshot},
};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    process::{Child, Command as Process, Stdio},
};

fn schema() -> Schema {
    Schema {
        id: [24; 32],
        fields: BTreeMap::from([(1, [1; 32])]),
    }
}
fn plan() -> Selection {
    Selection {
        collection: 24,
        schema: [24; 32],
        anchor: Anchor::Around(192),
        record_limit: 16,
        byte_limit: 8192,
        fields: vec![1],
    }
}
fn rss(pid: u32) -> usize {
    let output = Process::new("ps")
        .args(["-o", "rss=", "-p", &pid.to_string()])
        .output()
        .expect("ps");
    assert!(output.status.success());
    String::from_utf8(output.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}
fn read_frame(input: &mut impl Read) -> Vec<u8> {
    let mut size = [0; 4];
    input.read_exact(&mut size).unwrap();
    let size = u32::from_le_bytes(size) as usize;
    assert!(size <= 1024 * 1024 + 256);
    let mut data = vec![0; size];
    input.read_exact(&mut data).unwrap();
    data
}
fn write_frame(output: &mut impl Write, bytes: &[u8]) {
    output
        .write_all(&(bytes.len() as u32).to_le_bytes())
        .unwrap();
    output.write_all(bytes).unwrap();
    output.flush().unwrap();
}
fn worker() {
    let mut input = std::io::stdin().lock();
    let mut output = std::io::stdout().lock();
    for generation in 1..=3 {
        let bytes = read_frame(&mut input);
        let offer = Offer::from_bytes(&bytes).unwrap();
        let mut replica = Replica::new(71, 24, schema());
        replica.begin_request(generation, plan()).unwrap();
        replica.apply(&offer).unwrap();
        let window = replica.window().unwrap();
        let mut result = (rss(std::process::id()) as u64).to_le_bytes().to_vec();
        result.extend((window.snapshot().canonical_bytes() as u64).to_le_bytes());
        result.extend(window.snapshot().hash());
        write_frame(&mut output, &result);
    }
}
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--replica") {
        worker();
        return;
    }
    let mut child = ChildGuard(
        Process::new(std::env::current_exe().unwrap())
            .arg("--replica")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let mut c = Collection::new(24, schema(), Budget::default()).unwrap();
    let grant = Grant {
        collection: 24,
        fields: vec![1],
    };
    let mut generation = 0;
    println!(
        "records,index_pages_visited,records_considered,collection_canonical_bytes,selected_bytes,view_canonical_bytes,wire_bytes,authority_rss_kib,replica_rss_kib"
    );
    let mut view_hash = None;
    for count in 1..=65_536_u64 {
        let value = format!("record-{count:08} {}", "x".repeat(112));
        let row = Snapshot::empty()
            .edit(
                [1; 16],
                &[Edit {
                    path: vec![1],
                    schema: [1; 32],
                    value: Some(value.into_bytes()),
                }],
            )
            .unwrap()
            .0;
        c = c.append(count * 3, row).unwrap();
        if ![128, 4096, 65_536].contains(&count) {
            continue;
        }
        generation += 1;
        let window = c.select(71, &grant, &plan()).unwrap();
        let mut authority = Authority::new(71, c.clone());
        let mut lease = authority
            .subscribe(grant.clone(), generation, plan())
            .unwrap();
        let offer = authority.offer(&mut lease).unwrap().unwrap();
        let bytes = offer.to_bytes().unwrap();
        write_frame(child.0.stdin.as_mut().unwrap(), &bytes);
        let result = read_frame(child.0.stdout.as_mut().unwrap());
        assert_eq!(result.len(), 48);
        let replica_rss = u64::from_le_bytes(result[..8].try_into().unwrap());
        let replica_bytes = u64::from_le_bytes(result[8..16].try_into().unwrap());
        assert_eq!(replica_bytes as usize, window.snapshot().canonical_bytes());
        if let Some(hash) = view_hash {
            assert_eq!(hash, result[16..]);
        }
        view_hash = Some(<[u8; 32]>::try_from(&result[16..]).unwrap());
        println!(
            "{count},{},{},{},{},{},{},{},{replica_rss}",
            window.work.pages_visited,
            window.work.records_considered,
            c.canonical_bytes(),
            window.selected_bytes,
            window.snapshot().canonical_bytes(),
            bytes.len(),
            rss(std::process::id())
        );
    }
    assert!(child.0.wait().unwrap().success());
}
