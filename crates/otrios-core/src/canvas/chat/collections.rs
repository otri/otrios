// SPDX-License-Identifier: LGPL-3.0-only
//! Chat consumes the reusable coherent collection lane; no history pages or
//! individual message bodies are carried by ephemeral room commands.
use super::*;
use crate::collection as col;

impl Client {
    pub(super) fn select_window(&mut self, anchor: col::Anchor) -> Result<()> {
        self.collection
            .select(message_selection(anchor))
            .map_err(err)?;
        self.collection_anchor = anchor;
        self.history_wait = true;
        Ok(())
    }
    pub fn collection_outgoing(&mut self, counter: &mut u64) -> Result<Option<Vec<u8>>> {
        self.collection
            .outgoing(counter)
            .map_err(err)?
            .map(|command| col::encode_command(&command).map_err(err))
            .transpose()
    }
    pub fn cancel_collection(&mut self) -> Option<col::Command> {
        let generation = self.collection.generation();
        self.collection.stop();
        (generation != 0).then_some(col::Command::Cancel { generation })
    }
    pub(super) fn clear_collection(&mut self) {
        self.collection.stop();
        self.collection_revoked = true;
        self.messages.clear();
        self.append.clear();
        self.prepend.clear();
        self.reset = true;
        self.older = false;
        self.more_after = false;
        self.history_wait = false;
        self.members.clear();
        self.drafts.clear();
        self.particles.clear();
        self.authors_revision = self.authors_revision.saturating_add(1);
    }
    pub fn receive_collection(&mut self, bytes: &[u8], follow: bool) -> Result<bool> {
        let event = col::Event::from_bytes(bytes).map_err(err)?;
        match self.collection.receive(event).map_err(err)? {
            col::Change::View => {
                let window = self
                    .collection
                    .window()
                    .ok_or("Missing selected messages")?
                    .clone();
                let messages = window
                    .keys
                    .iter()
                    .map(|key| {
                        let message = read_message(&window.record(*key).map_err(err)?)?;
                        if message.sequence != *key {
                            return Err("Message identity does not match membership".into());
                        }
                        Ok(message)
                    })
                    .collect::<Result<std::collections::VecDeque<_>>>()?;
                let changed = self.messages != messages;
                let metadata = self.older != window.more_before
                    || self.more_after != window.more_after
                    || self.history_wait;
                self.older = window.more_before;
                self.more_after = window.more_after;
                self.history_wait = false;
                if changed {
                    self.messages = messages;
                    self.reset = true;
                    self.append.clear();
                    self.prepend.clear();
                    self.authors_revision = self.authors_revision.saturating_add(1);
                    self.tail |= follow
                        && !self.more_after
                        && matches!(self.collection_anchor, col::Anchor::Tail);
                }
                Ok(changed || metadata)
            }
            col::Change::Revoked => {
                self.clear_collection();
                self.online = false;
                self.joining = true;
                Ok(true)
            }
            col::Change::Expired(neighbor) => {
                self.tail = false;
                self.say("The reading anchor expired; moved to the nearest retained dispatch");
                self.select_window(neighbor.map_or(col::Anchor::Tail, col::Anchor::Around))?;
                Ok(true)
            }
            col::Change::Rejected(col::Status::Stale) => {
                // A reconnect may change authority epoch. Stable record identity
                // can be sought again without reusing the expired epoch cursor.
                let anchor = self.messages.front().map_or(col::Anchor::Tail, |message| {
                    col::Anchor::Around(message.sequence)
                });
                self.select_window(anchor)?;
                Ok(false)
            }
            col::Change::Rejected(col::Status::Backpressure) => Ok(false),
            col::Change::Rejected(status) => {
                self.history_wait = false;
                self.say(format!("Collection selection: {status:?}"));
                Ok(true)
            }
            col::Change::Unchanged => {
                self.history_wait = self.collection.waiting();
                Ok(false)
            }
        }
    }
    pub(crate) fn update_window(
        &mut self,
        viewport: &super::super::document::Viewport,
    ) -> Result<()> {
        if !self.online || self.collection.waiting() || self.messages.is_empty() {
            return Ok(());
        }
        let key = viewport.anchor().and_then(|anchor| {
            self.source_messages
                .iter()
                .rev()
                .find(|(_, (block, _))| *block <= anchor.block)
                .map(|(key, _)| *key)
        });
        let Some(key) = key else {
            return Ok(());
        };
        if viewport.at_tail() && !self.more_after {
            if !matches!(self.collection_anchor, col::Anchor::Tail) {
                self.select_window(col::Anchor::Tail)?;
                self.tail = true;
            }
            return Ok(());
        }
        let position = self
            .messages
            .iter()
            .position(|message| message.sequence == key)
            .unwrap_or(0);
        if matches!(self.collection_anchor, col::Anchor::Tail)
            || position < 16 && self.older
            || position + 16 >= self.messages.len() && self.more_after
        {
            self.select_window(col::Anchor::Around(key))?;
        }
        Ok(())
    }
}
