//! Rebuild and audit: making "the log is authoritative" checkable rather than
//! a slogan (Ontology §11, cluster C2b).
//!
//! Two operations that only make sense together. The audit says whether the
//! read model still agrees with the log; the rebuild makes it agree again.
//! Either alone is half a feature — a detector with no repair leaves you
//! knowing you are broken, and a repair with no detector never runs.
//!
//! **What the log can and cannot reconstruct.** Scalar fields, extension keys,
//! assertions, concepts and collaborative text all come back, because every
//! one of them entered the log as an op. Quantity does NOT, and must not: a
//! `fact` op carries no payload (the signed, hash-chained row lives in `fact`
//! and is hydrated at serve time), and re-folding a fact chain that is already
//! correct would corrupt the Ledger it is supposed to protect. Facts are their
//! own audit trail — that is what the chain IS — so a rebuild leaves them
//! alone and rebuilds everything around them.
//!
//! **Nothing is deleted first.** A rebuild replays; it does not truncate and
//! reconstruct. Superseded-only retention guarantees the log still holds, for
//! every live field, the op that established its current value, so replaying
//! is sufficient — and not truncating means a rebuild can never be the thing
//! that loses data it failed to reconstruct.

use crate::Engine;
use crate::error::EngineError;
use crate::sync::{Materialise, WireOp};
use store::sync_ops::{self, OpKind, OpRow};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RebuildReport {
    /// Ops replayed into the read model.
    pub replayed: usize,
    /// Ops skipped because the log is not their authority — `fact` ops.
    pub skipped: usize,
    /// Records whose collaborative text was re-materialized from its doc.
    pub docs_rebuilt: usize,
}

/// One record where the read model and the log disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergence {
    pub record_uid: String,
    pub field: String,
    /// What the log says the value should be.
    pub expected: String,
    /// What the read model actually holds.
    pub found: String,
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AuditReport {
    /// Field tips compared.
    pub checked: usize,
    pub diverged: Vec<Divergence>,
}

impl AuditReport {
    pub fn is_clean(&self) -> bool {
        self.diverged.is_empty()
    }
}

/// A log row read back as though it had arrived from a peer.
///
/// The rebuild deliberately reuses the import path's `materialise` rather than
/// growing a second copy of "what an op means". The copy that drifts would be
/// this one — the rebuild runs precisely when the read model is already
/// suspect, which is the worst moment to discover it disagrees with import
/// about how a field is applied.
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
    /// Replay the whole log into the read model, oldest STAMP first.
    ///
    /// Ordering by HLC is what makes this simple: last-write-wins is the
    /// replay order, so every op can be applied unconditionally and the final
    /// state is the same one import would have reached. `undelete` is always
    /// true for a `set` for the same reason — anything replayed later is, by
    /// construction, newer than the tombstone replayed before it.
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
            // `quantity` is the fact chain's, not the log's. Creation logs one
            // `set` carrying the record's opening quantity, and every change
            // after that is a signed fact folded into the column — so
            // replaying the creation op would silently reset a Ledger balance
            // to whatever it was on the day the record was made. Caught by
            // test, and it is exactly the failure mode this module warns about
            // in its own header: the log is authoritative for what entered it
            // as state, and quantity did not.
            if row.tbl == "record" && row.field == "quantity" {
                report.skipped += 1;
                continue;
            }
            let replica_root = row.replica_root.clone();
            let op = as_wire(row);
            // Text is rebuilt from the doc once, at the end, rather than
            // re-materialized on every blob: the doc is the authority for
            // head/body, and importing N tails just to write the columns N
            // times would be the same answer N-1 times over.
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
                // A `(table, kind)` pair the read model does not own. The
                // schema constrains `kind` alone, so the log CAN hold one.
                None => report.skipped += 1,
            }
        }
        for uid in docs {
            // A deleted record's doc stays frozen — rebuilding its text would
            // undo a delete the log still records.
            if store::sync_apply::record_deleted(&self.store.pool, &uid).await? != Some(false) {
                continue;
            }
            let (head, body) = self.doc_text(&uid).await?;
            store::sync_apply::set_record_text_raw(&self.store.pool, &uid, &head, &body).await?;
            report.docs_rebuilt += 1;
        }
        Ok(report)
    }

    /// Compare the read model against the log, without changing either.
    ///
    /// This is the failure it exists to catch: `import_ops` reads the stored
    /// stamp, appends, compares and materializes without a spanning
    /// transaction, and connections are served on separate tasks — so two
    /// concurrent imports can leave a LOWER-HLC value in the read model while
    /// the log correctly retains the higher one. Nothing notices, and it does
    /// not self-heal. `rebuild_read_model` is the repair.
    pub async fn audit_read_model(&self) -> Result<AuditReport, EngineError> {
        let mut report = AuditReport::default();
        for tip in sync_ops::record_field_tips(&self.store.pool).await? {
            // Text belongs to the record-doc once collab history exists, so a
            // create-era `set` on head/body is not what the row should say —
            // comparing against it would report divergence on every
            // collaboratively edited record.
            if (tip.field == "head" || tip.field == "body")
                && store::record_docs::has_crdt_history(&self.store.pool, &tip.uid).await?
            {
                continue;
            }
            let Some(row) = store::records::get(&self.store.pool, &tip.uid).await? else {
                continue; // deleted, or never materialized — not a field disagreement
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
            // A slug is a local suggestion, dropped on collision, so the read
            // model is ALLOWED to disagree with the log about it.
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

    /// Audit, and rebuild if it found anything. Returns what it found and
    /// whether a repair ran, so a caller can report "clean" honestly rather
    /// than rebuilding unconditionally and calling that health.
    pub async fn audit_and_repair(&self) -> Result<(AuditReport, Option<RebuildReport>), EngineError> {
        let audit = self.audit_read_model().await?;
        if audit.is_clean() {
            return Ok((audit, None));
        }
        let rebuild = self.rebuild_read_model().await?;
        Ok((audit, Some(rebuild)))
    }
}
