//! Sync op log (Ontology §11): the outbound feed of field-level operations.
//!
//! Every local write on a syncable table calls `log_local`/`log_local_tx`
//! right where the SQL write happens, so the log and the read model can never
//! disagree. Identity is minted by `Store::open`, so there is no such thing as
//! a Cell that writes without one.
//!
//! Applying a REMOTE op must NOT go through these helpers: the import path
//! appends the op with its ORIGINAL identity (actor_cell, hlc) via `append`,
//! so it can still be served on a catch-up feed to a peer that asks for our
//! log. It is not pushed onward — relaying is off, see `append`.

use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Set,
    Tombstone,
    Fact,
    Crdt,
    /// A full Loro snapshot of one record-doc, asserted by the Cell that
    /// compacted it. What makes the log authoritative for collaborative text:
    /// without it a rebuild from the log alone cannot reconstruct text, and
    /// `crdt` ops can never be pruned because each is only cumulative since a
    /// snapshot that lives outside the log.
    Snapshot,
}

impl OpKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Set => "set",
            Self::Tombstone => "tombstone",
            Self::Fact => "fact",
            Self::Crdt => "crdt",
            Self::Snapshot => "snapshot",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "set" => Self::Set,
            "tombstone" => Self::Tombstone,
            "fact" => Self::Fact,
            "crdt" => Self::Crdt,
            "snapshot" => Self::Snapshot,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct OpRow {
    pub seq: i64,
    pub tbl: String,
    pub uid: String,
    pub field: String,
    pub kind: String,
    pub value: Option<String>,
    pub hlc: i64,
    pub actor_cell: String,
    /// The Organ this op is attributed to — the published identity, not the
    /// device. Travels explicitly rather than being derived from `actor_cell`,
    /// because deriving it would make one person look like several Organs.
    pub organ_uid: String,
    /// The individual-replica root this op belongs to, or `None` for the
    /// general feed. Decides WHICH channel the op may leave on.
    pub replica_root: Option<String>,
}

fn map(row: sqlx::sqlite::SqliteRow) -> OpRow {
    OpRow {
        seq: row.get("seq"),
        tbl: row.get("tbl"),
        uid: row.get("uid"),
        field: row.get("field"),
        kind: row.get("kind"),
        value: row.get("value"),
        hlc: row.get("hlc"),
        actor_cell: row.get("actor_cell"),
        organ_uid: row.get("organ_uid"),
        replica_root: row.get("replica_root"),
    }
}

const INSERT: &str = "INSERT OR IGNORE INTO sync_op
    (tbl, uid, field, kind, value, hlc, actor_cell, organ_uid, replica_root)
    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)";

/// Queue one op for every contact holding an ACCEPTED grant on its root — the
/// individual-replica counterpart of `ENQUEUE`. A Record inside a root never
/// rides the general feed, so exactly one of the two statements runs per op.
///
/// `trust != 'blocked'` matters as much here as on the general feed: blocked
/// is terminal everywhere (Ontology §2), and a grant does not survive it.
const ENQUEUE_GRANT: &str =
    "INSERT INTO sync_outbox (contact_organ, tbl, uid, field, kind, seq, queued_at)
    SELECT g.contact_organ, ?, ?, ?, ?, ?, ?
      FROM replica_grant g
      JOIN organ_contact c ON c.record_uid = g.contact_organ
     WHERE g.root_record = ? AND g.state = 'accepted'
       AND c.trust != 'blocked' AND g.contact_organ != ?
    ON CONFLICT(contact_organ, tbl, uid, field, kind)
    DO UPDATE SET seq = excluded.seq, queued_at = excluded.queued_at";

/// Queue one op for every sync-out contact (except an optional excluded one),
/// replacing any older queued op on the same (contact, tbl, uid, field) —
/// the bounded outbox (Ontology §11 "Reactive deltas").
const ENQUEUE: &str = "INSERT INTO sync_outbox (contact_organ, tbl, uid, field, kind, seq, queued_at)
    SELECT record_uid, ?, ?, ?, ?, ?, ?
      FROM organ_contact
     WHERE sync_out = 1 AND trust != 'blocked' AND record_uid != ?
    ON CONFLICT(contact_organ, tbl, uid, field, kind)
    DO UPDATE SET seq = excluded.seq, queued_at = excluded.queued_at";

/// Append an op with an explicit identity — the import path, and collab's
/// local crdt writes. Returns the local seq, or `None` when the identity
/// already exists (idempotent import).
///
/// `from_contact` is the Organ this op arrived FROM, or `None` for a local
/// write. It decides whether the op is queued outward at all — see below.
pub async fn append(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
    field: &str,
    kind: OpKind,
    value: Option<&str>,
    hlc: i64,
    actor_cell: &str,
    organ_uid: &str,
    from_contact: Option<&str>,
    replica_root: Option<&str>,
) -> Result<Option<i64>, StoreError> {
    // A LOCAL write (`from_contact` is None — collab's crdt tails and
    // snapshots) re-mints its stamp on collision, exactly as `log_local` does:
    // within one process `hlc::next()` cannot repeat, so a taken identity can
    // only mean another process writing as this same Cell, and returning
    // `None` there would drop the write. An IMPORT keeps the opposite
    // behaviour, because for an import a taken identity genuinely means "I
    // already have this op".
    let seq = if from_contact.is_none() {
        match insert_local(
            pool,
            tbl,
            uid,
            field,
            kind,
            value,
            actor_cell,
            organ_uid,
            replica_root,
            Some(hlc),
        )
        .await?
        {
            Some(seq) => seq,
            None => return Ok(None),
        }
    } else {
        let res = sqlx::query(INSERT)
            .bind(tbl)
            .bind(uid)
            .bind(field)
            .bind(kind.as_str())
            .bind(value)
            .bind(hlc)
            .bind(actor_cell)
            .bind(organ_uid)
            .bind(replica_root)
            .execute(pool)
            .await?;
        if res.rows_affected() == 0 {
            return Ok(None);
        }
        res.last_insert_rowid()
    };
    let now = chrono::Utc::now().to_rfc3339();
    // RELAYING IS OFF (Ontology §11 "Op authenticity"). An op that arrived
    // from a contact is stored — so it can still be served on a catch-up feed
    // to someone who asks us for our log — but it is not pushed onward.
    //
    // This is what makes attribution free. With relay on, the Organ on the
    // other end of a connection is a carrier rather than the author, so
    // `op.organ_uid == batch.from_organ` cannot hold and the receiver has no
    // way to tell a forged attribution from a relayed one without signatures
    // that do not exist yet. Off, the rule holds by construction and the whole
    // forgery class disappears.
    //
    // The availability case relaying was meant to serve is already covered: a
    // contact tracks per-contact op progress, so when a peer comes back online
    // it compares cursors and sends exactly what that peer is missing.
    //
    // `from_contact.is_some()` IS "this op came off the wire" — local writes
    // (including collab's crdt appends) pass `None` and are queued normally.
    if from_contact.is_some() {
        return Ok(Some(seq));
    }
    // A SNAPSHOT is logged and served, never pushed. A peer that is keeping up
    // already holds every op the snapshot folds together, so sending it a full
    // doc copy on every compaction is pure waste; a peer starting from zero
    // picks it up from the catch-up feed by seq range, which is precisely how
    // replica bootstrap stops being a second mechanism.
    if matches!(kind, OpKind::Snapshot) {
        return Ok(Some(seq));
    }
    // An op inside a root goes to that root's grant holders and never to the
    // general feed.
    match replica_root {
        Some(root) => {
            sqlx::query(ENQUEUE_GRANT)
                .bind(tbl)
                .bind(uid)
                .bind(field)
                .bind(kind.as_str())
                .bind(seq)
                .bind(&now)
                .bind(root)
                .bind(from_contact.unwrap_or(""))
                .execute(pool)
                .await?;
        }
        None => {
            sqlx::query(ENQUEUE)
                .bind(tbl)
                .bind(uid)
                .bind(field)
                .bind(kind.as_str())
                .bind(seq)
                .bind(&now)
                .bind(from_contact.unwrap_or(""))
                .execute(pool)
                .await?;
        }
    }
    Ok(Some(seq))
}

/// This Cell's op identity: `(actor_cell, organ_uid)` — who wrote it and whose
/// it is. Both come from the Cell Record, which `Store::open` mints, so the
/// query is one indexed lookup and the `None` arm is unreachable in a store
/// that opened successfully.
///
/// It is still an `Option` rather than an error because the honest answer to
/// "no identity" is not "fail this write" — it is "this cannot happen", and a
/// caller that finds otherwise should skip the op rather than lose the write.
const LOCAL_IDENTITY: &str =
    "SELECT uid, organ_uid FROM record WHERE slug = ? AND kind = 'device' LIMIT 1";

async fn local_identity_tx(
    tx: &mut Transaction<'_, Sqlite>,
) -> Result<Option<(String, String)>, StoreError> {
    Ok(sqlx::query(LOCAL_IDENTITY)
        .bind(crate::cells::LOCAL_CELL_SLUG)
        .fetch_optional(&mut **tx)
        .await?
        .map(|row| (row.get("uid"), row.get("organ_uid"))))
}

async fn local_identity(pool: &SqlitePool) -> Result<Option<(String, String)>, StoreError> {
    Ok(sqlx::query(LOCAL_IDENTITY)
        .bind(crate::cells::LOCAL_CELL_SLUG)
        .fetch_optional(pool)
        .await?
        .map(|row| (row.get("uid"), row.get("organ_uid"))))
}

/// Log a LOCAL write as an op: the actor is this CELL, the attribution is its
/// Organ, and the stamp is this Cell's HLC.
///
/// How many times a local write re-mints its stamp after an identity
/// collision. Collisions are rare and each attempt jumps past everything the
/// other process has written, so this is generous rather than tuned.
const LOCAL_STAMP_ATTEMPTS: usize = 8;

/// Insert a LOCAL op, re-minting the stamp if the identity is already taken.
///
/// A local write and an import mean opposite things by "this identity already
/// exists". For an import it means "I already have this op" and skipping is
/// correct. For a local write it is impossible within one process —
/// `hlc::next()` is strictly monotonic — so it can only mean ANOTHER PROCESS
/// is writing as the same Cell, and skipping silently discards a write that
/// exists in the read model and will reach nobody.
///
/// Found by the multi-process harness (`tests/multi_process.rs`), which lost 7
/// of 80 writes on its first run. Not exotic: two processes share a Cell
/// whenever the CLI touches the database while the web Cell is running, which
/// SQLite in WAL mode permits, and both then run their own `nucleus::hlc`
/// static over the same `actor_cell`.
///
/// The retry jumps rather than nudges: on collision it observes the highest
/// stamp the other process has published for this Cell, so the next attempt
/// lands above all of it instead of walking into the same wall repeatedly.
/// `first_hlc` is the stamp the caller already minted, used for the first
/// attempt so the identity a caller believes it wrote is the one it gets.
/// Re-minting happens only on collision.
#[allow(clippy::too_many_arguments)]
async fn insert_local(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
    field: &str,
    kind: OpKind,
    value: Option<&str>,
    actor_cell: &str,
    organ_uid: &str,
    replica_root: Option<&str>,
    first_hlc: Option<i64>,
) -> Result<Option<i64>, StoreError> {
    let mut stamp = first_hlc.unwrap_or_else(nucleus::hlc::next);
    for attempt in 0..LOCAL_STAMP_ATTEMPTS {
        if attempt > 0 {
            stamp = nucleus::hlc::next();
        }
        let res = sqlx::query(INSERT)
            .bind(tbl)
            .bind(uid)
            .bind(field)
            .bind(kind.as_str())
            .bind(value)
            .bind(stamp)
            .bind(actor_cell)
            .bind(organ_uid)
            .bind(replica_root)
            .execute(pool)
            .await?;
        if res.rows_affected() > 0 {
            return Ok(Some(res.last_insert_rowid()));
        }
        // `adopt_own`, not `observe`: this is a stamp THIS Cell wrote, read back
        // from its own log. `observe` is bounded against a hostile peer
        // dragging the clock forward, and applying that bound here would make
        // the two guards fight — a skewed stamp beyond the window would be
        // ignored, every retry would land in the same occupied range, and an
        // ordinary local write would fail.
        if let Some(theirs) = max_hlc_for_actor(pool, actor_cell).await? {
            nucleus::hlc::adopt_own(theirs);
        }
    }
    // Refusing loudly beats writing the read model and losing the op, which is
    // the failure this function exists to end.
    Err(sqlx::Error::Protocol(format!(
        "could not mint a free op identity for cell {actor_cell} after \
         {LOCAL_STAMP_ATTEMPTS} attempts"
    )))
}

/// The highest stamp any process has recorded for this Cell.
async fn max_hlc_for_actor(
    pool: &SqlitePool,
    actor_cell: &str,
) -> Result<Option<i64>, StoreError> {
    Ok(
        sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(hlc) FROM sync_op WHERE actor_cell = ?")
            .bind(actor_cell)
            .fetch_one(pool)
            .await?,
    )
}

/// The actor must be the Cell — see `0040_sync_op.sql`. With the Organ there,
/// a second Cell's op collides on `UNIQUE(actor_cell, hlc)` and import drops
/// it as a duplicate, silently.
pub async fn log_local(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
    field: &str,
    kind: OpKind,
    value: Option<String>,
) -> Result<(), StoreError> {
    let Some((actor_cell, organ_uid)) = local_identity(pool).await? else {
        return Ok(());
    };
    // Resolved BEFORE the op is written, from the row it targets. Because
    // `record.replica_root` is stamped at creation and immutable, this is the
    // same answer forever — which is what lets every later reader trust the
    // copy on the op instead of re-deriving it.
    let root = crate::replica::root_for_op(pool, tbl, uid).await?;
    let res = insert_local(
        pool,
        tbl,
        uid,
        field,
        kind,
        value.as_deref(),
        &actor_cell,
        &organ_uid,
        root.as_deref(),
        None,
    )
    .await?;
    if let Some(seq) = res {
        let now = chrono::Utc::now().to_rfc3339();
        match &root {
            // Individually replicated: grant holders only, and NOT the general
            // feed. Riding both would be the leak the whole axis exists to
            // prevent.
            Some(root) => {
                sqlx::query(ENQUEUE_GRANT)
                    .bind(tbl)
                    .bind(uid)
                    .bind(field)
                    .bind(kind.as_str())
                    .bind(seq)
                    .bind(&now)
                    .bind(root)
                    .bind("")
                    .execute(pool)
                    .await?;
            }
            None => {
                sqlx::query(ENQUEUE)
                    .bind(tbl)
                    .bind(uid)
                    .bind(field)
                    .bind(kind.as_str())
                    .bind(seq)
                    .bind(&now)
                    .bind("")
                    .execute(pool)
                    .await?;
            }
        }
    }
    Ok(())
}

/// `log_local` inside a caller-owned transaction (the in-memory pool has one
/// connection, so a pool query while a tx is open would deadlock).
pub async fn log_local_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tbl: &str,
    uid: &str,
    field: &str,
    kind: OpKind,
    value: Option<String>,
) -> Result<(), StoreError> {
    let Some((actor_cell, organ_uid)) = local_identity_tx(tx).await? else {
        return Ok(());
    };
    // Same identity-collision retry as `insert_local`, inlined because this
    // path owns a transaction and cannot hand the pool to a helper. A local
    // write that finds its identity taken has met ANOTHER PROCESS writing as
    // the same Cell; skipping would lose the write silently.
    let mut seq = None;
    for _ in 0..LOCAL_STAMP_ATTEMPTS {
        let res = sqlx::query(INSERT)
            .bind(tbl)
            .bind(uid)
            .bind(field)
            .bind(kind.as_str())
            .bind(&value)
            .bind(nucleus::hlc::next())
            .bind(&actor_cell)
            .bind(&organ_uid)
            .bind(None::<String>)
            .execute(&mut **tx)
            .await?;
        if res.rows_affected() > 0 {
            seq = Some(res.last_insert_rowid());
            break;
        }
        let theirs: Option<i64> =
            sqlx::query_scalar("SELECT MAX(hlc) FROM sync_op WHERE actor_cell = ?")
                .bind(&actor_cell)
                .fetch_one(&mut **tx)
                .await?;
        // `adopt_own` — our own stamp, unbounded. See `insert_local`.
        if let Some(theirs) = theirs {
            nucleus::hlc::adopt_own(theirs);
        }
    }
    let Some(seq) = seq else {
        return Err(sqlx::Error::Protocol(format!(
            "could not mint a free op identity for cell {actor_cell}"
        )));
    };
    {
        sqlx::query(ENQUEUE)
            .bind(tbl)
            .bind(uid)
            .bind(field)
            .bind(kind.as_str())
            .bind(seq)
            .bind(chrono::Utc::now().to_rfc3339())
            .bind("")
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

/// Ops past a checkpoint, oldest first — the catch-up feed. One indexed
/// rowid-range query; an empty answer means converged.
/// The GENERAL feed only: `replica_root IS NULL`.
///
/// This is enforcement point two of three. An individually-replicated Record
/// must never appear here, or a contact with plain `sync_in` would receive a
/// conversation they were never granted — which is the exact failure the axis
/// exists to prevent, and it would be invisible because the catch-up feed is
/// served automatically.
///
/// Scoped to ops this Organ AUTHORED. Relaying is off (see `append`), and it
/// has to be off on BOTH directions to mean anything: without this filter the
/// catch-up feed served every op in the log, including ones imported from a
/// third Organ. The receiver rejects those — `organ_uid` must equal the
/// sending Organ — so they arrived as pure quarantine noise, and on the way
/// they told the peer the uids and values of Records authored by someone else
/// entirely. Push had this by construction; pull did not.
pub async fn after(
    pool: &SqlitePool,
    organ_uid: &str,
    seq: i64,
    limit: i64,
) -> Result<Vec<OpRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM sync_op
          WHERE seq > ? AND replica_root IS NULL AND organ_uid = ?
          ORDER BY seq LIMIT ?",
    )
    .bind(seq)
    .bind(organ_uid)
    .bind(limit)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map)
    .collect())
}

/// One entry of a version vector: a Cell, and the highest stamp we hold from
/// it. The whole vector is "what I already have", stated in terms both sides
/// agree on.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VectorEntry {
    pub actor_cell: String,
    pub max_hlc: i64,
}

/// What we hold of ONE Organ's ops, keyed by the Cell that wrote each.
///
/// The idiom is Automerge's sync protocol and Loro's `ExportMode::updates(vv)`:
/// exchange a compact per-ACTOR summary and derive the difference, rather than
/// walking records. It costs one entry per device instead of one per record,
/// and it works here for free because op identity is already `(actor_cell,
/// hlc)`.
///
/// It replaces a cursor that could not survive the peer's own maintenance.
/// `organ_contact.last_synced_seq` is the PEER'S local seq — a number that
/// means nothing once they prune, and nothing at all for ops that reached them
/// from somewhere else. A version vector is stated in stamps both sides
/// already share, so neither pruning nor a different arrival path disturbs it.
///
/// **Scoped to one Organ deliberately.** Sending our whole vector would tell a
/// contact which Cells of OTHER Organs we sync with — third parties who never
/// agreed to be mentioned. Asking only about theirs says "here is what I have
/// of YOURS; send me the rest" and reveals nothing about anyone else.
pub async fn version_vector_for_organ(
    pool: &SqlitePool,
    organ_uid: &str,
) -> Result<Vec<VectorEntry>, StoreError> {
    Ok(sqlx::query(
        "SELECT actor_cell, MAX(hlc) AS max_hlc FROM sync_op
          WHERE organ_uid = ? AND replica_root IS NULL
          GROUP BY actor_cell ORDER BY actor_cell",
    )
    .bind(organ_uid)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| VectorEntry {
        actor_cell: row.get("actor_cell"),
        max_hlc: row.get("max_hlc"),
    })
    .collect())
}

/// The SQL for "this op is NOT covered by their vector", plus the binds it
/// needs, in order.
///
/// Built as a clause rather than filtered in Rust because both callers run per
/// CONTACT per sync PASS. The first version loaded the whole general feed into
/// memory and filtered afterwards, applying `limit` at the end — which turned
/// an indexed range scan into a full read of the log on every pass, for every
/// contact, forever. A vector has one entry per device, so the clause stays
/// short.
fn uncovered_clause(theirs: &[VectorEntry]) -> String {
    if theirs.is_empty() {
        // They hold nothing of ours: everything is missing.
        return String::new();
    }
    let mut clause = String::from(" AND NOT (");
    for index in 0..theirs.len() {
        if index > 0 {
            clause.push_str(" OR ");
        }
        clause.push_str("(actor_cell = ? AND hlc <= ?)");
    }
    clause.push(')');
    clause
}

/// Our own ops that the holder of `theirs` does not have yet.
///
/// A Cell absent from their vector means they have nothing from it, so every
/// op it wrote is missing — which is exactly how a brand-new contact, or one
/// that has never heard of a device you enrolled last week, gets caught up
/// with no special case. An EMPTY vector therefore means "send me everything",
/// which is what a first sync is.
///
/// Ordered by seq so a limited page is a prefix and the caller simply asks
/// again; ordering by hlc would interleave Cells and make "the next page"
/// ambiguous.
/// Drop ops for columns a contact's scope does not name (Ontology §12, C5).
///
/// **A TOMBSTONE IS NOT A FIELD, and rides regardless.** A tombstone uses
/// `field = ''`, so an allowlist naming columns would exclude every DELETE by
/// construction — a narrowed contact would never learn a Record was removed
/// and their copy would live forever, which is worse than the leak the
/// narrowing was for. Empty-field is a real value here and never a wildcard,
/// the same rule the supersede predicate follows.
///
/// **`crdt` and `snapshot` are NOT tombstones, and this is the distinction
/// the first version of this function got wrong.** They share the empty
/// `field`, so keying only on that let them through every scope — and they
/// carry the whole Loro document, which holds `head` and `body`. A scope
/// excluding both still shipped both, silently, which is precisely the leak
/// the narrowing exists to prevent. Withholding them strands nothing (unlike
/// a delete), so they are filtered like the columns they actually carry.
///
/// **Collaborative text cannot be SPLIT**: `head` and `body` live in one Loro
/// document, so a scope wanting one gets both. That is refused where the
/// scope is SET rather than silently honoured here — at configuration time
/// there is somebody to tell.
///
/// Applied at SERVE time and nowhere else — no per-contact state on the write
/// path, which is what keeps one log serving every contact differently.
pub fn narrow_ops_to_scope(ops: Vec<OpRow>, scope: Option<&[String]>) -> Vec<OpRow> {
    let Some(scope) = scope else {
        return ops;
    };
    ops.into_iter()
        .filter(|op| op_in_scope(&op.tbl, &op.kind, &op.field, Some(scope)))
        .collect()
}

/// Whether one op belongs inside a scope — the single predicate BOTH
/// directions use.
///
/// Outbound narrowing (what a contact may see of us) and inbound acceptance
/// (what we take from them) are different policies over the same question, and
/// the question has enough edges — the tombstone exemption, the Loro document
/// carrying two columns under an empty field — that deriving it twice means
/// deriving it differently. The second copy would be the one that leaks, and
/// nothing would fail until it did. So there is one copy.
///
/// **It takes `tbl`, and needs to.** The first two versions keyed on `kind` and
/// `field` alone and leaked twice for the same reason: FIVE logged tables use
/// `field = ''` for ops that are not field-less at all, and a rule reading
/// "empty field rides" waved every one of them through — including under the
/// EMPTY scope, the one that asks for nothing. `kind` cannot tell a Record
/// tombstone (which must always ride) from an Assertion tombstone (which must
/// not), because both are spelled `tombstone`. Only the table can.
///
/// The shape is now an ALLOW-list over tables, matching the cluster's own rule
/// that a scope names what you want rather than what to hide: a sixth logged
/// table travels to nobody narrowed until someone decides what column it
/// carries, which is a visible omission rather than a silent leak.
pub fn op_in_scope(tbl: &str, kind: &str, field: &str, scope: Option<&[String]>) -> bool {
    let Some(scope) = scope else {
        return true;
    };
    let names = |wanted: &str| scope.iter().any(|s| s == wanted);
    match tbl {
        "record" => match kind {
            // A delete is not a column and is never withheld. Outbound,
            // dropping it would leave a contact holding a Record we removed;
            // inbound, refusing it would leave US holding one they removed.
            // Same rule, same reason, both ways round. This exemption belongs
            // to the RECORD table alone — it exists because a withheld delete
            // strands a row the peer already has, and nothing else here can
            // strand anything.
            "tombstone" => true,
            // These carry the whole Loro document, which holds `head` and
            // `body` and nothing else. Treating them as field-less let both
            // columns ride past every scope that excluded them. They are one
            // unit, so the test is whether the scope wants EITHER; naming
            // exactly one of the two is refused where the scope is set,
            // because there it can still be reported.
            "crdt" | "snapshot" => names("head") || names("body"),
            // A Record `set` always names its column. An empty one would be a
            // malformed op, and reading it as a wildcard is exactly the bug
            // this predicate has now had twice.
            "set" => !field.is_empty() && names(field),
            _ => false,
        },
        // A fact is a signed delta to `quantity` and nothing else, so that is
        // the column it answers for. It logs under an empty field because the
        // fact's own uid is the identity — which is not the same as being
        // field-less, and the difference is a leak: a contact scoped to `head`
        // was receiving every quantity change we made.
        "fact" => names("quantity"),
        // An Assertion is a relationship BETWEEN Records, not a column of one,
        // and the column vocabulary cannot name it. So a narrowed contact
        // receives no links at all — including their tombstones, which strand
        // nothing because the Assertion never arrived.
        //
        // Fail-closed rather than inventing a reserved word for it: letting
        // links through would disclose the shape of the graph to someone
        // narrowed to one column, which is most of what narrowing was for.
        // Naming links in a scope is a vocabulary extension and is deferred.
        "record_assertion" => false,
        // Extensions DO name a real key (`namespace.key`), so they follow the
        // ordinary column rule. In practice this withholds them from every
        // narrowed contact, because nobody types `lince.file_sync.enabled`
        // into a scope — which is the right default and is now the deliberate
        // one rather than an accident of the empty-field rule.
        "record_extension" => !field.is_empty() && names(field),
        // A Concept is shared vocabulary rather than anyone's content, and a
        // narrowed contact gets none of it. The cost is honest and small: a
        // column holding a concept uid arrives opaque, exactly as a withheld
        // column arrives absent. Riding unconditionally would have told a
        // contact narrowed to one column the whole vocabulary of this Cell.
        "concept" => false,
        // An unknown table is not silently shared, for the same reason
        // `visibility::records_of_op` returns no governing Record for one.
        _ => false,
    }
}

pub async fn ops_missing_from_vector(
    pool: &SqlitePool,
    organ_uid: &str,
    theirs: &[VectorEntry],
    limit: i64,
) -> Result<Vec<OpRow>, StoreError> {
    let sql = format!(
        "SELECT * FROM sync_op
          WHERE organ_uid = ? AND replica_root IS NULL{}
          ORDER BY seq LIMIT ?",
        uncovered_clause(theirs)
    );
    let mut query = sqlx::query(&sql).bind(organ_uid);
    for entry in theirs {
        query = query.bind(&entry.actor_cell).bind(entry.max_hlc);
    }
    Ok(query
        .bind(limit.max(0))
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map)
        .collect())
}

/// The same difference, scoped to ONE conversation root.
///
/// The grant channel used to re-fetch a conversation's entire history every
/// pass — `after: 0`, hardcoded — which was idempotent and therefore invisible,
/// and O(history) per conversation per pass forever. It also meant the grant
/// channel had no retention floor at all, because nothing was ever confirmed
/// received.
pub async fn ops_missing_from_vector_in_root(
    pool: &SqlitePool,
    root: &str,
    theirs: &[VectorEntry],
    limit: i64,
) -> Result<Vec<OpRow>, StoreError> {
    let sql = format!(
        "SELECT * FROM sync_op
          WHERE replica_root = ?{}
          ORDER BY seq LIMIT ?",
        uncovered_clause(theirs)
    );
    let mut query = sqlx::query(&sql).bind(root);
    for entry in theirs {
        query = query.bind(&entry.actor_cell).bind(entry.max_hlc);
    }
    Ok(query
        .bind(limit.max(0))
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map)
        .collect())
}

/// What we hold inside ONE conversation root, keyed by the Cell that wrote it.
/// Both parties write here, so unlike the general feed this legitimately names
/// the other party's Cells — they are already in the conversation.
pub async fn version_vector_for_root(
    pool: &SqlitePool,
    root: &str,
) -> Result<Vec<VectorEntry>, StoreError> {
    Ok(sqlx::query(
        "SELECT actor_cell, MAX(hlc) AS max_hlc FROM sync_op
          WHERE replica_root = ?
          GROUP BY actor_cell ORDER BY actor_cell",
    )
    .bind(root)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| VectorEntry {
        actor_cell: row.get("actor_cell"),
        max_hlc: row.get("max_hlc"),
    })
    .collect())
}

/// The highest seq of ours that the holder of `theirs` has DEFINITELY
/// received — the retention floor for that contact, derived from a version
/// vector instead of from a number they reported.
///
/// "Definitely" is the whole job, so it is the last seq before the first op
/// they lack, not the count of ops they hold. An op they are missing in the
/// middle of our log means everything after it is unconfirmed too, however
/// much of the tail they happen to have: pruning past a gap would delete ops
/// the peer never saw and can no longer ask for.
pub async fn seq_covered_by_vector(
    pool: &SqlitePool,
    organ_uid: &str,
    theirs: &[VectorEntry],
) -> Result<i64, StoreError> {
    // The FIRST op they lack. One indexed lookup rather than a walk.
    let sql = format!(
        "SELECT MIN(seq) FROM sync_op
          WHERE organ_uid = ? AND replica_root IS NULL{}",
        uncovered_clause(theirs)
    );
    let mut query = sqlx::query_scalar::<_, Option<i64>>(&sql).bind(organ_uid);
    for entry in theirs {
        query = query.bind(&entry.actor_cell).bind(entry.max_hlc);
    }
    match query.fetch_one(pool).await? {
        // Everything below the first gap is confirmed; the gap itself is not.
        Some(first_missing) => Ok(first_missing - 1),
        // Nothing is missing: they hold the whole feed.
        None => Ok(sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(seq) FROM sync_op WHERE organ_uid = ? AND replica_root IS NULL",
        )
        .bind(organ_uid)
        .fetch_one(pool)
        .await?
        .unwrap_or(0)),
    }
}

/// The catch-up feed for ONE grant root. The caller must have already checked
/// that the requesting contact holds an accepted grant on `root` — this
/// function selects, it does not authorize.
pub async fn after_in_root(
    pool: &SqlitePool,
    root: &str,
    seq: i64,
    limit: i64,
) -> Result<Vec<OpRow>, StoreError> {
    Ok(
        sqlx::query(
            "SELECT * FROM sync_op WHERE seq > ? AND replica_root = ? ORDER BY seq LIMIT ?",
        )
        .bind(seq)
        .bind(root)
        .bind(limit)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map)
        .collect(),
    )
}

/// Ops targeting one (table, uid, field) — merge/inspection helper.
pub async fn for_field(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
    field: &str,
) -> Result<Vec<OpRow>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM sync_op WHERE tbl = ? AND uid = ? AND field = ? ORDER BY seq")
            .bind(tbl)
            .bind(uid)
            .bind(field)
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(map)
            .collect(),
    )
}

/// The stored HLC of a field's current value: the max stamp among its ops.
/// This is the LWW compare on import — no field-HLC sidecar exists anywhere.
pub async fn latest_hlc_for_field(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
    field: &str,
) -> Result<Option<i64>, StoreError> {
    Ok(
        sqlx::query("SELECT MAX(hlc) AS hlc FROM sync_op WHERE tbl = ? AND uid = ? AND field = ?")
            .bind(tbl)
            .bind(uid)
            .bind(field)
            .fetch_one(pool)
            .await?
            .get::<Option<i64>, _>("hlc"),
    )
}

/// Max HLC in the whole log — boot seeds the Cell clock past it.
pub async fn max_hlc(pool: &SqlitePool) -> Result<Option<i64>, StoreError> {
    Ok(sqlx::query("SELECT MAX(hlc) AS hlc FROM sync_op")
        .fetch_one(pool)
        .await?
        .get::<Option<i64>, _>("hlc"))
}

/// Latest assigned seq (0 for an empty log) — the serve-side cursor head.
pub async fn max_seq(pool: &SqlitePool) -> Result<i64, StoreError> {
    Ok(
        sqlx::query("SELECT COALESCE(MAX(seq), 0) AS seq FROM sync_op")
            .fetch_one(pool)
            .await?
            .get("seq"),
    )
}

/// What `prune` did, or would do. Returned rather than logged so a caller can
/// show it before committing to anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PruneReport {
    /// The highest seq every synced contact has confirmed receiving.
    pub floor: i64,
    /// Ops deleted (or deletable, for a dry run).
    pub removed: i64,
    /// Ops at or below the floor that were KEPT because something still needs
    /// them — a queued outbox row, or a `crdt` op not yet folded into a
    /// snapshot.
    pub retained: i64,
}

/// The retention floor: the lowest `peer_acked_seq` across contacts we still
/// owe ops to. `None` means there is no floor and nothing may be pruned.
///
/// Blocked contacts are excluded — we will never send to them again, so a
/// blocked peer stuck at seq 0 must not freeze retention forever. Contacts
/// that receive nothing from us (`sync_out = 0` and no accepted grant) are
/// excluded for the same reason.
pub async fn retention_floor(pool: &SqlitePool) -> Result<Option<i64>, StoreError> {
    let row = sqlx::query(
        "SELECT MIN(c.peer_acked_seq) AS floor, COUNT(*) AS n
           FROM organ_contact c
          WHERE c.trust != 'blocked'
            AND (c.sync_out = 1
                 OR EXISTS (SELECT 1 FROM replica_grant g
                             WHERE g.contact_organ = c.record_uid))",
    )
    .fetch_one(pool)
    .await?;
    let contacts: i64 = row.get("n");
    if contacts == 0 {
        // No one to relay to. Pruning would be safe for THEM and unsafe for
        // anyone added later: there is no bootstrap-from-snapshot path yet, so
        // a new contact can only be brought up to date by replaying the log.
        // Keep everything and let the caller decide.
        return Ok(None);
    }
    Ok(Some(row.get::<i64, _>("floor")))
}

/// Drop ops every synced contact has already received.
///
/// Safe to run unattended, and `sync_once` does. It was manual while a
/// contact falling behind the pruned floor had no recovery path; superseded-
/// only retention plus snapshots in the log removed that risk, so the dry-run
/// mode stays (computed from the same predicate as the delete, so the two can
/// never disagree) but the human gate is gone.
///
/// **Only SUPERSEDED ops are prunable**, and that one rule is what makes a
/// replica bootstrap possible with no bootstrap protocol at all. An op is
/// superseded when a NEWER op exists for the same `(tbl, uid, field)` on the
/// same channel — so the log always retains, for every live field, the op that
/// established its current value. The surviving log is therefore current state
/// plus recent history, and a contact added long after a prune builds a
/// complete replica by replaying from zero like any other.
///
/// It also keeps the LWW memory intact. Import decides "is this op older than
/// what I hold?" by reading the highest HLC for that field OUT OF THIS LOG
/// (`latest_hlc_for_field`). Pruning a field's newest op would erase that
/// memory, and a later-arriving stale value would then look new and overwrite
/// current data. Keeping the tip keeps the comparison honest.
///
/// Growth is still bounded, because history is what accumulates: a field
/// rewritten ten thousand times keeps one op. The log becomes O(live state)
/// rather than O(edit history), which was the actual goal.
///
/// Two further exceptions, both load-bearing:
///
/// 1. **An op still referenced by `sync_outbox`.** The outbox holds at most one
///    row per (contact, tbl, uid, field) and points into the log by `seq`, so a
///    queued row for an offline contact can easily sit below another contact's
///    checkpoint. Deleting it turns a pending delivery into a silent no-op.
/// 2. **A `crdt` op is pruned only once a later `snapshot` folds it in.** Each
///    tail is cumulative since the writing Cell's last snapshot, so dropping
///    one with no snapshot above it would lose the text written before it with
///    no way to recover. This was the "never pruned" rule until `snapshot`
///    became an op kind: the identity it needs is not synthesized after all —
///    the compacting Cell signs it with its own `(actor_cell, hlc)` like any
///    other write, which is what made the whole thing affordable.
pub async fn prune(pool: &SqlitePool, dry_run: bool) -> Result<PruneReport, StoreError> {
    let Some(floor) = retention_floor(pool).await? else {
        return Ok(PruneReport {
            floor: 0,
            removed: 0,
            retained: 0,
        });
    };

    // One predicate, used for both the count and the delete, so a dry run can
    // never disagree with what the delete would do.
    //
    // `n.field = sync_op.field` matches exactly, empty string included: record
    // tombstones, assertions and crdt ops all use `field = ''`, so treating it
    // as a wildcard would let a tombstone and a crdt op on one uid look like
    // the same target and silently supersede each other.
    //
    // `IS` rather than `=` on `replica_root` because it is NULL for the general
    // feed and `=` never matches NULL. Comparing within one channel matters:
    // the general feed and a grant channel are served separately, so an op must
    // only be considered superseded by something a peer would receive ALONGSIDE
    // it.
    //
    // Supersede is KIND-AWARE, and that is not tidiness. `tombstone`, `crdt`
    // and `snapshot` all use `field = ''`, so a kind-blind rule treats them as
    // the same target: a snapshot would supersede a record's tombstone, the
    // delete would be pruned, and a peer replaying from zero would never learn
    // the record was deleted. It would come back, for that peer only.
    //
    // One deliberate CROSS-kind rule, and it is decision 2's whole payload: a
    // `crdt` tail is superseded by a later `snapshot` of the same record.
    // Safe because our snapshot is exported from a doc that has already
    // imported every op we hold — so everything below it is inside it. Not by
    // another `crdt` op, even though tails are cumulative: they are cumulative
    // since the writing CELL's own baseline, so one Cell's tail does not
    // contain another's.
    // SUPERSEDE-ONLY IS LOAD-BEARING FOR SIBLINGS (Ontology §11, decided
    // 2026-08-11). The retention floor is keyed by CONTACT, so another Cell of
    // this same Organ holds nothing down — an offline second device is not
    // considered here at all. That is safe only because everything removed
    // below is already superseded, so what a returning sibling loses is
    // intermediate values LWW would discard anyway, never current state.
    //
    // Widen this predicate — any rule that can remove an op still carrying
    // live state — and that reasoning is gone: siblings then have to be
    // tracked in the floor first. Whoever changes this owns that.
    const PRUNABLE: &str = "seq <= ?
           AND seq NOT IN (SELECT seq FROM sync_outbox)
           AND (EXISTS (SELECT 1 FROM sync_op n
                         WHERE n.tbl = sync_op.tbl
                           AND n.uid = sync_op.uid
                           AND n.field = sync_op.field
                           AND n.kind = sync_op.kind
                           AND n.replica_root IS sync_op.replica_root
                           AND n.seq > sync_op.seq)
                OR (sync_op.kind = 'crdt'
                    AND EXISTS (SELECT 1 FROM sync_op s
                                 WHERE s.kind = 'snapshot'
                                   AND s.tbl = sync_op.tbl
                                   AND s.uid = sync_op.uid
                                   AND s.replica_root IS sync_op.replica_root
                                   AND s.seq > sync_op.seq)))";

    let removable: i64 =
        sqlx::query(&format!("SELECT COUNT(*) AS n FROM sync_op WHERE {PRUNABLE}"))
            .bind(floor)
            .fetch_one(pool)
            .await?
            .get("n");
    let at_or_below: i64 = sqlx::query("SELECT COUNT(*) AS n FROM sync_op WHERE seq <= ?")
        .bind(floor)
        .fetch_one(pool)
        .await?
        .get("n");

    if !dry_run && removable > 0 {
        sqlx::query(&format!("DELETE FROM sync_op WHERE {PRUNABLE}"))
            .bind(floor)
            .execute(pool)
            .await?;
    }
    Ok(PruneReport {
        floor,
        removed: removable,
        retained: at_or_below - removable,
    })
}

/// Every op, oldest STAMP first — the rebuild feed.
///
/// Ordered by `hlc`, not by `seq`, and that is the whole point: seq is local
/// arrival order, while hlc is the order the writers meant. Replaying in hlc
/// order makes last-write-wins fall out of the sequence itself, so a rebuild
/// needs none of import's comparison machinery.
///
/// Unlike `after`, this crosses channels: an individual replica's ops are part
/// of this Cell's own state and must be rebuilt too. Nothing here is served to
/// a peer — the caller is repairing itself.
///
/// Reads the whole log. A rebuild is a maintenance operation on a log already
/// bounded to O(live state) by pruning, so paging it would add failure modes
/// (a partial rebuild is worse than none) to save memory that is not at risk.
pub async fn all_by_hlc(pool: &SqlitePool) -> Result<Vec<OpRow>, StoreError> {
    Ok(sqlx::query("SELECT * FROM sync_op ORDER BY hlc, seq")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map)
        .collect())
}

/// The winning `set` op for every record field — the log's opinion of current
/// state, which is what an audit compares the read model against.
///
/// Ties broken by `seq`, and the tie is NOT a freak case: an HLC is unique per
/// CELL, so two Cells of one Organ minting the same stamp is ordinary (that is
/// the whole reason `actor_cell` is the identity column). Without the
/// tiebreak both rows survive the `NOT EXISTS` and the audit reports one as
/// diverged against the other, arbitrarily and differently each run.
pub async fn record_field_tips(pool: &SqlitePool) -> Result<Vec<OpRow>, StoreError> {
    Ok(sqlx::query(
        "SELECT * FROM sync_op a
          WHERE a.tbl = 'record' AND a.kind = 'set'
            AND NOT EXISTS (SELECT 1 FROM sync_op b
                             WHERE b.tbl = 'record' AND b.kind = 'set'
                               AND b.uid = a.uid AND b.field = a.field
                               AND (b.hlc > a.hlc
                                    OR (b.hlc = a.hlc AND b.seq > a.seq)))",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map)
    .collect())
}

/// One op by its local seq — outbox hydration (a pruned seq returns None).
pub async fn get_by_seq(pool: &SqlitePool, seq: i64) -> Result<Option<OpRow>, StoreError> {
    Ok(sqlx::query("SELECT * FROM sync_op WHERE seq = ?")
        .bind(seq)
        .fetch_optional(pool)
        .await?
        .map(map))
}

/// The latest `set` stamp on any field of a row — what a record tombstone must
/// beat: "undelete is a newer write" means alive iff latest set > latest
/// tombstone.
pub async fn latest_set_hlc_for_row(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
) -> Result<Option<i64>, StoreError> {
    Ok(sqlx::query(
        "SELECT MAX(hlc) AS hlc FROM sync_op WHERE tbl = ? AND uid = ? AND kind = 'set'",
    )
    .bind(tbl)
    .bind(uid)
    .fetch_one(pool)
    .await?
    .get::<Option<i64>, _>("hlc"))
}

// ------------------------------------------------------- bounded outbox

#[derive(Debug, Clone)]
pub struct OutboxRow {
    pub contact_organ: String,
    pub tbl: String,
    pub uid: String,
    pub field: String,
    /// Part of the key: a tombstone and a crdt op share `field = ''` and would
    /// otherwise replace each other in the queue.
    pub kind: String,
    pub seq: i64,
    pub attempts: i64,
}

/// Every queued op, grouped by contact (stable order), oldest op first.
pub async fn outbox_due(pool: &SqlitePool) -> Result<Vec<OutboxRow>, StoreError> {
    Ok(
        sqlx::query("SELECT * FROM sync_outbox ORDER BY contact_organ, seq")
            .fetch_all(pool)
            .await?
            .into_iter()
            .map(|r| OutboxRow {
                contact_organ: r.get("contact_organ"),
                tbl: r.get("tbl"),
                uid: r.get("uid"),
                field: r.get("field"),
                kind: r.get("kind"),
                seq: r.get("seq"),
                attempts: r.get("attempts"),
            })
            .collect(),
    )
}

/// Delete one delivered outbox row — guarded by seq, so an op that was
/// replaced by a newer one while in flight stays queued for the next drain.
pub async fn outbox_delete(pool: &SqlitePool, row: &OutboxRow) -> Result<(), StoreError> {
    sqlx::query(
        "DELETE FROM sync_outbox
          WHERE contact_organ = ? AND tbl = ? AND uid = ? AND field = ? AND kind = ?
            AND seq = ?",
    )
    .bind(&row.contact_organ)
    .bind(&row.tbl)
    .bind(&row.uid)
    .bind(&row.field)
    .bind(&row.kind)
    .bind(row.seq)
    .execute(pool)
    .await?;
    Ok(())
}

/// A failed send: bump attempts on everything queued for the contact; rows
/// stay queued and retry next drain.
pub async fn outbox_bump_attempts(
    pool: &SqlitePool,
    contact_organ: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE sync_outbox SET attempts = attempts + 1 WHERE contact_organ = ?")
        .bind(contact_organ)
        .execute(pool)
        .await?;
    Ok(())
}

/// Drop everything queued for a contact (it vanished or is blocked).
pub async fn outbox_clear_contact(
    pool: &SqlitePool,
    contact_organ: &str,
) -> Result<(), StoreError> {
    sqlx::query("DELETE FROM sync_outbox WHERE contact_organ = ?")
        .bind(contact_organ)
        .execute(pool)
        .await?;
    Ok(())
}

/// Queue a Record's EXISTING ops for ONE contact — the re-grant path
/// (Ontology §12, C5).
///
/// Replay by identity, not a synthesized snapshot. A contact who was never to
/// have this Record holds none of its ops, so its own history is exactly what
/// they are missing, and sending it under the ORIGINAL `(actor_cell, hlc)`
/// costs nothing that a minted op would:
///
/// * Nothing is invented, so no signature, roster or clock question arises.
/// * The original stamps sit BELOW the contact's version-vector high-water
///   mark, so replaying moves nothing — a minted op stamped above it would
///   have handed them coverage of every real op of ours they had not yet
///   received, stranding those permanently.
/// * Import is guarded per field (`op.hlc <= prior` in `import_ops`) and by
///   op identity, so a replayed op can neither overwrite something newer nor
///   apply a quantity twice. Both guards already existed; this path only
///   needed to stop assuming they did not.
///
/// The bounded outbox does the compaction for free: the conflict clause keeps
/// the LATEST op per `(tbl, uid, field, kind)`, so a Record's whole history
/// collapses to its current value per column, while facts — each with its own
/// uid — all survive, which is what a quantity needs to add up.
///
/// The scope and the hide list are NOT applied here. They are applied when the
/// outbox drains, which is the single point both delivery paths already share,
/// and duplicating them here would be the second copy that eventually drifts.
pub async fn enqueue_record_for_contact(
    pool: &SqlitePool,
    contact_organ: &str,
    record_uid: &str,
) -> Result<usize, StoreError> {
    // Every op ABOUT this Record, whatever table it names — the same span
    // `visibility::records_of_op` maps in the other direction. An op-level
    // filter that only knew `tbl = 'record'` would re-grant a Record with no
    // quantity and no links, which is most of what many Records are.
    //
    // General feed only: an op inside a replica root belongs to that root's
    // grant holders and reaches nobody through this table.
    let rows = sqlx::query(
        "SELECT tbl, uid, field, kind, seq FROM sync_op
          WHERE replica_root IS NULL
            AND (
              (tbl = 'record' AND uid = ?1)
              OR (tbl = 'fact' AND uid IN (SELECT uid FROM fact WHERE record_uid = ?1))
              OR (tbl = 'record_assertion' AND uid IN (
                    SELECT uid FROM record_assertion
                     WHERE subject_uid = ?1 OR object_uid = ?1))
            )
          -- Seq order, and it is load-bearing rather than tidy: the genesis
          -- `quantity` op is an OPENING value and every later change adds to
          -- it, so a replay that arrived out of order would still converge but
          -- a truncated one would not.
          ORDER BY seq",
    )
    .bind(record_uid)
    .fetch_all(pool)
    .await?;
    let now = chrono::Utc::now().to_rfc3339();
    let mut queued = 0usize;
    for row in &rows {
        let tbl: String = row.get("tbl");
        let uid: String = row.get("uid");
        let field: String = row.get("field");
        let kind: String = row.get("kind");
        let seq: i64 = row.get("seq");
        // Addressed to ONE contact, unlike `ENQUEUE`, which fans out to every
        // sync-out contact. A re-grant is about the one person it was granted
        // to; fanning it out would re-send a Record's history to everybody.
        sqlx::query(
            "INSERT INTO sync_outbox (contact_organ, tbl, uid, field, kind, seq, queued_at)
             SELECT ?, ?, ?, ?, ?, ?, ?
               FROM organ_contact
              WHERE record_uid = ? AND sync_out = 1 AND trust != 'blocked'
             ON CONFLICT(contact_organ, tbl, uid, field, kind)
             DO UPDATE SET seq = excluded.seq, queued_at = excluded.queued_at",
        )
        .bind(contact_organ)
        .bind(&tbl)
        .bind(&uid)
        .bind(&field)
        .bind(&kind)
        .bind(seq)
        .bind(&now)
        .bind(contact_organ)
        .execute(pool)
        .await?;
        queued += 1;
    }
    Ok(queued)
}

/// Queue exactly what a WIDENING newly permits — the repair path
/// (Ontology §12, C5).
///
/// The same replay as `enqueue_record_for_contact` over a different op set,
/// and it exists for the same reason: adding a column to a contact's scope
/// leaves every op for that column below their version vector, so catch-up
/// will never offer it again and the column stays permanently blank for them
/// while the panel reads as shared.
///
/// `before` and `after` are the scope on either side of the change. Only the
/// difference is queued, and the difference is computed by asking the SAME
/// predicate twice rather than by reasoning about column names — so what gets
/// repaired and what gets served can never disagree.
///
/// Only ever reached because somebody widened a scope by hand. It is not on
/// any automatic path.
pub async fn enqueue_widened_for_contact(
    pool: &SqlitePool,
    contact_organ: &str,
    before: Option<&[String]>,
    after: Option<&[String]>,
) -> Result<u64, StoreError> {
    // The DIFF, and it is exact rather than a heuristic: an op is queued iff
    // the new scope lets it through and the old one did not. Both sides ask
    // `op_in_scope`, so the repair can never disagree with the filter that
    // will run when the outbox drains — a hand-written "which columns are
    // new" rule would be the second copy of the predicate, and the second
    // copy is the one that eventually gets an edge wrong.
    //
    // This replaced queuing the WHOLE feed and filtering at drain time. That
    // was correct but paid O(current state) of queue churn and bandwidth for
    // a change that usually adds one column.
    let rows = sqlx::query(
        // Collapsed to the LATEST op per (tbl, uid, field, kind) in the query
        // rather than in the outbox: the bounded outbox would collapse it
        // anyway, and doing it here means the log is walked once and the rows
        // that come back are already the ones worth looking at.
        "SELECT o.tbl, o.uid, o.field, o.kind, MAX(o.seq) AS seq
           FROM sync_op o
          WHERE o.replica_root IS NULL
          GROUP BY o.tbl, o.uid, o.field, o.kind",
    )
    .fetch_all(pool)
    .await?;
    let now = chrono::Utc::now().to_rfc3339();
    let mut queued = 0u64;
    for row in &rows {
        let tbl: String = row.get("tbl");
        let uid: String = row.get("uid");
        let field: String = row.get("field");
        let kind: String = row.get("kind");
        let seq: i64 = row.get("seq");
        if !op_in_scope(&tbl, &kind, &field, after) {
            continue;
        }
        if op_in_scope(&tbl, &kind, &field, before) {
            continue; // they had it, or could have: not what a widening owes.
        }
        queued += sqlx::query(
            "INSERT INTO sync_outbox (contact_organ, tbl, uid, field, kind, seq, queued_at)
             SELECT ?, ?, ?, ?, ?, ?, ?
               FROM organ_contact
              WHERE record_uid = ? AND sync_out = 1 AND trust != 'blocked'
             ON CONFLICT(contact_organ, tbl, uid, field, kind)
             DO UPDATE SET seq = excluded.seq, queued_at = excluded.queued_at",
        )
        .bind(contact_organ)
        .bind(&tbl)
        .bind(&uid)
        .bind(&field)
        .bind(&kind)
        .bind(seq)
        .bind(&now)
        .bind(contact_organ)
        .execute(pool)
        .await?
        .rows_affected();
    }
    Ok(queued)
}

