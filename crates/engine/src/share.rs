use crate::{Engine, EngineError};
use std::collections::HashSet;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ShareChange {
    pub entered: Vec<String>,
    pub left: Vec<String>,
}

impl ShareChange {
    pub fn is_empty(&self) -> bool {
        self.entered.is_empty() && self.left.is_empty()
    }
}

pub fn parse(raw: &str) -> Option<protein::Protein> {
    serde_json::from_str::<protein::Protein>(raw).ok()
}

pub async fn selected(
    engine: &Engine,
    contact_uid: &str,
    raw: &str,
) -> Result<HashSet<String>, EngineError> {
    let Some(protein) = parse(raw) else {
        return Ok(HashSet::new());
    };
    let hidden = store::visibility::hidden_from_organ(&engine.store.pool, contact_uid).await?;
    let matched = protein::matching_records(&engine.store, &protein, None).await?;
    let mut selected = HashSet::new();
    for row in matched {
        if hidden.contains(&row.uid) {
            continue;
        }
        selected.insert(row.uid);
    }
    Ok(selected)
}

const TOUCHED_PER_PASS: i64 = 512;

pub async fn touched_since(
    engine: &Engine,
    local_organ: &str,
    seen: i64,
) -> Result<(HashSet<String>, i64), EngineError> {
    let pool = &engine.store.pool;
    let mut touched = HashSet::new();
    let mut mark = seen;
    loop {
        let ops = store::sync_ops::after(pool, local_organ, mark, TOUCHED_PER_PASS).await?;
        if ops.is_empty() {
            break;
        }
        for op in &ops {
            for uid in store::visibility::records_of_op(pool, &op.tbl, &op.uid).await? {
                touched.insert(uid);
            }
            if op.tbl == "record_assertion" {
                widen_through_the_tree(engine, &op.uid, &mut touched).await?;
            }
            mark = mark.max(op.seq);
        }
        if (ops.len() as i64) < TOUCHED_PER_PASS {
            break;
        }
    }
    if mark == seen {
        mark = store::sync_ops::max_seq(pool).await?.max(seen);
    }
    Ok((touched, mark))
}

async fn widen_through_the_tree(
    engine: &Engine,
    assertion_uid: &str,
    touched: &mut HashSet<String>,
) -> Result<(), EngineError> {
    let pool = &engine.store.pool;
    let Some(link) = store::assertions::get(pool, assertion_uid).await? else {
        return Ok(());
    };
    if link.object_uid.is_none() {
        return Ok(());
    }
    let family = store::concepts::descendants_including(pool, &link.predicate_uid).await?;
    for uid in store::assertions::record_descendants(pool, &link.subject_uid, &family, true).await?
    {
        touched.insert(uid);
    }
    Ok(())
}

pub async fn reconcile_contact(
    engine: &Engine,
    contact: &store::organs::Contact,
) -> Result<ShareChange, EngineError> {
    let Some(raw) = contact.share_protein.as_deref() else {
        return Ok(ShareChange::default());
    };
    let pool = &engine.store.pool;
    let picked = store::contact_share::picked(pool, &contact.record_uid).await?;

    let (wanted, considered, mark) = match contact.share_seen_seq {
        None => {
            let mark = store::sync_ops::max_seq(pool).await?;
            (
                selected(engine, &contact.record_uid, raw).await?,
                None,
                mark,
            )
        }
        Some(seen) => {
            let local = store::organs::local(pool)
                .await?
                .map(|organ| organ.uid)
                .unwrap_or_default();
            let (touched, mark) = touched_since(engine, &local, seen).await?;
            let Some(protein) = parse(raw) else {
                return Ok(ShareChange::default());
            };
            let hidden = store::visibility::hidden_from_organ(pool, &contact.record_uid).await?;
            let mut wanted = protein::matching_among(&engine.store, &protein, &touched).await?;
            wanted.retain(|uid| !hidden.contains(uid));
            (wanted, Some(touched), mark)
        }
    };

    let moving = store::record_move::to_contact(pool, &contact.record_uid).await?;
    let entered: Vec<String> = wanted.difference(&picked).cloned().collect();
    let left: Vec<String> = picked
        .iter()
        .filter(|uid| !wanted.contains(*uid))
        .filter(|uid| !moving.contains(*uid))
        .filter(|uid| considered.as_ref().is_none_or(|set| set.contains(*uid)))
        .cloned()
        .collect();

    let mut tx = store::write_tx(pool).await?;
    for uid in entered.iter().chain(left.iter()) {
        let picked_now = wanted.contains(uid);
        store::contact_share::set_picked_tx(&mut tx, &contact.record_uid, uid, picked_now).await?;
    }
    tx.commit().await?;

    for uid in &entered {
        store::sync_ops::enqueue_record_for_contact(pool, &contact.record_uid, uid).await?;
    }

    let mut settle = store::write_tx(pool).await?;
    store::contact_share::set_watermark_tx(&mut settle, &contact.record_uid, mark).await?;
    settle.commit().await?;

    let mut change = ShareChange { entered, left };
    change.entered.sort();
    change.left.sort();
    Ok(change)
}

pub async fn reconcile_all(engine: &Engine) -> Result<Vec<(String, ShareChange)>, EngineError> {
    let pool = &engine.store.pool;
    let local = store::organs::local(pool).await?.map(|o| o.uid);
    let mut changed = Vec::new();
    for contact in store::organs::contacts(pool).await? {
        if Some(&contact.record_uid) == local.as_ref() {
            continue;
        }
        if contact.trust == "blocked" || !contact.sync_out {
            continue;
        }
        let change = reconcile_contact(engine, &contact).await?;
        if !change.is_empty() {
            changed.push((contact.record_uid.clone(), change));
        }
    }
    Ok(changed)
}

pub async fn narrow_to_feed(
    engine: &Engine,
    contact_uid: &str,
    rows: Vec<store::sync_ops::OpRow>,
) -> Result<Vec<store::sync_ops::OpRow>, EngineError> {
    let Some(contact) = store::organs::contact(&engine.store.pool, contact_uid).await? else {
        return Ok(rows);
    };
    let feed = open_feed(engine, &contact).await?;
    let mut kept = Vec::with_capacity(rows.len());
    for row in rows {
        if feed_carries(engine, &feed, &row).await?.is_some() {
            kept.push(row);
        }
    }
    Ok(kept)
}

#[derive(Debug, Default, Clone)]
pub struct Holdings {
    pub gained: HashSet<String>,
    pub lost: HashSet<String>,
}

impl Holdings {
    pub fn note(&mut self, op: &store::sync_ops::OpRow, records: Vec<String>) {
        let removes = op.tbl == "record" && op.kind == store::sync_ops::OpKind::Tombstone.as_str();
        for record in records {
            if removes {
                self.gained.remove(&record);
                self.lost.insert(record);
            } else if !self.lost.contains(&record) {
                self.gained.insert(record);
            }
        }
    }
}

pub async fn note_delivery(
    engine: &Engine,
    contact_uid: &str,
    holdings: &Holdings,
) -> Result<(), EngineError> {
    let pool = &engine.store.pool;
    for record in &holdings.gained {
        store::contact_share::mark_held(pool, contact_uid, record).await?;
    }
    for record in &holdings.lost {
        store::contact_share::forget(pool, contact_uid, record).await?;
    }
    Ok(())
}

#[derive(Debug, Default, Clone)]
pub struct Feed {
    pub selection: Option<HashSet<String>>,
    pub hidden: HashSet<String>,
    pub held: HashSet<String>,
    pub moving_to_them: HashSet<String>,
}

impl Feed {
    pub fn sends(&self, record: &str) -> bool {
        let picked = self
            .selection
            .as_ref()
            .is_none_or(|selection| selection.contains(record));
        picked && !self.hidden.contains(record)
    }
}

pub async fn open_feed(
    engine: &Engine,
    contact: &store::organs::Contact,
) -> Result<Feed, EngineError> {
    let pool = &engine.store.pool;
    let selection = match contact.share_protein {
        Some(_) => Some(store::contact_share::picked(pool, &contact.record_uid).await?),
        None => None,
    };
    Ok(Feed {
        selection,
        hidden: store::visibility::hidden_from_organ(pool, &contact.record_uid).await?,
        held: store::contact_share::held(pool, &contact.record_uid).await?,
        moving_to_them: store::record_move::to_contact(pool, &contact.record_uid).await?,
    })
}

pub async fn feed_carries(
    engine: &Engine,
    feed: &Feed,
    op: &store::sync_ops::OpRow,
) -> Result<Option<Vec<String>>, EngineError> {
    let records = store::visibility::records_of_op(&engine.store.pool, &op.tbl, &op.uid).await?;
    if records.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let removes = op.tbl == "record" && op.kind == store::sync_ops::OpKind::Tombstone.as_str();
    let carried: Vec<String> = records
        .into_iter()
        .filter(|record| {
            if removes {
                (feed.held.contains(record) || feed.sends(record))
                    && !feed.moving_to_them.contains(record)
            } else {
                // A handover outranks the selection. They are about to OWN
                // this Record, and a Record cannot be filtered out of its own
                // handover by a rule about what they are usually sent.
                feed.sends(record) || feed.moving_to_them.contains(record)
            }
        })
        .collect();
    if carried.is_empty() {
        return Ok(None);
    }
    Ok(Some(carried))
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Moved {
    pub handed_over: Vec<String>,
}

pub async fn settle_moves(engine: &Engine) -> Result<Moved, EngineError> {
    let pool = &engine.store.pool;
    let mut moved = Moved::default();
    for pending in store::record_move::pending(pool).await? {
        let Some(contact) = store::organs::contact(pool, &pending.contact_organ).await? else {
            store::record_move::forget(pool, &pending.record_uid).await?;
            continue;
        };
        if store::records::get(pool, &pending.record_uid)
            .await?
            .is_none()
        {
            store::record_move::forget(pool, &pending.record_uid).await?;
            continue;
        }
        if store::record_move::still_queued(pool, &pending.contact_organ, &pending.record_uid)
            .await?
        {
            continue;
        }
        let last = store::record_move::last_op_seq(pool, &pending.record_uid).await?;
        if last == 0 || contact.peer_acked_seq < last {
            continue;
        }
        store::records::mark_deleted(pool, &pending.record_uid).await?;
        store::contact_share::forget(pool, &pending.contact_organ, &pending.record_uid).await?;
        store::record_move::mark_handed_over(pool, &pending.record_uid).await?;
        moved.handed_over.push(pending.record_uid);
    }
    moved.handed_over.sort();
    Ok(moved)
}
