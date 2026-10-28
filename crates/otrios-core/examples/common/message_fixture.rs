// SPDX-License-Identifier: LGPL-3.0-only
//! Small deterministic source fixture shared by pixel and kernel-flip probes.
use otrios_core::{
    canvas::{
        chat::icons,
        document::{Block, BlockId, Inline, Source},
    },
    dashboard::{render_scene, scene::Scene},
};
use std::{collections::BTreeMap, sync::Arc};
pub struct Sample {
    pub id: BlockId,
    pub text: String,
    pub avatar: Option<u8>,
    pub anchor: Option<BlockId>,
}
pub fn samples(first: u64, count: u64) -> Result<Vec<Sample>, String> {
    if !(1..=64).contains(&count) {
        return Err("Fixture requires 1..=64 messages".into());
    }
    let end = first
        .checked_add(count)
        .ok_or("Fixture identity overflow")?;
    let first_id = BlockId {
        object: first,
        part: 0,
    };
    let mut samples = vec![Sample {
        id: BlockId {
            object: first,
            part: 2,
        },
        text: String::new(),
        avatar: None,
        anchor: Some(first_id),
    }];
    for id in first..end {
        let key = BlockId {
            object: id,
            part: 0,
        };
        if id == first || id % 8 == 0 {
            samples.push(Sample {
                id: BlockId {
                    object: id,
                    part: 1,
                },
                text: "2026-10-28".into(),
                avatar: None,
                anchor: Some(key),
            });
        }
        samples.push(Sample {id:key,text:format!("17:40  Captain Sprocket {id}\njgyÉ, tabs\tand  spaces. {}\nA hard break, unsupported 👩‍💻 and combining á.\n\n","A brass paragraph wraps at stable source positions. ".repeat((id%3+1) as usize)),avatar:Some((id%64) as u8),anchor:None});
    }
    Ok(samples)
}
pub fn source(samples: &[Sample]) -> Result<Source, String> {
    let mut avatars: BTreeMap<u8, Arc<Inline>> = BTreeMap::new();
    Source::from_blocks(
        samples
            .iter()
            .map(|sample| {
                if let Some(target) = sample.anchor {
                    return Block::label(sample.id, sample.text.clone(), 64., target);
                }
                let inline = if let Some(id) = sample.avatar {
                    if let Some(inline) = avatars.get(&id) {
                        Some(inline.clone())
                    } else {
                        let mesh = render_scene(&Scene::new(icons::avatar(id, 1, 32.))?);
                        let inline = Inline::new(mesh.vertices, [12., -4.])?;
                        avatars.insert(id, inline.clone());
                        Some(inline)
                    }
                } else {
                    None
                };
                Block::new(sample.id, sample.text.clone(), 64., inline)
            })
            .collect::<Result<Vec<_>, String>>()?,
    )
}
