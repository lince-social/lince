use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::Engine;
use crate::error::EngineError;
use crate::sync::{Materialise, WireOp};
use store::sync_ops::{self, OpKind, OpRow};

pub const READ_MODEL_AUDIT: &str = "lince.cell.read_model_audit";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadModelHealth {
    pub at: String,
    pub checked: usize,
    pub diverged: usize,
    pub repaired: bool,
    pub replayed: usize,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RebuildReport {
    pub replayed: usize,
    pub skipped: usize,
    pub docs_rebuilt: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    pub record_uid: String,
    pub field: String,
    pub expected: String,
    pub found: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AuditReport {
    pub checked: usize,
    pub diverged: Vec<Divergence>,
}

impl AuditReport {
    pub fn is_clean(&self) -> bool {
        self.diverged.is_empty()
    }
}

fn as_wire(row: OpRow) -> WireOp {
    WireOp {
        tbl: row.tbl,
        uid: row.uid,
        field: row.field,
        kind: row.kind,
        value: row.value,
        hlc: row.hlc,
        actor_cell: row.actor_cell,
        organ_uid: row.organ_uid,
        fact: None,
    }
}

impl Engine {
    pub async fn rebuild_read_model(&self) -> Result<RebuildReport, EngineError> {
        let mut report = RebuildReport::default();
        let mut docs: Vec<String> = Vec::new();
        for row in sync_ops::all_by_hlc(&self.store.pool).await? {
            let Some(kind) = OpKind::parse(&row.kind) else {
                report.skipped += 1;
                continue;
            };
            if matches!(kind, OpKind::Fact) {
                report.skipped += 1;
                continue;
            }
            if row.tbl == "record" && row.field == "quantity" {
                report.skipped += 1;
                continue;
            }
            let replica_root = row.replica_root.clone();
            let op = as_wire(row);
            if matches!(kind, OpKind::Crdt | OpKind::Snapshot) {
                if !docs.contains(&op.uid) {
                    docs.push(op.uid.clone());
                }
                report.replayed += 1;
                continue;
            }
            match self
                .materialise(Materialise {
                    op: &op,
                    kind,
                    replica_root: replica_root.as_deref(),
                    undelete: true,
                })
                .await?
            {
                Some(outcome) => report.replayed += outcome.applied,
                None => report.skipped += 1,
            }
        }
        for uid in docs {
            if store::sync_apply::record_deleted(&self.store.pool, &uid).await? != Some(false) {
                continue;
            }
            let (head, body) = self.doc_text(&uid).await?;
            store::sync_apply::set_record_text_raw(&self.store.pool, &uid, &head, &body).await?;
            report.docs_rebuilt += 1;
        }
        Ok(report)
    }

    pub async fn audit_read_model(&self) -> Result<AuditReport, EngineError> {
        let mut report = AuditReport::default();
        for tip in sync_ops::record_field_tips(&self.store.pool).await? {
            if (tip.field == "head" || tip.field == "body")
                && store::record_docs::has_crdt_history(&self.store.pool, &tip.uid).await?
            {
                continue;
            }
            let Some(row) = store::records::get(&self.store.pool, &tip.uid).await? else {
                continue;
            };
            let found = match tip.field.as_str() {
                "head" => row.head.clone(),
                "body" => row.body.clone(),
                "kind" => row.kind.clone(),
                "slug" => row.slug.clone().unwrap_or_default(),
                _ => continue,
            };
            report.checked += 1;
            let expected = tip
                .value
                .as_deref()
                .and_then(|raw| serde_json::from_str::<serde_json::Value>(raw).ok())
                .and_then(|value| value.as_str().map(str::to_string))
                .unwrap_or_default();
            if tip.field == "slug" && found.is_empty() {
                continue;
            }
            if expected != found {
                report.diverged.push(Divergence {
                    record_uid: tip.uid.clone(),
                    field: tip.field.clone(),
                    expected,
                    found,
                });
            }
        }
        Ok(report)
    }

    pub async fn audit_and_repair(
        &self,
    ) -> Result<(AuditReport, Option<RebuildReport>), EngineError> {
        let audit = self.audit_read_model().await?;
        if audit.is_clean() {
            return Ok((audit, None));
        }
        let rebuild = self.rebuild_read_model().await?;
        Ok((audit, Some(rebuild)))
    }

    pub async fn read_model_health(&self) -> Result<Option<ReadModelHealth>, EngineError> {
        let Some(stored) = store::cells::config(&self.store.pool, READ_MODEL_AUDIT).await? else {
            return Ok(None);
        };
        Ok(serde_json::from_value(stored).ok())
    }

    pub async fn audit_read_model_if_due(
        &self,
        now: DateTime<Utc>,
        every: Duration,
    ) -> Result<Option<ReadModelHealth>, EngineError> {
        if let Some(last) = self.read_model_health().await? {
            if let Ok(at) = DateTime::parse_from_rfc3339(&last.at) {
                if now.signed_duration_since(at.with_timezone(&Utc)) < every {
                    return Ok(None);
                }
            }
        }
        let (audit, rebuild) = self.audit_and_repair().await?;
        let health = ReadModelHealth {
            at: now.to_rfc3339(),
            checked: audit.checked,
            diverged: audit.diverged.len(),
            repaired: rebuild.is_some(),
            replayed: rebuild.as_ref().map(|report| report.replayed).unwrap_or(0),
        };
        store::cells::set_config(
            &self.store.pool,
            READ_MODEL_AUDIT,
            &serde_json::to_value(&health)
                .map_err(|error| EngineError::Consequence(error.to_string()))?,
        )
        .await?;
        if health.diverged > 0 {
            tracing::warn!(
                diverged = health.diverged,
                replayed = health.replayed,
                "the read model had drifted from the log and was rebuilt"
            );
        }
        Ok(Some(health))
    }
}
