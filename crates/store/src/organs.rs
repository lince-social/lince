use chrono::Utc;
use nucleus::RecordKind;
use serde_json::{Value, json};
use sqlx::{Row, SqlitePool};

use crate::StoreError;

pub const LOCAL_ORGAN_SLUG: &str = "local-organ";
const LOCAL_ORGAN_EXTENSION: &str = "lince.organ";

#[derive(Debug, Clone, PartialEq)]
pub struct OrganRecord {
    pub uid: String,
    pub slug: Option<String>,
    pub head: String,
    pub body: String,
    pub base_url: String,
    pub local: bool,
}

pub async fn ensure_local(pool: &SqlitePool, base_url: &str) -> Result<OrganRecord, StoreError> {
    // An EMPTY base_url means "do not change the address". `Store::open` mints
    // the identity before anything knows which port this Cell will bind and
    // passes ""; treating that as "clear the address" would make every CLI
    // invocation wipe the address the running web Cell wrote.
    let mut base_url = normalize_base_url(base_url);
    if base_url.is_empty() {
        if let Some(existing) = local(pool).await? {
            base_url = existing.base_url;
        }
    }
    let now = Utc::now().to_rfc3339();
    let existing_uid = sqlx::query_scalar::<_, String>("SELECT uid FROM record WHERE slug = ?")
        .bind(LOCAL_ORGAN_SLUG)
        .fetch_optional(pool)
        .await?;
    let uid = match existing_uid {
        Some(uid) => {
            // The head is NOT rewritten here. "Local Lince" is a first-boot
            // default, and re-stamping it every start would silently undo any
            // name the user gave this Cell — leaving every Cell in the world
            // called the same thing, which is precisely what makes a contact
            // row indistinguishable from this Cell's own in a list.
            sqlx::query(
                "UPDATE record
                    SET kind = ?,
                        body = ?,
                        updated_at = ?
                  WHERE uid = ?",
            )
            .bind(RecordKind::Organ.as_str())
            .bind(&base_url)
            .bind(&now)
            .bind(&uid)
            .execute(pool)
            .await?;
            uid
        }
        None => {
            let uid = nucleus::new_uid("r");
            sqlx::query(
                "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                                     organ_uid, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, '1', 0, ?, ?, ?)",
            )
            .bind(&uid)
            .bind(LOCAL_ORGAN_SLUG)
            .bind(RecordKind::Organ.as_str())
            .bind("Local Lince")
            .bind(&base_url)
            // An Organ Record's origin is itself. The alternative — leaving it
            // unstamped because "there is no Organ yet" — is the circularity
            // that made the column nullable in the first place, and the answer
            // to it is that identity is self-asserting, not conferred.
            .bind(&uid)
            .bind(&now)
            .bind(&now)
            .execute(pool)
            .await?;
            uid
        }
    };

    // Before anything can log an op: `sync_ops::log_local` stamps the CELL as
    // the op's actor, and the `set_extension` below is a logged write.
    crate::cells::ensure_local(pool, &uid, "this cell").await?;

    let fds = json!({
        "baseUrl": base_url,
        "aliases": ["http://127.0.0.1", "http://localhost"],
        "local": true
    });
    crate::records::set_extension(pool, &uid, LOCAL_ORGAN_EXTENSION, &fds).await?;
    local(pool).await?.ok_or(sqlx::Error::RowNotFound)
}

/// Whether this Cell holds any Record beyond the two every Cell boots with.
///
/// The enrolment eligibility test, and it lives here so the CHECK and the SWAP
/// cannot drift: asking it only inside `adopt_identity` would mean the caller
/// discovers it is ineligible after the single-use token has already been
/// redeemed on the other side — a spent code, a roster naming a Cell that
/// never joined, and a user who has to go and generate another one.
pub async fn holds_own_records(pool: &SqlitePool) -> Result<bool, StoreError> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM record WHERE slug IS NULL OR slug NOT IN (?, ?)")
            .bind(LOCAL_ORGAN_SLUG)
            .bind(crate::cells::LOCAL_CELL_SLUG)
            .fetch_one(pool)
            .await?;
    Ok(count > 0)
}

/// Replace this Cell's freshly-minted Organ with one it is JOINING
/// (Ontology §11, enrolment).
///
/// A new device boots, creates an Organ of its own, and only then learns it is
/// meant to be a second Cell of an existing identity. This is that swap: the
/// bootstrap Organ Record is deleted and the joined one takes its slug, and
/// the Cell Record is repointed at it.
///
/// **Only on a Cell that holds no data of its own.** Merging two identities is
/// a different and much larger act — every Record would need re-stamping and
/// every op re-attributing — and doing it silently as a side effect of
/// scanning a code is exactly the wrong default. Refused with a readable
/// reason instead.
///
/// **The bootstrap ops are PURGED, not re-stamped.** First boot already writes
/// ops (the `lince.organ` extension is a logged write), stamped with an Organ
/// uid that is about to stop existing. Re-stamping them to the joined Organ
/// would publish this device's local settings — its `baseUrl`, its `local`
/// flag — onto the shared Organ Record and over the wire to every sibling
/// Cell. They have never been sent anywhere (a fresh Cell has no contacts), so
/// deleting them loses nothing and keeps `rebuild_read_model` able to replay a
/// log that is coherent with the identity it belongs to.
pub async fn adopt_identity(
    pool: &SqlitePool,
    joined_organ_uid: &str,
    base_url: &str,
) -> Result<OrganRecord, StoreError> {
    let Some(current) = local(pool).await? else {
        return Err(sqlx::Error::Protocol(
            "this Cell has no Organ to replace".into(),
        ));
    };
    if current.uid == joined_organ_uid {
        return Ok(current);
    }
    if holds_own_records(pool).await? {
        return Err(sqlx::Error::Protocol(
            "this Cell already holds Records of its own, so joining another \
                      Organ would have to merge two identities. Enrol a device that \
                      has not been used yet."
                .into(),
        ));
    }

    let mut tx = crate::write_tx(pool).await?;
    // Order matters: the extension references the Record.
    sqlx::query("DELETE FROM sync_op").execute(&mut *tx).await?;
    sqlx::query("DELETE FROM record_extension WHERE record_uid = ?")
        .bind(&current.uid)
        .execute(&mut *tx)
        .await?;
    // The Cell first, so the Organ Record it points at never briefly does not
    // exist — `record_origin_required_update` would abort on an empty origin.
    sqlx::query("UPDATE record SET organ_uid = ? WHERE slug = ?")
        .bind(joined_organ_uid)
        .bind(crate::cells::LOCAL_CELL_SLUG)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM record WHERE uid = ?")
        .bind(&current.uid)
        .execute(&mut *tx)
        .await?;
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                             organ_uid, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, '1', 0, ?, ?, ?)",
    )
    .bind(joined_organ_uid)
    .bind(LOCAL_ORGAN_SLUG)
    .bind(RecordKind::Organ.as_str())
    .bind(&current.head)
    .bind(base_url)
    // An Organ Record's origin is itself, joined or minted.
    .bind(joined_organ_uid)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    // RAW, logging no op. These are this DEVICE's settings — its base url, its
    // `local` flag — and they now sit on a Record that SYNCS. Writing them
    // through the logged path would ship one Cell's address to every sibling
    // and have the last writer win. That the local surface config lives on the
    // shared Organ Record at all is a wart; it belongs on the Cell Record
    // (which never syncs) and moving it is scoping work.
    let fds = json!({
        "baseUrl": base_url,
        "aliases": ["http://127.0.0.1", "http://localhost"],
        "local": true
    });
    sqlx::query(
        "INSERT INTO record_extension (record_uid, namespace, fds) VALUES (?, ?, ?)
         ON CONFLICT(record_uid, namespace)
         DO UPDATE SET fds = excluded.fds, version = version + 1",
    )
    .bind(joined_organ_uid)
    .bind(LOCAL_ORGAN_EXTENSION)
    .bind(fds.to_string())
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    local(pool).await?.ok_or(sqlx::Error::RowNotFound.into())
}

pub async fn local(pool: &SqlitePool) -> Result<Option<OrganRecord>, StoreError> {
    let row = sqlx::query(
        "
        SELECT r.uid, r.slug, r.head, r.body, e.fds
        FROM record r
        LEFT JOIN record_extension e
          ON e.record_uid = r.uid AND e.namespace = ?
        WHERE r.slug = ? AND r.kind = ?
        LIMIT 1
        ",
    )
    .bind(LOCAL_ORGAN_EXTENSION)
    .bind(LOCAL_ORGAN_SLUG)
    .bind(RecordKind::Organ.as_str())
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| {
        let fds = row
            .get::<Option<String>, _>("fds")
            .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            .unwrap_or_else(|| json!({}));
        let body = row.get::<String, _>("body");
        let base_url = fds
            .get("baseUrl")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| body.clone());
        let local = fds.get("local").and_then(Value::as_bool).unwrap_or(true);
        OrganRecord {
            uid: row.get("uid"),
            slug: row.get("slug"),
            head: row.get("head"),
            body,
            base_url,
            local,
        }
    }))
}

fn normalize_base_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_string()
}

// ------------------------------------------------- contacts (blueprint XV)

#[derive(Debug, Clone)]
pub struct Contact {
    pub record_uid: String,
    pub slug: Option<String>,
    pub head: String,
    pub base_url: String,
    pub trust: String, // unknown | known | blocked
    pub proximity: u32,
    pub sync_out: bool,
    pub sync_in: bool,
    /// Their op-log seq as we last acknowledged it (catch-up checkpoint).
    pub last_synced_seq: i64,
    /// How far this contact has RECEIVED our own log — the retention floor.
    /// The mirror image of `last_synced_seq`, and not interchangeable with it:
    /// pruning against the wrong one deletes ops the peer never saw.
    pub peer_acked_seq: i64,
    /// `replica` (local rows, deltas + reconciliation) or `live` (Protein WS
    /// against the remote, zero local rows).
    pub mode: String,
    /// Seconds between catch-up pulls; 0 disables the cycle (reactive deltas
    /// and reconnect catch-up still run).
    pub catchup_interval_secs: i64,
    /// A saved Protein query naming WHICH Records travel to this contact.
    /// `None` is the unnarrowed feed — the visibility gate alone decides.
    pub share_protein: Option<String>,
    /// How far the op log had been read when this contact's selection was last
    /// reconciled. A watermark rather than a rescan: without it, every pass
    /// would re-evaluate the selection against the whole log.
    pub share_seen_seq: Option<i64>,
    /// Which COLUMNS of the Records this contact can see actually travel to
    /// them (Ontology §12, C5). `None` is unnarrowed — everything the
    /// visibility gate already allows.
    ///
    /// Not the same as `Some(vec![])`, which is a real and different answer:
    /// nothing but the identifying columns. Conflating "not configured" with
    /// "configured to nothing" is how a migration silently stops someone's
    /// sync.
    pub scope_fields: Option<Vec<String>>,
    /// Bumped whenever the scope changes, so a WIDENING is detectable — the
    /// catch-up vector never goes back on its own.
    pub scope_version: i64,
    /// The stored text of a scope that would NOT parse, if there is one.
    ///
    /// The unparseable case is read as unnarrowed rather than as empty, which
    /// is a choice for legibility over strictness (a corrupt row silently
    /// stopping a contact's sync is the worse failure, and the visibility gate
    /// is still in front of it either way). But "read as unnarrowed" must not
    /// mean "look identical to unnarrowed": that is a WIDER setting than
    /// anyone asked for, showing as if somebody had asked for it. So the raw
    /// text survives to the surface, which says the setting is broken and
    /// offers to replace it.
    ///
    /// `None` in the ordinary case — including a scope that is genuinely
    /// absent. Only a value that failed to parse appears here.
    pub scope_unreadable: Option<String>,
    /// The same for the inbound half. Separate value, separate repair: the two
    /// directions are separate settings everywhere else and a shared flag
    /// would report one of them as broken because the other is.
    pub accept_unreadable: Option<String>,
    /// Which columns we ACCEPT from this contact. `None` is unnarrowed, which
    /// is what `sync_in` alone meant. The other half of the pairing: outbound
    /// is what they may see of us, this is what they may change about our
    /// copy of the world.
    pub accept_fields: Option<Vec<String>>,
    /// Bumped on every acceptance change. Nothing consumes it yet — inbound
    /// has no cursor to re-open, since we cannot ask a peer to re-send what
    /// we chose to drop. Kept so a WIDENING is at least visible locally, and
    /// so the two directions have the same shape.
    pub accept_version: i64,
    /// Added from a code, with no connection yet to learn their real uid. The
    /// row is held under a uid derived from the NodeId until an Introduction
    /// replaces it; until then they cannot sync, because every batch they push
    /// is attributed to a uid this Cell does not know them by.
    pub pending_introduction: bool,
    /// The contact's iroh NodeId — the ONLY routing input (Ontology §11
    /// "Transport: iroh"). Under iroh the address is the key, so this both
    /// locates and authenticates. `None` for contacts made before the iroh
    /// path, which must be re-paired.
    pub node_id: Option<String>,
    /// When this contact first failed to answer, and still has not (Ontology
    /// C4). `None` means "answered the last time we tried" — the fallback to
    /// mail reads this, never an attempt count, because the window is
    /// wall-clock and pass frequency is not.
    pub unreachable_since: Option<String>,
    /// When we last left mail for them, so a peer that stays down is sealed
    /// and deposited once per window rather than once per pass.
    pub mailed_at: Option<String>,
}

/// Register a remote organ contact: an organ record carrying the REMOTE
/// organ's own uid (identity replicates by uid, blueprint XV.2 — introduction
/// hands it over) + the contact sidecar. Idempotent by uid; a colliding slug
/// is dropped (slugs are local suggestions, never identity).
pub async fn add_contact(
    pool: &SqlitePool,
    uid: &str,
    slug: Option<&str>,
    head: &str,
    base_url: &str,
    proximity: u32,
) -> Result<String, StoreError> {
    let base_url = normalize_base_url(base_url);
    let now = Utc::now().to_rfc3339();
    if crate::records::get(pool, uid).await?.is_none() {
        let slug_taken = match slug {
            Some(slug) => crate::records::resolve(pool, slug).await?.is_some(),
            None => false,
        };
        sqlx::query(
            "INSERT INTO record (uid, slug, kind, head, body, quantity_mantissa, quantity_scale,
                                 organ_uid, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, '1', 0, ?, ?, ?)",
        )
        .bind(uid)
        .bind(if slug_taken { None } else { slug })
        .bind(RecordKind::Organ.as_str())
        .bind(head)
        .bind(&base_url)
        // A contact's Organ Record originates from that contact, not from us.
        // Stamping it with the local Organ would make every address book entry
        // look like something this Cell authored.
        .bind(uid)
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await?;
    }
    // `unknown`, never `known`. Knowing someone's address is not deciding to
    // trust them, and `known` is what opens the sync ALPN (Ontology §11): a
    // default of `known` would mean every path that records a contact quietly
    // opens that door. Callers that HAVE made the decision — pairing, adopting
    // a code — say so with `set_trust`.
    sqlx::query(
        "INSERT OR IGNORE INTO organ_contact (record_uid, trust, proximity)
         VALUES (?, 'unknown', ?)",
    )
    .bind(uid)
    .bind(proximity as i64)
    .execute(pool)
    .await?;
    Ok(uid.to_string())
}

pub async fn set_trust(pool: &SqlitePool, organ_uid: &str, trust: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET trust = ? WHERE record_uid = ?")
        .bind(trust)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_proximity(
    pool: &SqlitePool,
    organ_uid: &str,
    proximity: u32,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET proximity = ? WHERE record_uid = ?")
        .bind(proximity as i64)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_sync_policy(
    pool: &SqlitePool,
    organ_uid: &str,
    sync_out: bool,
    sync_in: bool,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET sync_out = ?, sync_in = ? WHERE record_uid = ?")
        .bind(sync_out as i64)
        .bind(sync_in as i64)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Narrow (or unnarrow) what travels to one contact.
///
/// `None` clears the narrowing. The version bump is not bookkeeping: adding a
/// column leaves every op for it below this contact's version vector, so
/// without a detectable change the field would stay blank for them forever and
/// the setting would look applied while doing nothing.
pub async fn set_contact_scope(
    pool: &SqlitePool,
    organ_uid: &str,
    fields: Option<&[String]>,
) -> Result<(), StoreError> {
    let encoded = match fields {
        Some(fields) => Some(
            serde_json::to_string(fields)
                .map_err(|error| sqlx::Error::Protocol(error.to_string()))?,
        ),
        None => None,
    };
    sqlx::query(
        "UPDATE organ_contact
            SET scope_fields = ?, scope_version = scope_version + 1
          WHERE record_uid = ?",
    )
    .bind(encoded)
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

/// Narrow (or unnarrow) what we ACCEPT from one contact.
///
/// Deliberately a separate column and a separate function from
/// `set_contact_scope`, not a direction parameter on one: the outbound scope
/// is a privacy control and this is an integrity one, and a surface that made
/// them look like one setting with two ends would invite keeping them equal,
/// which is exactly what they are not for.
pub async fn set_contact_accept_scope(
    pool: &SqlitePool,
    organ_uid: &str,
    fields: Option<&[String]>,
) -> Result<(), StoreError> {
    let encoded = match fields {
        Some(fields) => Some(
            serde_json::to_string(fields)
                .map_err(|error| sqlx::Error::Protocol(error.to_string()))?,
        ),
        None => None,
    };
    sqlx::query(
        "UPDATE organ_contact
            SET accept_fields = ?, accept_version = accept_version + 1
          WHERE record_uid = ?",
    )
    .bind(encoded)
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

/// A stored scope, or `None` for both "not set" and "will not parse".
fn parse_scope(raw: Option<String>) -> Option<Vec<String>> {
    raw.and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
}

/// The stored text ONLY when it failed to parse — the one case the two
/// answers above have to be told apart, and the reason they are computed by
/// two functions over the same column rather than by one that returns a
/// three-way enum: every caller of `scope_fields` wants the ordinary answer,
/// and making all of them unwrap a failure they cannot act on is how the
/// failure ends up ignored at each of them.
fn unreadable_scope(raw: Option<String>) -> Option<String> {
    let raw = raw?;
    match serde_json::from_str::<Vec<String>>(&raw) {
        Ok(_) => None,
        Err(_) => Some(raw),
    }
}

fn map_contact(r: sqlx::sqlite::SqliteRow) -> Contact {
    Contact {
        record_uid: r.get("record_uid"),
        // A stored scope that will not parse is read as UNNARROWED rather than
        // as empty. Failing closed would be the instinct, but here it would
        // mean a corrupt row silently stops a contact's sync with no error —
        // and the visibility gate is still in front of this either way.
        //
        // What it must NOT do is look the same as an ordinary unnarrowed
        // scope, which is why the raw text is carried out alongside: this is a
        // wider setting than anybody chose, and a surface has to be able to
        // say so. See `scope_unreadable`.
        scope_fields: parse_scope(r.get("scope_fields")),
        scope_unreadable: unreadable_scope(r.get("scope_fields")),
        scope_version: r.get("scope_version"),
        accept_fields: parse_scope(r.get("accept_fields")),
        accept_unreadable: unreadable_scope(r.get("accept_fields")),
        accept_version: r.get("accept_version"),
        slug: r.get("slug"),
        head: r.get("head"),
        base_url: r.get("body"),
        trust: r.get("trust"),
        proximity: r.get::<i64, _>("proximity") as u32,
        sync_out: r.get::<i64, _>("sync_out") != 0,
        sync_in: r.get::<i64, _>("sync_in") != 0,
        last_synced_seq: r.get("last_synced_seq"),
        peer_acked_seq: r.get("peer_acked_seq"),
        mode: r.get("mode"),
        catchup_interval_secs: r.get("catchup_interval_secs"),
        share_protein: r.get("share_protein"),
        share_seen_seq: r.get("share_seen_seq"),
        node_id: r.get("node_id"),
        pending_introduction: r.get::<i64, _>("pending_introduction") != 0,
        unreachable_since: r.get("unreachable_since"),
        mailed_at: r.get("mailed_at"),
    }
}

/// Note that a contact did not answer this pass. Idempotent: the FIRST
/// failure is the one that dates the window, so a peer down for an hour is
/// not perpetually one pass old.
pub async fn mark_unreachable(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE organ_contact SET unreachable_since = ?
          WHERE record_uid = ? AND unreachable_since IS NULL",
    )
    .bind(Utc::now().to_rfc3339())
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

/// Note that a contact answered. Clears the mail bookkeeping with it: once
/// they are reachable, both "since when" and "when we last mailed" are stale.
///
/// Called from ANY successful exchange with the peer, not only a successful
/// push — reachability is a property of the peer, and the catch-up pull dials
/// the same contacts a moment later.
pub async fn mark_reachable(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE organ_contact SET unreachable_since = NULL, mailed_at = NULL
          WHERE record_uid = ? AND (unreachable_since IS NOT NULL OR mailed_at IS NOT NULL)",
    )
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

/// Record that mail was left for a contact just now.
pub async fn mark_mailed(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET mailed_at = ? WHERE record_uid = ?")
        .bind(Utc::now().to_rfc3339())
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Forget that mail was left, so the next pass may leave more. Used when a
/// human asks for delivery now rather than at the end of the window.
pub async fn mark_mailed_clear(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET mailed_at = NULL WHERE record_uid = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Move a contact's unreachability back in time.
///
/// A test seam, and deliberately a narrow one: proving "not on the first
/// failed dial" needs a clock that has moved, and backdating one column is
/// cheaper and less invasive than threading an injectable clock through the
/// whole sync pass. It writes nothing a real pass does not also write.
pub async fn backdate_unreachable(
    pool: &SqlitePool,
    organ_uid: &str,
    when: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET unreachable_since = ? WHERE record_uid = ?")
        .bind(when)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Mark a contact as still owing an Introduction, or clear it once one has
/// happened. Set only by adding from a code, cleared only by a connection.
pub async fn set_pending_introduction(
    pool: &SqlitePool,
    organ_uid: &str,
    pending: bool,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET pending_introduction = ? WHERE record_uid = ?")
        .bind(if pending { 1_i64 } else { 0 })
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Contacts added by code that no connection has confirmed yet. Only rows with
/// a NodeId are returned — without one there is nothing to dial, so there is
/// nothing a sync pass could do about them.
pub async fn pending_introductions(pool: &SqlitePool) -> Result<Vec<Contact>, StoreError> {
    Ok(sqlx::query(
        "SELECT c.*, r.slug, r.head, r.body FROM organ_contact c
           JOIN record r ON r.uid = c.record_uid
          WHERE c.pending_introduction = 1 AND c.node_id IS NOT NULL",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_contact)
    .collect())
}

/// Drop a contact and the Record standing in for it.
///
/// Used to retire a placeholder once the real Organ has introduced itself, and
/// to forget someone deliberately. Safe precisely because `add_contact` writes
/// both rows with plain SQL rather than through the Record write path: nothing
/// was ever logged to the op log, so no peer was told about this uid and there
/// is no history to orphan.
///
/// The annotations go with it. Setting trust, proximity or feed direction
/// commits a Fact against this record, and `fact.record_uid` is a foreign key —
/// so leaving them behind does not preserve history, it just makes forgetting
/// fail with a constraint error. What is being deleted is this Cell's own notes
/// about a row that stands in for someone else's Organ; the Organ itself is
/// untouched, and nothing here was ever anyone else's to keep.
pub async fn forget_contact(pool: &SqlitePool, organ_uid: &str) -> Result<(), StoreError> {
    for child in [
        "DELETE FROM fact_concept WHERE fact_uid IN (SELECT uid FROM fact WHERE record_uid = ?)",
        "DELETE FROM fact_concept_event WHERE fact_uid IN (SELECT uid FROM fact WHERE record_uid = ?)",
        "DELETE FROM fact_action_intent WHERE fact_uid IN (SELECT uid FROM fact WHERE record_uid = ?)",
        "DELETE FROM fact_remote_command WHERE fact_uid IN (SELECT uid FROM fact WHERE record_uid = ?)",
        "DELETE FROM fact WHERE record_uid = ?",
        "DELETE FROM record_extension WHERE record_uid = ?",
        "DELETE FROM record_doc WHERE record_uid = ?",
    ] {
        sqlx::query(child).bind(organ_uid).execute(pool).await?;
    }
    sqlx::query("DELETE FROM identity_key WHERE actor_uid = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM sync_outbox WHERE contact_organ = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM organ_contact WHERE record_uid = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    sqlx::query("DELETE FROM record WHERE uid = ?")
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Bind a contact to the iroh NodeId that reaches them. Written by pairing
/// (QR, paste, or an introduction over an already-authenticated connection),
/// never inferred from an inbound connection: adopting the NodeId of whoever
/// dialed us is exactly how an impostor would claim a contact's row.
/// Rename a contact to what the LOCAL user calls them.
///
/// Plain SQL and no op, for the same reason `add_contact` writes its record
/// that way: this is our private label for someone else's Organ, and logging
/// it would push our name for them back to them and to every other contact.
pub async fn rename_contact(
    pool: &SqlitePool,
    organ_uid: &str,
    head: &str,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE record SET head = ?, updated_at = ? WHERE uid = ?")
        .bind(head)
        .bind(Utc::now().to_rfc3339())
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_node_id(
    pool: &SqlitePool,
    organ_uid: &str,
    node_id: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET node_id = ? WHERE record_uid = ?")
        .bind(node_id)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Resolve an inbound connection's authenticated `remote_id()` to a contact.
/// This is the accept path's whole authorization input: iroh proved possession
/// of the private half during the QUIC/TLS handshake, so a hit here means the
/// peer IS that contact — no challenge, no signature, no replay window.
pub async fn contact_by_node_id(
    pool: &SqlitePool,
    node_id: &str,
) -> Result<Option<Contact>, StoreError> {
    Ok(sqlx::query(
        "SELECT c.*, r.slug, r.head, r.body FROM organ_contact c
           JOIN record r ON r.uid = c.record_uid
          WHERE c.node_id = ?",
    )
    .bind(node_id)
    .fetch_optional(pool)
    .await?
    .map(map_contact))
}

/// Advance the catch-up checkpoint: the peer's op seq we have fully applied.
pub async fn set_last_synced_seq(
    pool: &SqlitePool,
    organ_uid: &str,
    seq: i64,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET last_synced_seq = ? WHERE record_uid = ?")
        .bind(seq)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Advance the retention floor: how far this contact has received OUR log.
///
/// Monotonic by `MAX`, never assignment. A peer may legitimately ask for an
/// older `after` (a rebuild, a restored backup, two Cells of one Organ at
/// different points), and letting that move the floor BACKWARDS would be
/// harmless for correctness but would silently un-prune nothing while making
/// the floor meaningless. Moving it backwards is never useful; moving it
/// forwards on evidence is the whole point.
pub async fn advance_peer_acked_seq(
    pool: &SqlitePool,
    organ_uid: &str,
    seq: i64,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE organ_contact SET peer_acked_seq = MAX(peer_acked_seq, ?) WHERE record_uid = ?",
    )
    .bind(seq)
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reach {
    Direct,
    Mailbox,
    Auto,
}

impl Reach {
    pub fn as_str(self) -> &'static str {
        match self {
            Reach::Direct => "direct",
            Reach::Mailbox => "mailbox",
            Reach::Auto => "auto",
        }
    }
}

impl Contact {
    pub fn reach(&self) -> Reach {
        match self.mode.as_str() {
            "direct" => Reach::Direct,
            "mailbox" => Reach::Mailbox,
            _ => Reach::Auto,
        }
    }
}

/// Save (or clear) the Protein query that names what travels to this contact.
///
/// Clearing the watermark with it is what makes a NEW selection evaluate
/// against the whole log rather than only against ops since the last pass.
pub async fn set_contact_share_protein(
    pool: &SqlitePool,
    organ_uid: &str,
    protein: Option<&str>,
) -> Result<(), StoreError> {
    sqlx::query(
        "UPDATE organ_contact SET share_protein = ?, share_seen_seq = NULL
          WHERE record_uid = ?",
    )
    .bind(protein)
    .bind(organ_uid)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn set_mode(pool: &SqlitePool, organ_uid: &str, mode: &str) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET mode = ? WHERE record_uid = ?")
        .bind(mode)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn set_catchup_interval(
    pool: &SqlitePool,
    organ_uid: &str,
    secs: i64,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE organ_contact SET catchup_interval_secs = ? WHERE record_uid = ?")
        .bind(secs)
        .bind(organ_uid)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn contact(pool: &SqlitePool, organ_uid: &str) -> Result<Option<Contact>, StoreError> {
    Ok(sqlx::query(
        "SELECT c.*, r.slug, r.head, r.body FROM organ_contact c
           JOIN record r ON r.uid = c.record_uid
          WHERE c.record_uid = ?",
    )
    .bind(organ_uid)
    .fetch_optional(pool)
    .await?
    .map(map_contact))
}

pub async fn contacts(pool: &SqlitePool) -> Result<Vec<Contact>, StoreError> {
    Ok(sqlx::query(
        "SELECT c.*, r.slug, r.head, r.body FROM organ_contact c
           JOIN record r ON r.uid = c.record_uid
          ORDER BY r.created_at",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(map_contact)
    .collect())
}

// The op-based bounded outbox lives in `crate::sync_ops` (queued by the op
// log itself; drained by `Engine::drain_outbox`).

/// Record a rejected import row (blueprint XI.1: reject the row, keep the
/// package, remember why).
/// How many rejected ops are kept per contact. A ring: the oldest is dropped
/// to make room, so a contact that keeps sending garbage keeps only its most
/// recent garbage.
pub const QUARANTINE_PER_CONTACT: i64 = 200;

/// Record a rejected op, bounded (Ontology §11 "Quarantine needs a
/// lifecycle").
///
/// The bound is the point. Every malformed or out-of-scope op lands here with
/// its payload verbatim, and nothing aged it out — so the REJECT path was
/// cheaper for a hostile contact than the valid one, and filling a disk cost
/// them nothing. Worse after C1 and C2, which added three new ways in (a
/// forged Organ, a Cell outside the roster, a stamp from the future) without
/// adding a way out.
///
/// Per contact rather than one global cap, so a peer flooding it cannot evict
/// the evidence of what a different peer did — which is exactly what someone
/// would do to hide a real attack behind noise.
pub async fn quarantine(
    pool: &SqlitePool,
    from_organ: &str,
    reason: &str,
    payload: &str,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO sync_quarantine (uid, from_organ, reason, payload, at)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(nucleus::new_uid("qr"))
    .bind(from_organ)
    .bind(reason)
    .bind(payload)
    .bind(Utc::now().to_rfc3339())
    .execute(pool)
    .await?;
    // Trim by rowid, not by `at`: two rejections inside the same millisecond
    // share a timestamp, and ordering by it would make which one survives
    // arbitrary.
    sqlx::query(
        "DELETE FROM sync_quarantine
          WHERE from_organ = ?
            AND rowid NOT IN (SELECT rowid FROM sync_quarantine
                               WHERE from_organ = ?
                               ORDER BY rowid DESC LIMIT ?)",
    )
    .bind(from_organ)
    .bind(from_organ)
    .bind(QUARANTINE_PER_CONTACT)
    .execute(pool)
    .await?;
    trim_quarantine_to_budget(pool, from_organ).await?;
    Ok(())
}

/// Trim one contact's ring to its share of the Cell's storage budget (C2c).
///
/// **Both bounds apply, and the tighter one wins.** The count above answers
/// "how much garbage from one peer is worth reading"; this answers "how much
/// disk may one peer cost me", and they are different questions — 200 rows of
/// four bytes is nothing, 200 rows of a megabyte is not. Neither subsumes the
/// other, so keeping both is not redundancy.
///
/// Per contact rather than per Cell, for the same reason the count is: a global
/// byte cap lets one peer flood the ring and evict the evidence of what a
/// different peer did, which is exactly what somebody would do to hide a real
/// attack behind noise. The quota each contact gets is the whole quarantine
/// share — contacts do not divide it between them, because dividing it would
/// mean adding a contact shrinks everybody's evidence.
async fn trim_quarantine_to_budget(pool: &SqlitePool, from_organ: &str) -> Result<(), StoreError> {
    let total = crate::budget::total(pool).await?;
    let Some(quota) = crate::budget::share(total, crate::budget::Area::Quarantine) else {
        return Ok(()); // unlimited
    };
    // Newest first, which is the order `evict_plan` keeps by — the evidence a
    // peer is misbehaving right now outranks last month's.
    let rows: Vec<(i64, i64)> = sqlx::query(
        "SELECT rowid AS id, LENGTH(payload) AS n FROM sync_quarantine
          WHERE from_organ = ? ORDER BY rowid DESC",
    )
    .bind(from_organ)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| (row.get("id"), row.get("n")))
    .collect();
    let evicted = crate::budget::evict_plan(&rows, quota);
    if evicted.is_empty() {
        return Ok(());
    }
    let mut tx = crate::write_tx(pool).await?;
    for id in evicted {
        sqlx::query("DELETE FROM sync_quarantine WHERE rowid = ?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// One contact's rejected ops, newest first — what a person reads when asking
/// why a peer is not syncing.
pub async fn quarantined_for(
    pool: &SqlitePool,
    from_organ: &str,
    limit: i64,
) -> Result<Vec<(String, String, String)>, StoreError> {
    Ok(sqlx::query(
        "SELECT reason, payload, at FROM sync_quarantine
          WHERE from_organ = ? ORDER BY rowid DESC LIMIT ?",
    )
    .bind(from_organ)
    .bind(limit)
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| (row.get("reason"), row.get("payload"), row.get("at")))
    .collect())
}

/// Rejected-op counts per contact, worst first.
///
/// A contact producing a steady stream of rejections is reporting either a bug
/// or an attack, and both deserve a human's attention — this is what makes
/// that visible instead of leaving a table nobody reads.
pub async fn quarantine_by_contact(pool: &SqlitePool) -> Result<Vec<(String, i64)>, StoreError> {
    Ok(sqlx::query(
        "SELECT from_organ, COUNT(1) AS n FROM sync_quarantine
          GROUP BY from_organ ORDER BY n DESC",
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|row| (row.get("from_organ"), row.get("n")))
    .collect())
}

/// Forget one contact's rejected ops — the "I have read this" action.
pub async fn clear_quarantine(pool: &SqlitePool, from_organ: &str) -> Result<u64, StoreError> {
    Ok(
        sqlx::query("DELETE FROM sync_quarantine WHERE from_organ = ?")
            .bind(from_organ)
            .execute(pool)
            .await?
            .rows_affected(),
    )
}

pub async fn quarantine_count(pool: &SqlitePool) -> Result<i64, StoreError> {
    Ok(sqlx::query("SELECT COUNT(1) AS n FROM sync_quarantine")
        .fetch_one(pool)
        .await?
        .get("n"))
}
