// SPDX-License-Identifier: LGPL-3.0-only
//! Shared ink lives in the app Merkle tree: one head and one leaf per segment.
use super::*;
use otrios_interfaces::drawing::{COORDINATE_LIMIT, Command, MAX_POINTS, MAX_SEGMENTS, Stroke};
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Head {
    epoch: [u8; 16],
    cursor: u16,
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    owner: [u8; 32],
    stroke: Stroke,
}
#[derive(Clone, Debug, Default)]
pub struct Drawing {
    pub epoch: [u8; 16],
    pub strokes: Vec<Stroke>,
}
pub(super) fn command_bytes(command: &Command) -> usize {
    match command {
        Command::Put(s) => 32 + s.points.len() * 8,
        Command::Clear { .. } => 32,
    }
}
pub fn validate_command(command: &Command) -> Result<()> {
    match command {
        Command::Put(s)
            if s.id == [0; 16]
                || s.points.is_empty()
                || s.points.len() > MAX_POINTS
                || s.points
                    .iter()
                    .flatten()
                    .any(|n| n.unsigned_abs() > COORDINATE_LIMIT as u32) =>
        {
            Err("Drawing segment exceeds bounds".into())
        }
        Command::Clear { expected, next } if expected == next || *next == [0; 16] => {
            Err("Drawing Clear requires a fresh epoch".into())
        }
        _ => Ok(()),
    }
}
impl Drawing {
    fn apply(&mut self, command: &Command) {
        match command {
            Command::Clear { expected, next } if self.epoch == *expected => {
                self.epoch = *next;
                self.strokes.clear();
            }
            Command::Put(stroke) if stroke.epoch == self.epoch => {
                if let Some(old) = self.strokes.iter_mut().find(|s| s.id == stroke.id) {
                    if stroke.points.starts_with(&old.points) {
                        *old = stroke.clone();
                    }
                } else {
                    if self.strokes.len() == MAX_SEGMENTS {
                        self.strokes.remove(0);
                    }
                    self.strokes.push(stroke.clone());
                }
            }
            _ => {}
        }
    }
}
impl Document {
    pub fn configure_drawing(&mut self, ui: &Ui) -> Result<bool> {
        self.drawing_enabled = matches!(ui.presentation,Some(Presentation::TouchPad{ink:Some(options),..})if options.drawing.is_some());
        if !self.drawing_enabled {
            return Ok(false);
        }
        if payload(&self.tree, &path(5, 0)).is_ok() {
            return Ok(false);
        }
        self.change(
            &[],
            vec![Edit {
                path: path(5, 0),
                schema: self.extra_schema(5, 0),
                value: Some(bytes(&Head {
                    epoch: [0; 16],
                    cursor: 0,
                })?),
            }],
        )
    }
    pub fn drawing(&self) -> Result<Drawing> {
        let head: Head = decode(&payload(&self.tree, &path(5, 0))?)?;
        let mut strokes = vec![];
        // Ring order keeps the oldest retained segment first.
        for index in 0..MAX_SEGMENTS {
            let slot = (usize::from(head.cursor) + index) % MAX_SEGMENTS + 1;
            if let Ok(raw) = payload(&self.tree, &path(5, slot)) {
                let e: Entry = decode(&raw)?;
                if e.stroke.epoch == head.epoch {
                    strokes.push(e.stroke);
                }
            }
        }
        Ok(Drawing {
            epoch: head.epoch,
            strokes,
        })
    }
    pub(super) fn validate_drawing(&self, tree: &Snapshot) -> Result<()> {
        let head: Head = decode(&payload(tree, &path(5, 0))?)?;
        if usize::from(head.cursor) >= MAX_SEGMENTS {
            return Err("Drawing cursor bound".into());
        }
        let branch = tree.subtree(&[5]).map_err(err)?;
        if branch.register().is_some() {
            return Err("Invalid drawing branch".into());
        }
        for bucket in branch.child_keys() {
            if bucket > 2 {
                return Err("Drawing bucket bound".into());
            }
            let group = branch.subtree(&[bucket]).map_err(err)?;
            if group.register().is_some() {
                return Err("Invalid drawing bucket".into());
            }
            for key in group.child_keys() {
                let slot = (bucket * 32 + key) as usize;
                if key >= 32 || slot > MAX_SEGMENTS {
                    return Err("Drawing slot bound".into());
                }
                let leaf = group.subtree(&[key]).map_err(err)?;
                if leaf.child_keys().next().is_some()
                    || leaf
                        .register()
                        .is_none_or(|r| r.schema() != self.extra_schema(5, slot))
                {
                    return Err("Drawing leaf schema".into());
                }
                if slot > 0 {
                    let entry: Entry = decode(&payload(tree, &path(5, slot))?)?;
                    validate_command(&Command::Put(entry.stroke))?;
                }
            }
        }
        Ok(())
    }
    fn drawing_write(&mut self, writer: [u8; 16], command: &Command) -> Result<bool> {
        validate_command(command)?;
        let mut head: Head = decode(&payload(&self.tree, &path(5, 0))?)?;
        let mut edits = vec![];
        match command {
            Command::Clear { expected, next } if head.epoch == *expected => {
                head.epoch = *next;
                head.cursor = 0;
            }
            Command::Put(stroke) if stroke.epoch == head.epoch => {
                let mut found = None;
                for slot in 1..=MAX_SEGMENTS {
                    if let Ok(raw) = payload(&self.tree, &path(5, slot)) {
                        let old: Entry = decode(&raw)?;
                        if old.stroke.epoch == head.epoch && old.stroke.id == stroke.id {
                            if old.owner != Sha256::digest(writer).as_slice() {
                                return Err("Drawing segment belongs to another writer".into());
                            }
                            if old.stroke.points.starts_with(&stroke.points) {
                                return Ok(false);
                            }
                            if !stroke.points.starts_with(&old.stroke.points) {
                                return Err("Drawing segments only append".into());
                            }
                            found = Some(slot);
                            break;
                        }
                    }
                }
                let slot = found.unwrap_or(usize::from(head.cursor) + 1);
                if found.is_none() {
                    head.cursor = ((usize::from(head.cursor) + 1) % MAX_SEGMENTS) as u16;
                }
                edits.push(Edit {
                    path: path(5, slot),
                    schema: self.extra_schema(5, slot),
                    value: Some(bytes(&Entry {
                        owner: Sha256::digest(writer).into(),
                        stroke: stroke.clone(),
                    })?),
                });
            }
            _ => return Ok(false), // A late pre-Clear packet cannot resurrect old strokes.
        }
        edits.push(Edit {
            path: path(5, 0),
            schema: self.extra_schema(5, 0),
            value: Some(bytes(&head)?),
        });
        self.change(&[], edits)
    }
    pub(super) fn receive_drawing(
        &mut self,
        session: &Session,
        writer: [u8; 16],
        sequence: u64,
        command: Command,
    ) -> Result<bool> {
        if !session.drawing_edit || !self.drawing_enabled {
            return Err("Drawing write requires admitted edit access".into());
        }
        let slot = session.slot.ok_or("Drawing writer receipt missing")?;
        let mut receipt: Receipt = decode(&payload(&self.tree, &path(2, slot))?)?;
        if receipt.writer != hash(&writer) {
            return Err("Drawing writer receipt retired".into());
        }
        let digest = hash(&bytes(&("drawing-v1", &command))?);
        if sequence <= receipt.sequence {
            if sequence != receipt.sequence || receipt.digest != digest {
                return Err("Changed drawing replay".into());
            }
            return Ok(false);
        }
        if sequence
            != receipt
                .sequence
                .checked_add(1)
                .ok_or("Drawing sequence exhausted")?
        {
            return Err("Drawing sequence gap".into());
        }
        let mut candidate = self.clone();
        candidate.drawing_write(writer, &command)?;
        receipt.sequence = sequence;
        receipt.digest = digest;
        receipt.time = self.wall;
        candidate.change(
            &[],
            vec![Edit {
                path: path(2, slot),
                schema: self.extra_schema(2, slot),
                value: Some(bytes(&receipt)?),
            }],
        )?;
        self.tree = candidate.tree;
        Ok(true)
    }
}
impl StateAdapter {
    pub fn drawing(&self) -> Result<Drawing> {
        let mut result = self.doc.drawing()?;
        if let Some(command) = self.flight.as_ref().and_then(|f| f.drawing.as_ref()) {
            result.apply(command);
        }
        for command in &self.drawing_pending {
            result.apply(command);
        }
        Ok(result)
    }
    pub fn queue_drawing(&mut self, command: Command) -> Result<bool> {
        if self.read_only {
            return Ok(false);
        }
        validate_command(&command)?;
        if matches!(command, Command::Clear { .. }) {
            self.drawing_pending.clear();
        }
        if let Command::Put(s) = &command {
            if let Some(old) = self
                .drawing_pending
                .iter_mut()
                .find(|c| matches!(c,Command::Put(o)if o.id==s.id&&o.epoch==s.epoch))
            {
                *old = command;
                return Ok(true);
            }
        }
        if self.drawing_pending.len() >= MAX_SEGMENTS + 1 {
            return Err("Drawing pending budget".into());
        }
        self.drawing_pending.push_back(command);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ui() -> Ui {
        Ui {
            format: "otrios-canvas-v2".into(),
            ui_ref: "app:drawing/ui".into(),
            title: "Drawing".into(),
            state_schema: StateSchema {
                id: "drawing-test-v1".into(),
                fields: vec![],
            },
            layout: vec![],
            selectors: vec![],
            handlers: vec![],
            navigation: vec![],
            behavior: None,
            presentation: Some(Presentation::TouchPad {
                control: 1,
                ink: Some(InkOptions {
                    drawing: Some(otrios_interfaces::drawing::Options {
                        reference: [1280, 800],
                        grid: 80,
                    }),
                    lifetime_ms: None,
                    max_segments: 64,
                    fullscreen: true,
                }),
            }),
        }
    }
    fn pump(doc: &mut Document, s: &mut Session, c: &mut StateAdapter) {
        for _ in 0..30 {
            let mut changed = false;
            while let Some(b) = c.generate().unwrap() {
                doc.receive(s, &b).unwrap();
                changed = true;
            }
            if let Some(b) = doc.generate(s).unwrap() {
                c.receive(&b).unwrap();
                changed = true;
            }
            if !changed {
                return;
            }
        }
        panic!("Drawing sync did not settle");
    }
    #[test]
    fn shared_drawing_merges_writers_clear_fences_old_strokes_and_recovers() {
        let ui = ui();
        let mut doc = Document::new(ui.state_schema.clone(), "drawing").unwrap();
        doc.configure_drawing(&ui).unwrap();
        let mut a = StateAdapter::for_ui(&ui, "drawing", &[], [1; 16]).unwrap();
        let mut b = StateAdapter::for_ui(&ui, "drawing", &[], [2; 16]).unwrap();
        let (mut sa, mut sb) = (Session::default(), Session::default());
        pump(&mut doc, &mut sa, &mut a);
        pump(&mut doc, &mut sb, &mut b);
        let stroke = |id| {
            Command::Put(Stroke {
                epoch: [0; 16],
                id: [id; 16],
                points: vec![[0, 0], [100, 200]],
            })
        };
        a.queue_drawing(stroke(1)).unwrap();
        b.queue_drawing(stroke(2)).unwrap();
        pump(&mut doc, &mut sa, &mut a);
        pump(&mut doc, &mut sb, &mut b);
        pump(&mut doc, &mut sa, &mut a);
        assert_eq!(a.drawing().unwrap().strokes.len(), 2);
        assert_eq!(b.drawing().unwrap().strokes.len(), 2);
        let saved = doc.save().unwrap();
        let mut recovered = Document::load(ui.state_schema.clone(), "drawing", &saved).unwrap();
        recovered.configure_drawing(&ui).unwrap();
        assert_eq!(recovered.drawing().unwrap().strokes.len(), 2);
        b.queue_drawing(stroke(3)).unwrap();
        a.queue_drawing(Command::Clear {
            expected: [0; 16],
            next: [9; 16],
        })
        .unwrap();
        pump(&mut doc, &mut sa, &mut a);
        pump(&mut doc, &mut sb, &mut b);
        assert!(doc.drawing().unwrap().strokes.is_empty());
        assert!(b.drawing().unwrap().strokes.is_empty());
        assert_eq!(b.drawing().unwrap().epoch, [9; 16]);
        let mut read = doc.open_read_session(&[3; 16]).unwrap();
        let denied = encode_state_request(&StateRequest::Drawing {
            sequence: 1,
            command: stroke(4),
        })
        .unwrap();
        assert!(doc.receive(&mut read, &denied).is_err());
    }
}
