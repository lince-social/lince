use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

const TTL: Duration = Duration::from_secs(20);
const RECORD_LIMIT: usize = 64;
const CURSOR_LIMIT: usize = 256;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cursor {
    pub session: String,
    pub person: Option<String>,
    pub organ: Option<String>,
    pub property: String,
    pub anchor: String,
    pub focus: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub record: String,
    pub origin: String,
    pub cursor: Cursor,
    pub age_ms: u64,
}

impl Entry {
    pub fn valid(&self) -> bool {
        nucleus::valid_uid(&self.record, "r")
            && nucleus::valid_uid(&self.origin, "r")
            && self.cursor.session.len() <= 128
            && !self.cursor.session.is_empty()
            && self
                .cursor
                .person
                .as_ref()
                .is_none_or(|person| nucleus::valid_uid(person, "r"))
            && self.cursor.organ.is_none()
            && matches!(self.cursor.property.as_str(), "head" | "body")
            && self.cursor.anchor.len() <= 2048
            && self.cursor.focus.len() <= 2048
            && self.age_ms < TTL.as_millis() as u64
    }
}

#[derive(Default)]
struct State {
    local: HashMap<(String, String), (Instant, Cursor)>,
    remote: HashMap<String, (Instant, Vec<Entry>)>,
    viewers: HashMap<String, BTreeSet<String>>,
}

struct Selected<'a> {
    record: &'a str,
    origin: &'a str,
    cursor: &'a Cursor,
    age_ms: u64,
}

impl Selected<'_> {
    fn entry(self) -> Entry {
        Entry {
            record: self.record.into(),
            origin: self.origin.into(),
            cursor: self.cursor.clone(),
            age_ms: self.age_ms,
        }
    }

    fn cursor(self) -> Cursor {
        let mut cursor = self.cursor.clone();
        if !self.origin.is_empty() {
            cursor.session = format!("{}/{}", self.origin, cursor.session);
            cursor.organ = Some(self.origin.into());
        }
        cursor
    }
}

impl State {
    // Filter and deduplicate borrowed entries before copying their text. Callers
    // share this ordering and expiry policy for both wire and local delivery.
    fn select<'a>(
        &'a mut self,
        local: &'a str,
        includes: impl Fn(&str) -> bool,
    ) -> Vec<Selected<'a>> {
        let _selection = crate::performance::Span::new("presence-selection");
        self.local.retain(|_, (at, _)| at.elapsed() < TTL);
        self.remote.retain(|_, (at, _)| at.elapsed() < TTL);
        let mut visited = self.local.len();
        let mut entries = Vec::new();
        for ((record, _), (at, cursor)) in &self.local {
            if includes(record) {
                entries.push(Selected {
                    record,
                    origin: local,
                    cursor,
                    age_ms: at.elapsed().as_millis() as u64,
                });
            }
        }
        for (at, remote) in self.remote.values() {
            visited += remote.len();
            for entry in remote {
                if !includes(&entry.record) {
                    continue;
                }
                let age_ms = entry.age_ms.saturating_add(at.elapsed().as_millis() as u64);
                if age_ms < TTL.as_millis() as u64 {
                    entries.push(Selected {
                        record: &entry.record,
                        origin: &entry.origin,
                        cursor: &entry.cursor,
                        age_ms,
                    });
                }
            }
        }
        entries.sort_by(|a, b| {
            (a.record, a.origin, &a.cursor.session, a.age_ms).cmp(&(
                b.record,
                b.origin,
                &b.cursor.session,
                b.age_ms,
            ))
        });
        entries.dedup_by(|a, b| {
            a.record == b.record && a.origin == b.origin && a.cursor.session == b.cursor.session
        });
        crate::performance::add("presence-entries-visited", visited as u64);
        crate::performance::add("presence-entries-selected", entries.len() as u64);
        entries
    }
}

#[derive(Default)]
pub struct Presence {
    state: Mutex<State>,
    changed: tokio::sync::watch::Sender<u64>,
}

impl Presence {
    pub fn changes(&self) -> tokio::sync::watch::Receiver<u64> {
        self.changed.subscribe()
    }

    fn notify(&self) {
        crate::performance::add("presence-wakeups", 1);
        self.changed
            .send_modify(|version| *version = version.wrapping_add(1));
    }

    pub fn join(&self, record: &str, session: &str) {
        let changed = self
            .state
            .lock()
            .expect("presence")
            .viewers
            .entry(session.into())
            .or_default()
            .insert(record.into());
        if changed {
            self.notify();
        }
    }

    pub fn has_viewers(&self) -> bool {
        self.state
            .lock()
            .expect("presence")
            .viewers
            .values()
            .any(|records| !records.is_empty())
    }

    pub fn records(&self) -> Vec<String> {
        let state = self.state.lock().expect("presence");
        let mut records = BTreeSet::new();
        for record in state.viewers.values().flatten() {
            // Only the first sorted IDs can be requested on the wire. Keep
            // that bounded prefix instead of sorting all viewer memberships.
            if records.len() < RECORD_LIMIT || records.last().is_some_and(|last| record < *last) {
                records.insert(record);
                if records.len() > RECORD_LIMIT {
                    let _ = records.pop_last();
                }
            }
        }
        records.into_iter().cloned().collect()
    }

    pub fn cursor(&self, record: &str, cursor: Cursor) {
        let mut state = self.state.lock().expect("presence");
        state.local.retain(|_, (at, _)| at.elapsed() < TTL);
        let key = (record.into(), cursor.session.clone());
        if state
            .local
            .get(&key)
            .is_some_and(|(at, _)| at.elapsed() < Duration::from_millis(100))
            || (state.local.len() >= 8192 && !state.local.contains_key(&key))
        {
            return;
        }
        state.local.insert(key, (Instant::now(), cursor));
        drop(state);
        self.notify();
    }

    pub fn leave_cursor(&self, record: Option<&str>, session: &str) {
        let mut state = self.state.lock().expect("presence");
        let previous = state.local.len();
        state.local.retain(|(uid, editor), _| {
            editor != session || record.is_some_and(|record| record != uid)
        });
        let membership_changed = if let Some(record) = record {
            if let Some(records) = state.viewers.get_mut(session) {
                let removed = records.remove(record);
                if records.is_empty() {
                    state.viewers.remove(session);
                }
                removed
            } else {
                false
            }
        } else {
            state
                .viewers
                .remove(session)
                .is_some_and(|records| !records.is_empty())
        };
        let changed = membership_changed || previous != state.local.len();
        drop(state);
        if changed {
            self.notify();
        }
    }

    pub fn snapshot(&self, local: &str) -> Vec<Entry> {
        self.snapshot_matching(local, |_| true)
    }

    fn snapshot_for(&self, local: &str, records: &[String]) -> Vec<Entry> {
        if records.is_empty() {
            return Vec::new();
        }
        let records: HashSet<_> = records.iter().map(String::as_str).collect();
        self.snapshot_matching(local, |record| records.contains(record))
    }

    fn snapshot_matching(&self, local: &str, includes: impl Fn(&str) -> bool) -> Vec<Entry> {
        let _preparation = crate::performance::Span::new("presence-preparation");
        let mut state = self.state.lock().expect("presence");
        let entries: Vec<_> = state
            .select(local, includes)
            .into_iter()
            .map(Selected::entry)
            .collect();
        drop(state);
        crate::performance::add("presence-entries-materialized", entries.len() as u64);
        entries
    }

    pub fn replace(&self, peer: &str, entries: Vec<Entry>) {
        let mut state = self.state.lock().expect("presence");
        let previous = state.remote.len();
        state.remote.retain(|_, (at, _)| at.elapsed() < TTL);
        let expired = previous != state.remote.len();
        if state.remote.len() >= 128 && !state.remote.contains_key(peer) {
            return;
        }
        let changed = if entries.is_empty() {
            state.remote.remove(peer).is_some()
        } else {
            let changed = state.remote.get(peer).map(|(_, previous)| previous) != Some(&entries);
            state.remote.insert(peer.into(), (Instant::now(), entries));
            changed
        };
        drop(state);
        if changed || expired {
            self.notify();
        }
    }

    pub fn cursors(&self, record: &str) -> Vec<Cursor> {
        let _preparation = crate::performance::Span::new("presence-preparation");
        let cursors: Vec<_> = self
            .state
            .lock()
            .expect("presence")
            .select("", |uid| uid == record)
            .into_iter()
            .take(CURSOR_LIMIT)
            .map(Selected::cursor)
            .collect();
        crate::performance::add("presence-entries-materialized", cursors.len() as u64);
        cursors
    }

    /// One short-lived snapshot for a refresh's subscribed Records. Delivery
    /// still requires the caller's fresh Record and cursor-owner access checks.
    pub fn cursors_for<'a>(
        &self,
        records: impl IntoIterator<Item = &'a str>,
    ) -> HashMap<&'a str, Vec<Cursor>> {
        let mut cursors: HashMap<_, Vec<_>> =
            records.into_iter().map(|uid| (uid, Vec::new())).collect();
        if cursors.is_empty() {
            return cursors;
        }
        let _preparation = crate::performance::Span::new("presence-preparation");
        let mut state = self.state.lock().expect("presence");
        let selected = state.select("", |uid| cursors.contains_key(uid));
        let mut materialized = 0;
        for entry in selected {
            let record_cursors = cursors.get_mut(entry.record).expect("selected Record");
            if record_cursors.len() < CURSOR_LIMIT {
                record_cursors.push(entry.cursor());
                materialized += 1;
            }
        }
        drop(state);
        crate::performance::add("presence-entries-materialized", materialized);
        cursors
    }
}

impl crate::Engine {
    pub async fn presence_shared(
        &self,
        peer: &str,
        uid: &str,
        property: &str,
    ) -> Result<bool, crate::EngineError> {
        let pool = &self.store.pool;
        let Some(contact) = store::organs::contact(pool, peer).await? else {
            return Ok(false);
        };
        if contact.trust == "blocked" {
            return Ok(false);
        }
        if contact.scope_unreadable.is_some() || contact.accept_unreadable.is_some() {
            return Ok(false);
        }
        let Some(record) = store::records::routing(pool, uid).await? else {
            return Ok(false);
        };
        if record.kind == "message_draft" {
            return Ok(false);
        }
        if store::visibility::hidden_from_organ(pool, peer)
            .await?
            .contains(uid)
        {
            return Ok(false);
        }
        if contact
            .scope_fields
            .as_ref()
            .is_some_and(|fields| !fields.iter().any(|field| field == property))
        {
            return Ok(false);
        }
        if let Some(root) = store::replica::root_of(pool, uid).await? {
            return Ok(store::replica::is_accepted(pool, &root, peer).await?);
        }
        let local = store::organs::local(pool).await?.map(|organ| organ.uid);
        if record.organ_uid == local && !crate::share::open_feed(self, &contact).await?.sends(uid) {
            return Ok(false);
        }
        if record.organ_uid.as_deref() == Some(peer)
            && contact
                .accept_fields
                .as_ref()
                .is_some_and(|fields| !fields.iter().any(|field| field == property))
        {
            return Ok(false);
        }
        Ok(contact.trust == "known"
            && ((record.organ_uid == local && contact.sync_out)
                || (record.organ_uid.as_deref() == Some(peer) && contact.sync_in)))
    }

    pub async fn receive_presence(
        &self,
        peer: &str,
        entries: Vec<Entry>,
    ) -> Result<(), crate::EngineError> {
        self.receive_presence_route(peer, "response", entries).await
    }

    async fn receive_presence_route(
        &self,
        peer: &str,
        route: &str,
        entries: Vec<Entry>,
    ) -> Result<(), crate::EngineError> {
        if entries.len() > CURSOR_LIMIT || entries.iter().any(|entry| !entry.valid()) {
            return Err(crate::EngineError::Consequence(
                "Invalid cursor presence".into(),
            ));
        }
        let local = store::organs::local(&self.store.pool)
            .await?
            .map(|organ| organ.uid)
            .unwrap_or_default();
        let mut accepted = Vec::new();
        for entry in entries {
            if entry.origin == local
                || !self
                    .presence_shared(peer, &entry.record, &entry.cursor.property)
                    .await?
            {
                continue;
            }
            let owner = store::records::organ_uid(&self.store.pool, &entry.record).await?;
            if entry.origin == peer || owner.as_deref() == Some(peer) {
                accepted.push(entry);
            }
        }
        self.presence.replace(&format!("{peer}/{route}"), accepted);
        Ok(())
    }

    pub async fn presence_for(
        &self,
        peer: &str,
        records: &[String],
    ) -> Result<Vec<Entry>, crate::EngineError> {
        let _preparation = crate::performance::Span::new("presence-wire-preparation-await");
        if records.len() > RECORD_LIMIT || records.iter().any(|uid| !nucleus::valid_uid(uid, "r")) {
            return Err(crate::EngineError::Consequence(
                "Invalid cursor subscriptions".into(),
            ));
        }
        let local = store::organs::local(&self.store.pool)
            .await?
            .map(|organ| organ.uid)
            .unwrap_or_default();
        let mut entries = Vec::new();
        for entry in self.presence.snapshot_for(&local, records) {
            if entry.origin == local
                && !self
                    .may_read_record(entry.cursor.person.as_deref(), &entry.record)
                    .await?
            {
                continue;
            }
            if entry.origin == peer
                || !self
                    .presence_shared(peer, &entry.record, &entry.cursor.property)
                    .await?
            {
                continue;
            }
            let owner = store::records::organ_uid(&self.store.pool, &entry.record).await?;
            if entry.origin == local || owner.as_deref() == Some(&local) {
                entries.push(entry);
                if entries.len() == CURSOR_LIMIT {
                    break;
                }
            }
        }
        Ok(entries)
    }

    pub async fn exchange_presence(
        &self,
        peer: &str,
        records: &[String],
        entries: Vec<Entry>,
    ) -> Result<Vec<Entry>, crate::EngineError> {
        if records.len() > RECORD_LIMIT || records.iter().any(|uid| !nucleus::valid_uid(uid, "r")) {
            return Err(crate::EngineError::Consequence(
                "Invalid cursor subscriptions".into(),
            ));
        }
        self.receive_presence_route(peer, "request", entries)
            .await?;
        self.presence_for(peer, records).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relays_keep_original_age_and_expired_cursors_cannot_be_renewed() {
        let presence = Presence::default();
        let entry = Entry {
            record: nucleus::new_uid("r"),
            origin: nucleus::new_uid("r"),
            cursor: Cursor {
                session: "session".into(),
                person: None,
                organ: None,
                property: "body".into(),
                anchor: "a".into(),
                focus: "b".into(),
            },
            age_ms: 19000,
        };
        presence.replace("owner", vec![entry.clone()]);
        assert!(presence.snapshot("local")[0].age_ms >= 19000);
        presence
            .state
            .lock()
            .unwrap()
            .remote
            .get_mut("owner")
            .unwrap()
            .0 -= Duration::from_secs(2);
        assert!(presence.cursors(&entry.record).is_empty());
        let expired = Entry {
            age_ms: 20000,
            ..entry.clone()
        };
        assert!(!expired.valid());
        let oversized = Entry {
            cursor: Cursor {
                focus: "x".repeat(2049),
                ..entry.cursor
            },
            ..entry
        };
        assert!(!oversized.valid());
    }
}
