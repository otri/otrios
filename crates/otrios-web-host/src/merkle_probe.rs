// SPDX-License-Identifier: Apache-2.0
//! Opt-in diagnostic adapter. Runtime implementation stays in the covered core.
use otrios_core::merkle::*;
use wasm_bindgen::prelude::*;
#[wasm_bindgen]
pub struct MerkleProbe {
    authority: Authority,
    subscription: Subscription,
    replica: Replica,
    nodes: u32,
    full_bytes: u32,
}
fn path(mut index: u64) -> Path {
    let mut p = vec![0; 4];
    for key in p.iter_mut().rev() {
        *key = index % 8;
        index /= 8;
    }
    p
}
#[wasm_bindgen]
impl MerkleProbe {
    #[wasm_bindgen(constructor)]
    pub fn new(leaves: u32) -> std::result::Result<Self, String> {
        fn create(leaves: u32) -> Result<MerkleProbe> {
            if leaves == 0 || leaves > 4096 {
                return Err(Error::Limit);
            }
            let mut tree = Snapshot::empty();
            for first in (0..leaves).step_by(MAX_EDITS) {
                let commands = (first..(first + MAX_EDITS as u32).min(leaves))
                    .map(|i| Edit {
                        path: path(u64::from(i)),
                        schema: [7; 32],
                        value: Some({
                            let mut value = vec![0; 1024];
                            value[..4].copy_from_slice(&i.to_le_bytes());
                            value
                        }),
                    })
                    .collect::<Vec<_>>();
                tree = tree.edit([1; 16], &commands)?.0;
            }
            let nodes = tree.node_count() as u32;
            let mut authority = Authority::new(1, tree);
            let mut subscription = authority.subscribe(&[vec![]], vec![])?;
            let mut replica = Replica::new(&subscription);
            let initial = authority.offer(&mut subscription)?.ok_or(Error::Missing)?;
            let full_bytes = initial.bytes().len() as u32;
            replica.apply(initial.bytes())?;
            subscription.acknowledge(initial.cursor, initial.root)?;
            Ok(MerkleProbe {
                authority,
                subscription,
                replica,
                nodes,
                full_bytes,
            })
        }
        create(leaves).map_err(|e| e.to_string())
    }
    /// One full edit/encode/decode/validate/ACK. No browser storage is used.
    pub fn step(&mut self, revision: u32) -> std::result::Result<Vec<u32>, String> {
        fn step(p: &mut MerkleProbe, revision: u32) -> Result<Vec<u32>> {
            let mut value = vec![0; 1024];
            value[..4].copy_from_slice(&revision.to_le_bytes());
            let (next, work) = p.authority.snapshot().edit(
                [1; 16],
                &[Edit {
                    path: path(0),
                    schema: [7; 32],
                    value: Some(value),
                }],
            )?;
            p.authority.publish(p.authority.snapshot().hash(), next)?;
            let offer = p
                .authority
                .offer(&mut p.subscription)?
                .ok_or(Error::Missing)?;
            p.replica.apply(offer.bytes())?;
            if p.replica.snapshot().map(Snapshot::hash) != offer.root {
                return Err(Error::Invalid);
            }
            p.subscription.acknowledge(offer.cursor, offer.root)?;
            Ok(vec![
                work.hashed_nodes as u32,
                work.hashed_bytes as u32,
                offer.bytes().len() as u32,
                p.nodes,
                p.full_bytes,
            ])
        }
        step(self, revision).map_err(|e| e.to_string())
    }
    pub fn root_hash(&self) -> Vec<u8> {
        self.authority.snapshot().hash().to_vec()
    }
}
