use sqlx::{Row, Sqlite, SqlitePool, Transaction};

use crate::StoreError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpKind {
    Set,
    Tombstone,
    Fact,
    Crdt,
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
    pub organ_uid: String,
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

const ENQUEUE_GRANT: &str =
    "INSERT INTO sync_outbox (contact_organ, tbl, uid, field, kind, seq, queued_at)
    SELECT g.contact_organ, ?, ?, ?, ?, ?, ?
      FROM replica_grant g
      JOIN organ_contact c ON c.record_uid = g.contact_organ
     WHERE g.root_record = ? AND g.state = 'accepted'
       AND c.trust != 'blocked' AND g.contact_organ != ?
    ON CONFLICT(contact_organ, tbl, uid, field, kind)
    DO UPDATE SET seq = excluded.seq, queued_at = excluded.queued_at";

const ENQUEUE: &str =
    "INSERT INTO sync_outbox (contact_organ, tbl, uid, field, kind, seq, queued_at)
    SELECT record_uid, ?, ?, ?, ?, ?, ?
      FROM organ_contact
     WHERE sync_out = 1 AND trust != 'blocked' AND record_uid != ?
    ON CONFLICT(contact_organ, tbl, uid, field, kind)
    DO UPDATE SET seq = excluded.seq, queued_at = excluded.queued_at";

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
    if from_contact.is_some() {
        return Ok(Some(seq));
    }
    if matches!(kind, OpKind::Snapshot) {
        return Ok(Some(seq));
    }
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

const LOCAL_STAMP_ATTEMPTS: usize = 8;

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
        if let Some(theirs) = max_hlc_for_actor(pool, actor_cell).await? {
            nucleus::hlc::adopt_own(theirs);
        }
    }
    Err(sqlx::Error::Protocol(format!(
        "could not mint a free op identity for cell {actor_cell} after \
         {LOCAL_STAMP_ATTEMPTS} attempts"
    )))
}

async fn max_hlc_for_actor(pool: &SqlitePool, actor_cell: &str) -> Result<Option<i64>, StoreError> {
    Ok(
        sqlx::query_scalar::<_, Option<i64>>("SELECT MAX(hlc) FROM sync_op WHERE actor_cell = ?")
            .bind(actor_cell)
            .fetch_one(pool)
            .await?,
    )
}

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
        if tbl == "record" && kind == OpKind::Set {
            crate::record_changes::note_local(pool, uid, field).await?;
        }
        let now = chrono::Utc::now().to_rfc3339();
        match &root {
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
    let root = crate::replica::root_for_op_tx(tx, tbl, uid).await?;
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
            .bind(root.as_deref())
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
        if let Some(theirs) = theirs {
            nucleus::hlc::adopt_own(theirs);
        }
    }
    let Some(seq) = seq else {
        return Err(sqlx::Error::Protocol(format!(
            "could not mint a free op identity for cell {actor_cell}"
        )));
    };
    if tbl == "record" && kind == OpKind::Set {
        crate::record_changes::note_local_tx(tx, uid, field).await?;
    }
    let now = chrono::Utc::now().to_rfc3339();
    match &root {
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
                .execute(&mut **tx)
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
                .execute(&mut **tx)
                .await?;
        }
    }
    Ok(())
}

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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct VectorEntry {
    pub actor_cell: String,
    pub max_hlc: i64,
}

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

fn uncovered_clause(theirs: &[VectorEntry]) -> String {
    if theirs.is_empty() {
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

pub const LINK_PREFIX: &str = "link:";
pub const LINK_ALL: &str = "link:*";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LinkScope {
    pub all: bool,
    pub predicates: std::collections::BTreeSet<String>,
}

impl LinkScope {
    pub fn wanted(&self) -> bool {
        self.all || !self.predicates.is_empty()
    }

    fn admits(&self, predicate_uid: Option<&str>) -> bool {
        if self.all {
            return true;
        }
        predicate_uid.is_some_and(|uid| self.predicates.contains(uid))
    }
}

pub fn link_names(scope: &[String]) -> Vec<String> {
    scope
        .iter()
        .filter_map(|entry| entry.strip_prefix(LINK_PREFIX))
        .filter(|name| *name != "*")
        .map(str::to_string)
        .collect()
}

pub fn names_links(scope: &[String]) -> bool {
    scope.iter().any(|entry| entry.starts_with(LINK_PREFIX))
}

pub async fn resolve_link_scope(
    pool: &SqlitePool,
    scope: Option<&[String]>,
) -> Result<LinkScope, StoreError> {
    let Some(scope) = scope else {
        return Ok(LinkScope {
            all: true,
            predicates: Default::default(),
        });
    };
    let mut resolved = LinkScope {
        all: scope.iter().any(|entry| entry == LINK_ALL),
        predicates: Default::default(),
    };
    for name in link_names(scope) {
        if let Some(uid) = crate::concepts::resolve(pool, &name).await? {
            resolved.predicates.insert(uid);
        }
    }
    Ok(resolved)
}

pub fn narrow_ops_to_scope(
    ops: Vec<OpRow>,
    scope: Option<&[String]>,
    links: &LinkScope,
) -> Vec<OpRow> {
    let Some(scope) = scope else {
        return ops;
    };
    ops.into_iter()
        .filter(|op| {
            op_in_scope_with_links(
                &op.tbl,
                &op.kind,
                &op.field,
                Some(scope),
                op.value.as_deref(),
                links,
            )
        })
        .collect()
}

pub fn op_in_scope(tbl: &str, kind: &str, field: &str, scope: Option<&[String]>) -> bool {
    let links = scope
        .map(|scope| LinkScope {
            all: names_links(scope),
            predicates: Default::default(),
        })
        .unwrap_or(LinkScope {
            all: true,
            predicates: Default::default(),
        });
    op_in_scope_with_links(tbl, kind, field, scope, None, &links)
}

pub fn op_in_scope_with_links(
    tbl: &str,
    kind: &str,
    field: &str,
    scope: Option<&[String]>,
    value: Option<&str>,
    links: &LinkScope,
) -> bool {
    let Some(scope) = scope else {
        return true;
    };
    let names = |wanted: &str| scope.iter().any(|s| s == wanted);
    match tbl {
        "record" => match kind {
            "tombstone" => true,
            "crdt" | "snapshot" => names("head") || names("body"),
            "set" => !field.is_empty() && names(field),
            _ => false,
        },
        "fact" => names("quantity"),
        "record_assertion" => match kind {
            "tombstone" => links.wanted(),
            "set" => links.admits(predicate_of(value).as_deref()),
            _ => false,
        },
        "record_extension" => !field.is_empty() && names(field),
        "concept" => links.wanted(),
        _ => false,
    }
}

fn predicate_of(value: Option<&str>) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(value?)
        .ok()?
        .get("predicate_uid")?
        .as_str()
        .map(str::to_string)
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

pub async fn seq_covered_by_vector(
    pool: &SqlitePool,
    organ_uid: &str,
    theirs: &[VectorEntry],
) -> Result<i64, StoreError> {
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
        Some(first_missing) => Ok(first_missing - 1),
        None => Ok(sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(seq) FROM sync_op WHERE organ_uid = ? AND replica_root IS NULL",
        )
        .bind(organ_uid)
        .fetch_one(pool)
        .await?
        .unwrap_or(0)),
    }
}

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

pub async fn latest_author_for_field(
    pool: &SqlitePool,
    tbl: &str,
    uid: &str,
    field: &str,
) -> Result<Option<String>, StoreError> {
    Ok(sqlx::query(
        "SELECT organ_uid FROM sync_op
          WHERE tbl = ? AND uid = ? AND field = ?
          ORDER BY hlc DESC
          LIMIT 1",
    )
    .bind(tbl)
    .bind(uid)
    .bind(field)
    .fetch_optional(pool)
    .await?
    .map(|r| r.get::<String, _>("organ_uid")))
}

pub async fn max_hlc(pool: &SqlitePool) -> Result<Option<i64>, StoreError> {
    Ok(sqlx::query("SELECT MAX(hlc) AS hlc FROM sync_op")
        .fetch_one(pool)
        .await?
        .get::<Option<i64>, _>("hlc"))
}

pub async fn max_seq(pool: &SqlitePool) -> Result<i64, StoreError> {
    Ok(
        sqlx::query("SELECT COALESCE(MAX(seq), 0) AS seq FROM sync_op")
            .fetch_one(pool)
            .await?
            .get("seq"),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PruneReport {
    pub floor: i64,
    pub removed: i64,
    pub retained: i64,
}

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
        return Ok(None);
    }
    Ok(Some(row.get::<i64, _>("floor")))
}

pub async fn prune(pool: &SqlitePool, dry_run: bool) -> Result<PruneReport, StoreError> {
    let Some(floor) = retention_floor(pool).await? else {
        return Ok(PruneReport {
            floor: 0,
            removed: 0,
            retained: 0,
        });
    };

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

    let removable: i64 = sqlx::query(&format!(
        "SELECT COUNT(*) AS n FROM sync_op WHERE {PRUNABLE}"
    ))
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

pub async fn all_by_hlc(pool: &SqlitePool) -> Result<Vec<OpRow>, StoreError> {
    Ok(sqlx::query("SELECT * FROM sync_op ORDER BY hlc, seq")
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(map)
        .collect())
}

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

pub async fn get_by_seq(pool: &SqlitePool, seq: i64) -> Result<Option<OpRow>, StoreError> {
    Ok(sqlx::query("SELECT * FROM sync_op WHERE seq = ?")
        .bind(seq)
        .fetch_optional(pool)
        .await?
        .map(map))
}

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

#[derive(Debug, Clone)]
pub struct OutboxRow {
    pub contact_organ: String,
    pub tbl: String,
    pub uid: String,
    pub field: String,
    pub kind: String,
    pub seq: i64,
    pub attempts: i64,
}

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

pub async fn enqueue_record_for_contact(
    pool: &SqlitePool,
    contact_organ: &str,
    record_uid: &str,
) -> Result<usize, StoreError> {
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

pub async fn enqueue_widened_for_contact(
    pool: &SqlitePool,
    contact_organ: &str,
    before: Option<&[String]>,
    after: Option<&[String]>,
) -> Result<u64, StoreError> {
    let rows = sqlx::query(
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
            continue;
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
