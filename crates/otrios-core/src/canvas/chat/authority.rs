// SPDX-License-Identifier: LGPL-3.0-only
use super::journal::{Entry, Guest, Journal, Receipt};
use super::{Command, Event, Kind, Member, Message, Profile, Result, err, random};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use unicode_segmentation::UnicodeSegmentation;

const DAY: u64 = 86_400_000;
const EPOCH: u64 = 600_000;
const MAX_RECEIPTS: usize = 65_536;
#[derive(Clone, Debug)]
pub struct Delivery {
    pub target: Option<u64>,
    pub event: Event,
}
impl Delivery {
    fn all(event: Event) -> Self {
        Self {
            target: None,
            event,
        }
    }
    fn one(target: u64, event: Event) -> Self {
        Self {
            target: Some(target),
            event,
        }
    }
}
type Parsed = (Kind, String, Option<(String, Option<String>)>);
struct Connection {
    id: String,
    admin: bool,
    admission: Arc<AtomicBool>,
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.admission.store(false, Ordering::Release);
    }
}
#[derive(Default)]
struct Draft {
    owner: u64,
    generation: u64,
    revision: u64,
    text: String,
    at: u64,
    window: u64,
    count: u8,
}
struct Rate {
    at: u64,
    tokens: f64,
    effect_at: u64,
    effects: u8,
}
pub struct Room {
    guests: BTreeMap<String, Guest>,
    connections: BTreeMap<u64, Connection>,
    memberships: BTreeMap<String, u64>,
    drafts: BTreeMap<String, Draft>,
    rates: BTreeMap<String, Rate>,
    history: super::history::History,
    receipts: BTreeMap<(String, u64, String), Receipt>,
    sequence: u64,
    failed: bool,
    journal: Option<Journal>,
}
fn digest(value: &str) -> String {
    super::super::hex(&super::super::hash(value.as_bytes()))
}
fn generation() -> Result<u64> {
    let mut bytes = [0; 8];
    getrandom::fill(&mut bytes).map_err(err)?;
    Ok(u64::from_le_bytes(bytes).max(1))
}
fn text(value: &str, bytes: usize, clusters: usize, lines: usize) -> Result<()> {
    if value.len() > bytes
        || value.graphemes(true).count() > clusters
        || value.graphemes(true).any(|g| g.len() > 256)
        || value.lines().count() > lines
        || value
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\t'))
    {
        return Err("Text exceeds room bounds".into());
    }
    Ok(())
}
fn name(value: &str) -> Result<String> {
    let value = value.trim();
    text(value, 96, 24, 1)?;
    if value.is_empty() || value.starts_with(['@', '+', '/']) || value.chars().any(|c| c.is_control() || matches!(c, '\u{200e}' | '\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')) {
        return Err("Choose a name without an operator prefix".into());
    }
    Ok(value.to_owned())
}
impl Room {
    #[cfg(test)]
    pub(super) fn fail_journal_writes(&mut self) {
        self.journal.as_mut().unwrap().fail_writes();
    }
    #[cfg(test)]
    pub(super) fn force_compaction(&mut self) {
        self.journal.as_mut().unwrap().bytes = 49 * 1024 * 1024;
    }
    pub fn open(path: Option<PathBuf>, now: u64) -> Result<Self> {
        Self::open_in(
            path,
            now,
            Arc::new(Mutex::new(crate::collection::Channel::new(generation()?))),
        )
    }
    pub fn open_in(
        path: Option<PathBuf>,
        now: u64,
        collections: Arc<Mutex<crate::collection::Channel>>,
    ) -> Result<Self> {
        let mut room = Self {
            guests: BTreeMap::new(),
            connections: BTreeMap::new(),
            memberships: BTreeMap::new(),
            drafts: BTreeMap::new(),
            rates: BTreeMap::new(),
            history: super::history::History::recovering()?,
            receipts: BTreeMap::new(),
            sequence: 0,
            failed: false,
            journal: None,
        };
        if let Some(path) = path {
            room.journal = Some(Journal::recover(path, |entry| room.restore(entry, now))?);
        }
        room.prune(now);
        // Recovery never exposes a partly rebuilt log, including on a corrupt
        // later record. The shared registry receives exactly one qualified root.
        room.history.activate(collections)?;
        Ok(room)
    }
    fn restore(&mut self, entry: Entry, now: u64) -> Result<()> {
        match entry {
            Entry::Guest(guest) => {
                if !super::valid_hex(&guest.profile.id, 16)
                    || !super::valid_hex(&guest.digest, 32)
                    || guest.profile.avatar >= super::AVATARS
                {
                    return Err("Invalid saved guest".into());
                }
                name(&guest.profile.name)?;
                self.guests.insert(guest.profile.id.clone(), guest);
            }
            Entry::Sent { message, receipt } => {
                self.add_message(message)?;
                if (receipt.epoch.saturating_add(2)).saturating_mul(EPOCH) > now {
                    self.receipts.insert(
                        (
                            receipt.participant.clone(),
                            receipt.epoch,
                            receipt.nonce.clone(),
                        ),
                        receipt,
                    );
                }
            }
            Entry::Kick { guest, message } => {
                self.guests.insert(guest.profile.id.clone(), guest);
                self.add_message(message)?;
            }
            Entry::Message(message) => self.add_message(message)?,
            Entry::Receipt(receipt) => {
                if (receipt.epoch.saturating_add(2)).saturating_mul(EPOCH) > now {
                    self.receipts.insert(
                        (
                            receipt.participant.clone(),
                            receipt.epoch,
                            receipt.nonce.clone(),
                        ),
                        receipt,
                    );
                }
            }
            Entry::Sequence(sequence) => self.sequence = self.sequence.max(sequence),
        }
        if self.guests.len() > 4096 || self.receipts.len() > MAX_RECEIPTS {
            return Err("Saved room exceeds identity/receipt bounds".into());
        }
        Ok(())
    }
    fn prepare_message(&self, message: &Message) -> Result<super::history::Prepared> {
        text(&message.text, 2304, 1200, 18)?;
        name(&message.sender.name)?;
        if !super::valid_hex(&message.sender.id, 16)
            || message.sender.avatar >= super::AVATARS
            || (!message.nonce.is_empty() && !super::valid_hex(&message.nonce, 16))
        {
            return Err("Invalid immutable message profile or nonce".into());
        }
        if message.sequence <= self.sequence {
            return Err("Chat history sequence is not increasing".into());
        }
        self.history.prepare(message)
    }
    fn add_message(&mut self, message: Message) -> Result<()> {
        self.history.commit(self.prepare_message(&message)?)?;
        self.accept_message(&message);
        Ok(())
    }
    fn persist_message(&mut self, message: &Message, entry: &Entry, now: u64) -> Result<()> {
        let prepared = self.prepare_message(message)?;
        self.persist(entry, now)?;
        // Room is the sole writer for this resource. A publication failure after
        // fsync must stop further writes, since retry deduplication is durable
        // in the journal even if its new index root could not become active.
        if let Err(error) = self.history.commit(prepared) {
            self.failed = true;
            for connection in self.connections.values() {
                connection.admission.store(false, Ordering::Release);
            }
            return Err(error);
        }
        self.accept_message(message);
        Ok(())
    }
    fn accept_message(&mut self, message: &Message) {
        self.sequence = message.sequence;
        if let Some(guest) = self.guests.get_mut(&message.sender.id) {
            guest.last_seen_unix_ms = guest.last_seen_unix_ms.max(message.at);
        }
    }
    fn prune(&mut self, now: u64) {
        self.guests.retain(|id, g| {
            g.cooldown > now
                || now.saturating_sub(g.last_seen_unix_ms) < 30 * DAY
                || self.memberships.contains_key(id)
        });
        self.receipts
            .retain(|_, r| (r.epoch.saturating_add(2)).saturating_mul(EPOCH) > now);
        self.rates.retain(|id, _| self.guests.contains_key(id));
    }
    fn persist(&mut self, entry: &Entry, now: u64) -> Result<()> {
        if self.failed {
            return Err("Room requires journal recovery".into());
        }
        self.prune(now);
        if self
            .journal
            .as_ref()
            .is_some_and(|j| j.bytes > 48 * 1024 * 1024)
        {
            let snapshot = self.history.snapshot()?;
            let entries = self
                .guests
                .values()
                .cloned()
                .map(|guest| Ok(Entry::Guest(guest)))
                .chain(
                    snapshot
                        .records()
                        .map(|(_, row)| super::read_message(row).map(Entry::Message)),
                )
                .chain(
                    self.receipts
                        .values()
                        .cloned()
                        .map(|receipt| Ok(Entry::Receipt(receipt))),
                )
                .chain([Ok(Entry::Sequence(self.sequence))]);
            self.journal.as_mut().unwrap().compact(entries)?;
        }
        if let Some(journal) = &mut self.journal {
            journal.append(entry)?;
        }
        Ok(())
    }
    pub fn identity(&mut self, credential: Option<&str>, now: u64) -> Result<(Profile, String)> {
        self.prune(now);
        if let Some(token) = credential {
            if !super::valid_hex(token, 32) {
                return Err("Invalid guest credential".into());
            }
            let hash = digest(token);
            let old = self
                .guests
                .values()
                .find(|g| g.digest == hash)
                .ok_or("Guest credential expired")?;
            let mut guest = old.clone();
            guest.last_seen_unix_ms = guest.last_seen_unix_ms.max(now);
            self.persist(&Entry::Guest(guest.clone()), now)?;
            self.guests.insert(guest.profile.id.clone(), guest.clone());
            return Ok((guest.profile, token.to_owned()));
        }
        if self.guests.len() >= 4096 {
            return Err("Guest capacity reached; try later".into());
        }
        let token = random(32)?;
        let id = random(16)?;
        const TITLES: [&str; 16] = [
            "Professor",
            "Captain",
            "Lady",
            "Baron",
            "Doctor",
            "Admiral",
            "Count",
            "Dame",
            "Agent",
            "Major",
            "Pilot",
            "Sir",
            "Madam",
            "Inspector",
            "Duke",
            "Chief",
        ];
        const THINGS: [&str; 16] = [
            "Piston",
            "Sprocket",
            "Gearbox",
            "Biscuit",
            "Teapot",
            "Whistle",
            "Goggles",
            "Rivet",
            "Waffles",
            "Copper",
            "Crumpet",
            "Boiler",
            "Pocketwatch",
            "Pigeon",
            "Steam",
            "Gadget",
        ];
        let mut seed = [0; 3];
        getrandom::fill(&mut seed).map_err(err)?;
        let mut display = format!(
            "{} {}",
            TITLES[seed[0] as usize % 16],
            THINGS[seed[1] as usize % 16]
        );
        if self
            .guests
            .values()
            .any(|g| g.profile.name.to_lowercase() == display.to_lowercase())
        {
            display = format!(
                "{} {}",
                display.graphemes(true).take(18).collect::<String>(),
                &id[..4]
            );
        }
        let profile = Profile {
            id: id.clone(),
            name: display,
            avatar: seed[2] % super::AVATARS,
        };
        let guest = Guest {
            profile: profile.clone(),
            digest: digest(&token),
            last_seen_unix_ms: now,
            cooldown: 0,
        };
        self.persist(&Entry::Guest(guest.clone()), now)?;
        self.guests.insert(id, guest);
        Ok((profile, token))
    }
    pub fn valid_identity(&self, credential: &str) -> bool {
        if !super::valid_hex(credential, 32) {
            return false;
        }
        let hash = digest(credential);
        self.guests.values().any(|g| g.digest == hash)
    }
    pub fn members(&self) -> Vec<Member> {
        let mut members = self
            .memberships
            .iter()
            .filter_map(|(id, generation)| {
                let g = self.guests.get(id)?;
                Some(Member {
                    profile: g.profile.clone(),
                    generation: *generation,
                    admin: self.connections.values().any(|c| c.id == *id && c.admin),
                })
            })
            .collect::<Vec<_>>();
        members.sort_by(|a, b| {
            b.admin
                .cmp(&a.admin)
                .then_with(|| {
                    a.profile
                        .name
                        .to_lowercase()
                        .cmp(&b.profile.name.to_lowercase())
                })
                .then(a.profile.id.cmp(&b.profile.id))
        });
        members
    }
    fn roster(&self) -> Delivery {
        Delivery::all(Event::Members {
            members: self.members(),
        })
    }
    pub fn active(&self, connection: u64) -> bool {
        self.connections.contains_key(&connection)
    }
    /// Bind the server's local admission lifetime. Client bytes cannot construct
    /// this link. Dropping a membership clears it before any kick/leave event.
    pub fn bind_admission(&mut self, connection: u64, admission: Arc<AtomicBool>) -> Result<()> {
        let connection = self
            .connections
            .get_mut(&connection)
            .ok_or("Join the room first")?;
        connection.admission.store(false, Ordering::Release);
        connection.admission = admission;
        connection.admission.store(true, Ordering::Release);
        Ok(())
    }
    pub fn join(
        &mut self,
        connection: u64,
        credential: Option<&str>,
        admin: bool,
        now: u64,
        native: bool,
    ) -> Result<Vec<Delivery>> {
        if self.connections.len() >= 68 && !self.connections.contains_key(&connection) {
            return Err("Room full".into());
        }
        let (profile, token) = self.identity(credential, now)?;
        let guest = &self.guests[&profile.id];
        if guest.cooldown > now {
            return Ok(vec![Delivery::one(
                connection,
                Event::Kicked {
                    reason: "Please let the boiler cool".into(),
                    rejoin_at: guest.cooldown,
                },
            )]);
        }
        let mut events = self.leave(connection, now)?;
        self.memberships
            .entry(profile.id.clone())
            .or_insert(generation()?);
        self.connections.insert(
            connection,
            Connection {
                id: profile.id.clone(),
                admin,
                admission: Arc::new(AtomicBool::new(true)),
            },
        );
        let mut you = self
            .members()
            .into_iter()
            .find(|m| m.profile.id == profile.id)
            .unwrap();
        you.admin = admin;
        events.push(Delivery::one(
            connection,
            Event::Welcome {
                you,
                temporary: !native && credential.is_none(),
                credential: native.then_some(token),
                epoch: now / EPOCH,
                members: self.members(),
            },
        ));
        events.push(self.roster());
        events.extend(self.claim(connection, now)?);
        Ok(events)
    }
    fn record_seen(&mut self, id: &str, now: u64) -> Result<()> {
        let Some(current) = self.guests.get(id) else {
            return Ok(());
        };
        if now <= current.last_seen_unix_ms {
            return Ok(());
        }
        let mut guest = current.clone();
        guest.last_seen_unix_ms = now;
        self.persist(&Entry::Guest(guest.clone()), now)?;
        self.guests.insert(id.to_owned(), guest);
        Ok(())
    }
    pub fn leave(&mut self, connection: u64, now: u64) -> Result<Vec<Delivery>> {
        if let Some(c) = self.connections.get(&connection) {
            self.record_seen(&c.id.clone(), now)?;
        }
        let Some(c) = self.connections.remove(&connection) else {
            return Ok(vec![]);
        };
        let mut events = vec![];
        if let Some(d) = self.drafts.get_mut(&c.id)
            && d.owner == connection
        {
            d.text.clear();
            d.revision = d.revision.saturating_add(1);
            events.push(Delivery::all(Event::Draft {
                participant: c.id.clone(),
                generation: d.generation,
                revision: d.revision,
                text: String::new(),
                expires: now,
            }));
        }
        if !self.connections.values().any(|other| other.id == c.id) {
            self.memberships.remove(&c.id);
            self.drafts.remove(&c.id);
        }
        events.push(self.roster());
        Ok(events)
    }
    pub fn set_admin(&mut self, connection: u64, admin: bool) -> Vec<Delivery> {
        let Some(c) = self.connections.get_mut(&connection) else {
            return vec![];
        };
        if c.admin == admin {
            return vec![];
        }
        c.admin = admin;
        vec![
            Delivery::one(connection, Event::Permissions { admin }),
            self.roster(),
        ]
    }
    fn claim(&mut self, connection: u64, now: u64) -> Result<Vec<Delivery>> {
        let c = self
            .connections
            .get(&connection)
            .ok_or("Join the room first")?;
        let lease = generation()?;
        self.drafts.insert(
            c.id.clone(),
            Draft {
                owner: connection,
                generation: lease,
                ..Draft::default()
            },
        );
        Ok(vec![
            Delivery::one(connection, Event::DraftLease { generation: lease }),
            Delivery::all(Event::Draft {
                participant: c.id.clone(),
                generation: lease,
                revision: 0,
                text: String::new(),
                expires: now,
            }),
        ])
    }
    pub fn tick(&mut self, now: u64) -> Result<Vec<Delivery>> {
        // A quiet connected reader is still present. Checkpoint at most once
        // per minute per profile, rather than syncing on every 500ms host tick.
        let due = self
            .memberships
            .keys()
            .filter(|id| {
                self.guests
                    .get(*id)
                    .is_some_and(|g| now.saturating_sub(g.last_seen_unix_ms) >= 60_000)
            })
            .cloned()
            .collect::<Vec<_>>();
        for id in due {
            self.record_seen(&id, now)?;
        }
        let mut events = vec![];
        for (id, d) in &mut self.drafts {
            if !d.text.is_empty() && now.saturating_sub(d.at) >= 3000 {
                d.text.clear();
                d.revision = d.revision.saturating_add(1);
                events.push(Delivery::all(Event::Draft {
                    participant: id.clone(),
                    generation: d.generation,
                    revision: d.revision,
                    text: String::new(),
                    expires: now,
                }));
            }
        }
        Ok(events)
    }
    pub fn handle(&mut self, connection: u64, command: Command, now: u64) -> Result<Vec<Delivery>> {
        if self.failed {
            return Err("Room requires journal recovery".into());
        }
        let c = self
            .connections
            .get(&connection)
            .ok_or("Join the room first")?;
        let id = c.id.clone();
        let admin = c.admin;
        match command {
            Command::Leave => {
                let mut events = vec![Delivery::one(connection, Event::Left)];
                events.extend(self.leave(connection, now)?);
                Ok(events)
            }
            Command::ClaimDraft => self.claim(connection, now),
            Command::Profile {
                name: requested,
                avatar,
            } => {
                let mut display = name(&requested)?;
                if avatar >= super::AVATARS {
                    return Err("Unknown avatar".into());
                }
                if self.guests.iter().any(|(other, g)| {
                    *other != id && g.profile.name.to_lowercase() == display.to_lowercase()
                }) {
                    let shortened = display.graphemes(true).take(18).collect::<String>();
                    display = format!("{} {}", shortened, &id[..4]);
                }
                let mut guest = self.guests[&id].clone();
                guest.profile.name = display;
                guest.profile.avatar = avatar;
                guest.last_seen_unix_ms = guest.last_seen_unix_ms.max(now);
                self.persist(&Entry::Guest(guest.clone()), now)?;
                self.guests.insert(id, guest);
                Ok(vec![self.roster()])
            }
            Command::Draft {
                generation,
                revision,
                text: body,
            } => {
                text(&body, 1024, 256, 16)?;
                let d = self.drafts.get_mut(&id).ok_or("Claim a draft first")?;
                if d.owner != connection || generation != d.generation || revision <= d.revision {
                    return Ok(vec![]);
                }
                if now.saturating_sub(d.window) >= 1000 {
                    d.window = now;
                    d.count = 0;
                }
                d.count = d.count.saturating_add(1);
                if d.count > 12 {
                    return Ok(vec![]);
                }
                d.revision = revision;
                d.at = now;
                d.text = if body.starts_with('/') && !body.starts_with("//") {
                    String::new()
                } else {
                    body
                };
                Ok(vec![Delivery::all(Event::Draft {
                    participant: id,
                    generation: d.generation,
                    revision,
                    text: d.text.clone(),
                    expires: now + 3000,
                })])
            }
            Command::ClearDraft {
                generation,
                revision,
            } => self.handle(
                connection,
                Command::Draft {
                    generation,
                    revision,
                    text: String::new(),
                },
                now,
            ),
            Command::Kick {
                participant,
                generation,
                reason,
            } => {
                if !admin {
                    return Err("Monitoring sign-in is required to kick a user".into());
                }
                text(&reason, 256, 120, 1)?;
                if participant == id {
                    return Err("You cannot kick yourself".into());
                }
                if self.memberships.get(&participant) != Some(&generation) {
                    return Err("That membership has changed; open Members again".into());
                }
                let mut guest = self
                    .guests
                    .get(&participant)
                    .ok_or("Unknown member")?
                    .clone();
                guest.cooldown = now + 60_000;
                guest.last_seen_unix_ms = guest.last_seen_unix_ms.max(now);
                let message = Message {
                    sequence: self
                        .sequence
                        .checked_add(1)
                        .ok_or("Room message sequence exhausted")?,
                    at: now,
                    sender: self.guests[&id].profile.clone(),
                    kind: Kind::Notice,
                    text: format!(
                        "{} was kicked by {}{}",
                        guest.profile.name,
                        self.guests[&id].profile.name,
                        if reason.is_empty() {
                            String::new()
                        } else {
                            format!(": {reason}")
                        }
                    ),
                    nonce: String::new(),
                };
                self.persist_message(
                    &message,
                    &Entry::Kick {
                        guest: guest.clone(),
                        message: message.clone(),
                    },
                    now,
                )?;
                self.guests.insert(participant.clone(), guest);
                let targets = self
                    .connections
                    .iter()
                    .filter_map(|(key, c)| (c.id == participant).then_some(*key))
                    .collect::<Vec<_>>();
                let mut events = vec![];
                for target in targets {
                    events.push(Delivery::one(
                        target,
                        Event::Kicked {
                            reason: reason.clone(),
                            rejoin_at: now + 60_000,
                        },
                    ));
                    self.connections.remove(&target);
                }
                self.memberships.remove(&participant);
                self.drafts.remove(&participant);
                events.push(self.roster());
                Ok(events)
            }
            Command::Send {
                epoch,
                nonce,
                text: body,
            } => {
                text(&body, 2048, 1024, 16)?;
                if body.trim().is_empty() {
                    return Err("Write a message first".into());
                }
                if !super::valid_hex(&nonce, 16)
                    || epoch > now / EPOCH
                    || epoch.saturating_add(2).saturating_mul(EPOCH) <= now
                {
                    return Err("Send expired; send again with a fresh receipt epoch".into());
                }
                let hash = super::super::hash(body.as_bytes());
                let receipt_key = (id.clone(), epoch, nonce.clone());
                if let Some(r) = self.receipts.get(&receipt_key) {
                    if r.digest != hash {
                        return Err("Message nonce conflicts with its original text".into());
                    }
                    return Ok(vec![Delivery::one(
                        connection,
                        Event::Receipt {
                            nonce,
                            sequence: r.sequence,
                        },
                    )]);
                }
                self.prune(now);
                if self.receipts.len() >= MAX_RECEIPTS {
                    return Err("Receipt capacity reached; try later".into());
                }
                let rate = self.rates.entry(id.clone()).or_insert(Rate {
                    at: now,
                    tokens: 5.,
                    effect_at: now,
                    effects: 0,
                });
                rate.tokens = (rate.tokens + now.saturating_sub(rate.at) as f64 / 1000.).min(5.);
                rate.at = now;
                if rate.tokens < 1. {
                    return Err("A little slower, please; try again shortly".into());
                }
                rate.tokens -= 1.;
                let (kind, content, effect) = self.parse(&id, &body, admin)?;
                if effect.is_some() {
                    let rate = self.rates.get_mut(&id).unwrap();
                    if now.saturating_sub(rate.effect_at) >= 1000 {
                        rate.effect_at = now;
                        rate.effects = 0;
                    }
                    if rate.effects >= 2 {
                        return Err("Let the steam settle before another effect".into());
                    }
                    rate.effects += 1;
                }
                let message = Message {
                    sequence: self
                        .sequence
                        .checked_add(1)
                        .ok_or("Room message sequence exhausted")?,
                    at: now,
                    sender: self.guests[&id].profile.clone(),
                    kind,
                    text: content,
                    nonce: nonce.clone(),
                };
                let receipt = Receipt {
                    participant: id.clone(),
                    epoch,
                    nonce: nonce.clone(),
                    digest: hash,
                    sequence: message.sequence,
                };
                self.persist_message(
                    &message,
                    &Entry::Sent {
                        message: message.clone(),
                        receipt: receipt.clone(),
                    },
                    now,
                )?;
                self.receipts.insert(receipt_key, receipt);
                let mut events = vec![Delivery::one(
                    connection,
                    Event::Receipt {
                        nonce,
                        sequence: message.sequence,
                    },
                )];
                if let Some(d) = self.drafts.get_mut(&id) {
                    d.text.clear();
                    d.revision = d.revision.saturating_add(1);
                    events.push(Delivery::all(Event::Draft {
                        participant: id.clone(),
                        generation: d.generation,
                        revision: d.revision,
                        text: String::new(),
                        expires: now,
                    }));
                }
                if let Some((kind, target)) = effect {
                    events.push(Delivery::all(Event::Effect {
                        participant: id,
                        kind,
                        target,
                    }));
                }
                Ok(events)
            }
            _ => Err("Unexpected room command".into()),
        }
    }
    fn parse(&self, id: &str, body: &str, admin: bool) -> Result<Parsed> {
        let who = &self.guests[id].profile.name;
        if body.starts_with("//") {
            return Ok((Kind::Message, body[1..].into(), None));
        }
        if !body.starts_with('/') {
            return Ok((Kind::Message, body.into(), None));
        }
        let (cmd, arg) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
        let arg = arg.trim();
        match cmd {
            "/me" if !arg.is_empty() => Ok((Kind::Action, arg.into(), None)),
            "/shrug" => Ok((Kind::Message, format!("{arg} ¯\\_(ツ)_/¯"), None)),
            "/steam" => Ok((
                Kind::Notice,
                format!("{who} releases a dignified puff of steam."),
                Some(("steam".into(), None)),
            )),
            "/salute" | "/boop" => {
                let target = self
                    .members()
                    .into_iter()
                    .find(|m| m.profile.name.to_lowercase() == arg.to_lowercase())
                    .ok_or("Use a full name from Members")?;
                Ok((
                    Kind::Notice,
                    format!("{who} tips a brass-trimmed hat to {}.", target.profile.name),
                    Some(("salute".into(), Some(target.profile.id))),
                ))
            }
            "/roll" => {
                let notation = if arg.is_empty() { "1d20" } else { arg };
                let (n, d) = notation
                    .split_once('d')
                    .ok_or("Try /roll 1d20 or /roll 2d6")?;
                let count = n.parse::<u8>().map_err(err)?;
                let sides = d.parse::<u8>().map_err(err)?;
                if !(1..=10).contains(&count) || !(2..=100).contains(&sides) {
                    return Err("Use 1–10 dice and 2–100 sides".into());
                }
                let mut rolls = vec![];
                for _ in 0..count {
                    let mut byte = [0];
                    loop {
                        getrandom::fill(&mut byte).map_err(err)?;
                        if u16::from(byte[0]) < 256 - 256 % u16::from(sides) {
                            break;
                        }
                    }
                    rolls.push(u16::from(byte[0] % sides + 1));
                }
                let total = rolls.iter().sum::<u16>();
                Ok((
                    Kind::Notice,
                    format!(
                        "{who} rolls {notation}: {} = {total}",
                        rolls
                            .iter()
                            .map(u16::to_string)
                            .collect::<Vec<_>>()
                            .join(" + ")
                    ),
                    None,
                ))
            }
            "/topic" if admin && !arg.is_empty() => {
                Ok((Kind::Notice, format!("{who} sets the topic: {arg}"), None))
            }
            "/topic" => Err("A Monitoring admin can set /topic text".into()),
            _ => Err("Unknown or incomplete command. Try /help.".into()),
        }
    }
}
