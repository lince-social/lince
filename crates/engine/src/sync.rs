//! Sync (blueprint XV.2): facts replicate. A package is a visibility-filtered
//! bundle of records + their signed facts; import re-appends through the one
//! write path (idempotent by fact uid, deltas commute) preserving the origin
//! author and signature. Rows travel by uid; slugs are local suggestions.

use chrono::Utc;
use nucleus::{Cause, CauseKind, Fact, NewFact};
use serde::{Deserialize, Serialize};
use store::Store;

use crate::error::EngineError;
use crate::Engine;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordSeed {
    pub uid: String,
    pub slug: Option<String>,
    pub kind: String,
    pub head: String,
    pub concept_uid: Option<String>,
    pub unit_uid: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub from_organ: String,
    pub records: Vec<RecordSeed>,
    pub facts: Vec<Fact>,
}

impl Engine {
    /// Export the records visible to `subject` plus their facts (blueprint XV).
    /// The visibility gate is the same one Protein uses — one boundary.
    pub async fn export_package(
        &self,
        subject: &str,
        from_organ: &str,
    ) -> Result<Package, EngineError> {
        let visible = store::visibility::visible_targets(&self.store.pool, subject).await?;
        let mut records = Vec::new();
        let mut facts = Vec::new();
        for r in store::records::list_all(&self.store.pool).await? {
            if !visible.contains(&r.uid) {
                continue;
            }
            records.push(RecordSeed {
                uid: r.uid.clone(),
                slug: r.slug,
                kind: r.kind,
                head: r.head,
                concept_uid: r.concept_uid,
                unit_uid: r.unit_uid,
            });
            facts.extend(store::facts::for_record(&self.store.pool, &r.uid, 10_000).await?);
        }
        facts.sort_by(|a, b| a.at.cmp(&b.at));
        Ok(Package { from_organ: from_organ.to_string(), records, facts })
    }

    /// Import a package: ensure the records exist locally by uid, then re-append
    /// their facts (idempotent, authorship preserved). Returns facts newly
    /// applied. Bad-signature facts are skipped (quarantine, blueprint XI.1).
    pub async fn import_package(&self, package: &Package) -> Result<Vec<Fact>, EngineError> {
        for seed in &package.records {
            ensure_record(&self.store, seed).await?;
        }
        let mut applied = Vec::new();
        for fact in &package.facts {
            // verify authorship before trusting a foreign delta
            if fact.signature.is_some()
                && !crate::trust::verify_fact(&self.store, fact).await.unwrap_or(false)
            {
                continue; // unverifiable: quarantine by skipping
            }
            let imported = NewFact {
                uid: Some(fact.uid.clone()), // idempotent by uid
                record_uid: fact.record_uid.clone(),
                delta: fact.delta,
                at: Some(fact.at),
                actor_uid: fact.actor_uid.clone(), // origin author survives replication
                cause: Cause { kind: CauseKind::Sync, uid: Some(package.from_organ.clone()) },
                payload: fact.payload.clone(),
            };
            // Sync-imported facts keep the ORIGIN signature so downstream Cells
            // can still verify the original author; we re-seal for the local
            // chain but carry the origin signature forward.
            let mut news = imported;
            let signer = self.signer.lock().await.clone();
            let mut tx = self.store.pool.begin().await?;
            if store::facts::exists(&mut tx, news.uid.as_ref().unwrap()).await? {
                tx.rollback().await?;
                continue;
            }
            news.actor_uid.get_or_insert_with(|| {
                signer.as_ref().map(|s| s.actor_uid.clone()).unwrap_or_default()
            });
            let prev = store::facts::last_hash(&mut tx).await?;
            let mut sealed = nucleus::fact::seal(news, &prev, Utc::now());
            sealed.signature = fact.signature.clone(); // origin authorship
            store::facts::insert(&mut tx, &sealed).await?;
            store::records::bump_quantity(&mut tx, &sealed.record_uid, sealed.delta, &Utc::now().to_rfc3339()).await?;
            tx.commit().await?;
            applied.push(sealed);
        }
        Ok(applied)
    }
}

async fn ensure_record(store: &Store, seed: &RecordSeed) -> Result<(), EngineError> {
    if store::records::get(&store.pool, &seed.uid).await?.is_some() {
        return Ok(());
    }
    // Insert with the ORIGIN uid so cross-organ joins line up (blueprint XV.2).
    // A slug is a local suggestion, never identity — drop it on collision.
    let slug_taken = match &seed.slug {
        Some(slug) => store::records::resolve(&store.pool, slug).await?.is_some(),
        None => false,
    };
    let now = Utc::now().to_rfc3339();
    store::sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity, concept_uid, unit_uid, created_at, updated_at)
         VALUES (?, ?, ?, ?, '', 0, ?, ?, ?, ?)",
    )
    .bind(&seed.uid)
    .bind(if slug_taken { None } else { seed.slug.clone() })
    .bind(&seed.kind)
    .bind(&seed.head)
    .bind(&seed.concept_uid)
    .bind(&seed.unit_uid)
    .bind(&now)
    .bind(&now)
    .execute(&store.pool)
    .await?;
    Ok(())
}
