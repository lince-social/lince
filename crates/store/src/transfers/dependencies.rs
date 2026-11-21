use std::collections::HashSet;

use nucleus::{
    PromiseState,
    transfer::{
        TransferDependencyEdge, TransferDependencyNode, TransferDependencyScope,
        TransferDependencyUpstreamKind, TransferRevisionDependency,
    },
};
use sqlx::{Row, Sqlite, SqliteConnection, SqlitePool, Transaction, sqlite::SqliteRow};

use crate::StoreError;

/// An upstream outcome required by the Transfer or by one promise.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferDependencyInput {
    pub uid: Option<String>,
    pub scope: TransferDependencyScope,
    pub promise_uid: Option<String>,
    pub upstream_kind: TransferDependencyUpstreamKind,
    pub upstream_uid: String,
    pub required_state: String,
}

pub(super) fn validate_dependency_inputs(
    dependencies: &[TransferDependencyInput],
) -> Result<(), StoreError> {
    let supplied_uids: HashSet<&str> = dependencies
        .iter()
        .filter_map(|dependency| dependency.uid.as_deref())
        .collect();
    if supplied_uids.len()
        != dependencies
            .iter()
            .filter(|value| value.uid.is_some())
            .count()
    {
        return Err(sqlx::Error::Protocol(
            "transfer draft repeats a dependency uid".into(),
        ));
    }
    let mut terms = HashSet::new();
    for dependency in dependencies {
        let upstream_uid = dependency.upstream_uid.trim();
        if upstream_uid.is_empty() {
            return Err(sqlx::Error::Protocol(
                "transfer dependency requires an upstream uid".into(),
            ));
        }
        if dependency.scope == TransferDependencyScope::Promise
            && dependency.promise_uid.as_deref().is_none_or(str::is_empty)
        {
            return Err(sqlx::Error::Protocol(
                "promise-scoped dependency requires a promise uid".into(),
            ));
        }
        if dependency.scope == TransferDependencyScope::Transfer && dependency.promise_uid.is_some()
        {
            return Err(sqlx::Error::Protocol(
                "Transfer-scoped dependency cannot name a scoped promise".into(),
            ));
        }
        let required_state = normalized_dependency_state(&dependency.required_state)?;
        if dependency.upstream_kind == TransferDependencyUpstreamKind::Transfer
            && !matches!(required_state, "agreed" | "kept")
        {
            return Err(sqlx::Error::Protocol(
                "Transfer dependencies require an agreed or settled outcome".into(),
            ));
        }
        if !terms.insert((
            dependency.scope,
            dependency.promise_uid.as_deref(),
            dependency.upstream_kind,
            upstream_uid,
            required_state,
        )) {
            return Err(sqlx::Error::Protocol(
                "transfer draft repeats identical dependency terms".into(),
            ));
        }
    }
    Ok(())
}

fn normalized_dependency_state(value: &str) -> Result<&str, StoreError> {
    let value = value.trim();
    let value = if value.is_empty() || value == "settled" {
        "kept"
    } else {
        value
    };
    PromiseState::parse(value)
        .map(nucleus::PromiseState::as_str)
        .ok_or_else(|| sqlx::Error::Protocol(format!("unknown dependency state `{value}`")))
}

pub(super) async fn replace_dependencies(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
    revision: i64,
    dependencies: &[TransferDependencyInput],
) -> Result<(), StoreError> {
    validate_dependency_inputs(dependencies)?;
    for dependency in dependencies {
        if let Some(uid) = dependency.uid.as_deref() {
            let owner: Option<String> =
                sqlx::query_scalar("SELECT transfer_uid FROM transfer_dependency WHERE uid = ?")
                    .bind(uid)
                    .fetch_optional(&mut **tx)
                    .await?;
            if owner.as_deref().is_some_and(|owner| owner != transfer_uid) {
                return Err(sqlx::Error::Protocol(
                    "dependency uid belongs to another Transfer".into(),
                ));
            }
        }
        if let Some(promise_uid) = dependency.promise_uid.as_deref() {
            let scoped_here: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM promise
                 WHERE uid = ? AND transfer_uid = ? AND state != 'withdrawn')",
            )
            .bind(promise_uid)
            .bind(transfer_uid)
            .fetch_one(&mut **tx)
            .await?;
            if !scoped_here {
                return Err(sqlx::Error::Protocol(format!(
                    "dependency scoped promise `{promise_uid}` is not current on this Transfer"
                )));
            }
        }
        let upstream_uid = dependency.upstream_uid.trim();
        let upstream_exists: bool = match dependency.upstream_kind {
            TransferDependencyUpstreamKind::Transfer => {
                if upstream_uid == transfer_uid {
                    return Err(sqlx::Error::Protocol(
                        "Transfer cannot depend directly on itself".into(),
                    ));
                }
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer WHERE record_uid = ?)
                    OR EXISTS(SELECT 1 FROM transfer_remote_reference WHERE transfer_uid = ? AND state = 'active' AND projection IS NOT NULL)")
                    .bind(upstream_uid)
                    .bind(upstream_uid)
                    .fetch_one(&mut **tx)
                    .await?
            }
            TransferDependencyUpstreamKind::Promise => {
                if dependency.promise_uid.as_deref() == Some(upstream_uid) {
                    return Err(sqlx::Error::Protocol(
                        "promise cannot depend directly on itself".into(),
                    ));
                }
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM promise WHERE uid = ?)")
                    .bind(upstream_uid)
                    .fetch_one(&mut **tx)
                    .await?
            }
        };
        if !upstream_exists {
            return Err(sqlx::Error::Protocol(format!(
                "unknown {} dependency upstream `{upstream_uid}`",
                dependency.upstream_kind.as_str()
            )));
        }
    }

    validate_dependency_dag(tx, transfer_uid, dependencies).await?;

    sqlx::query("DELETE FROM transfer_dependency WHERE transfer_uid = ?")
        .bind(transfer_uid)
        .execute(&mut **tx)
        .await?;
    for dependency in dependencies {
        let (origin, reference) = if dependency.upstream_kind
            == TransferDependencyUpstreamKind::Transfer
        {
            crate::transfer_outcomes::source_on(tx, transfer_uid, dependency.upstream_uid.trim())
                .await?
        } else {
            (None, None)
        };
        sqlx::query(
            "INSERT INTO transfer_dependency
                (uid, transfer_uid, revision, scope, promise_uid, upstream_kind,
                 upstream_uid, required_state, upstream_origin_uid, upstream_reference_uid)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(
            dependency
                .uid
                .clone()
                .unwrap_or_else(|| nucleus::new_uid("td")),
        )
        .bind(transfer_uid)
        .bind(revision)
        .bind(dependency.scope.as_str())
        .bind(&dependency.promise_uid)
        .bind(dependency.upstream_kind.as_str())
        .bind(dependency.upstream_uid.trim())
        .bind(normalized_dependency_state(&dependency.required_state)?)
        .bind(origin)
        .bind(reference)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

pub(super) async fn validate_transfer_parent(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
    parent_uid: Option<&str>,
) -> Result<(), StoreError> {
    let Some(mut cursor) = parent_uid
        .map(str::trim)
        .filter(|uid| !uid.is_empty())
        .map(str::to_owned)
    else {
        return Ok(());
    };
    let mut seen = HashSet::from([transfer_uid.to_owned()]);
    loop {
        if !seen.insert(cursor.clone()) {
            return Err(sqlx::Error::Protocol(format!(
                "Transfer parent cycle reaches `{cursor}`"
            )));
        }
        let row: Option<Option<String>> =
            sqlx::query_scalar("SELECT parent_uid FROM transfer WHERE record_uid = ?")
                .bind(&cursor)
                .fetch_optional(&mut **tx)
                .await?;
        let Some(parent) = row else {
            return Err(sqlx::Error::Protocol(format!(
                "unknown Transfer parent `{cursor}`"
            )));
        };
        let Some(next) = parent else {
            return Ok(());
        };
        cursor = next;
    }
}

pub(crate) async fn validate_dependency_dag(
    tx: &mut Transaction<'_, Sqlite>,
    transfer_uid: &str,
    replacement: &[TransferDependencyInput],
) -> Result<(), StoreError> {
    let rows = sqlx::query(
        "SELECT transfer_uid, scope, promise_uid, upstream_kind, upstream_uid
         FROM transfer_dependency WHERE transfer_uid != ?",
    )
    .bind(transfer_uid)
    .fetch_all(&mut **tx)
    .await?;
    let mut edges = Vec::with_capacity(rows.len() + replacement.len());
    for row in rows {
        let scope: &str = row.get("scope");
        let upstream_kind: &str = row.get("upstream_kind");
        let downstream = if scope == "transfer" {
            TransferDependencyNode::Transfer(row.get("transfer_uid"))
        } else {
            TransferDependencyNode::Promise(row.get("promise_uid"))
        };
        let upstream_uid: String = row.get("upstream_uid");
        let upstream = if upstream_kind == "transfer" {
            TransferDependencyNode::Transfer(upstream_uid)
        } else {
            TransferDependencyNode::Promise(upstream_uid)
        };
        edges.push(TransferDependencyEdge {
            upstream,
            downstream,
        });
    }
    for dependency in replacement {
        let downstream = match dependency.scope {
            TransferDependencyScope::Transfer => {
                TransferDependencyNode::Transfer(transfer_uid.to_owned())
            }
            TransferDependencyScope::Promise => TransferDependencyNode::Promise(
                dependency.promise_uid.clone().ok_or_else(|| {
                    StoreError::Protocol("promise-scoped dependency requires a promise uid".into())
                })?,
            ),
        };
        let upstream = match dependency.upstream_kind {
            TransferDependencyUpstreamKind::Transfer => {
                TransferDependencyNode::Transfer(dependency.upstream_uid.trim().to_owned())
            }
            TransferDependencyUpstreamKind::Promise => {
                TransferDependencyNode::Promise(dependency.upstream_uid.trim().to_owned())
            }
        };
        edges.push(TransferDependencyEdge {
            upstream,
            downstream,
        });
    }
    crate::transfer_agreement::validate_graph_on(tx, &mut edges).await?;
    Ok(())
}

fn map_dependency_row(row: SqliteRow) -> Result<TransferRevisionDependency, StoreError> {
    let scope: &str = row.get("scope");
    let upstream_kind: &str = row.get("upstream_kind");
    Ok(TransferRevisionDependency {
        uid: row.get("uid"),
        scope: TransferDependencyScope::parse(scope)
            .ok_or_else(|| sqlx::Error::Protocol(format!("unknown dependency scope `{scope}`")))?,
        promise_uid: row.get("promise_uid"),
        upstream_kind: TransferDependencyUpstreamKind::parse(upstream_kind).ok_or_else(|| {
            sqlx::Error::Protocol(format!(
                "unknown dependency upstream kind `{upstream_kind}`"
            ))
        })?,
        upstream_uid: row.get("upstream_uid"),
        origin_organ_uid: row.get("upstream_origin_uid"),
        required_state: row.get("required_state"),
    })
}

/// Reads a Transfer's dependencies in UID order.
///
/// # Errors
///
/// Returns an error if a dependency is invalid or storage fails.
pub async fn dependencies_of(
    pool: &SqlitePool,
    transfer_uid: &str,
) -> Result<Vec<TransferRevisionDependency>, StoreError> {
    let mut connection = pool.acquire().await?;
    dependencies_of_on(&mut connection, transfer_uid).await
}

pub(crate) async fn dependencies_of_on(
    connection: &mut SqliteConnection,
    transfer_uid: &str,
) -> Result<Vec<TransferRevisionDependency>, StoreError> {
    sqlx::query(
        "SELECT uid, scope, promise_uid, upstream_kind, upstream_uid, required_state, upstream_origin_uid
         FROM transfer_dependency WHERE transfer_uid = ? ORDER BY uid",
    )
    .bind(transfer_uid)
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .map(map_dependency_row)
    .collect()
}
