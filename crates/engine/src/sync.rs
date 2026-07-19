//! Sync (blueprint XV.2): facts replicate. A package is a visibility-filtered
//! bundle of records + their signed facts; import re-appends through the one
//! write path (idempotent by fact uid, deltas commute) preserving the origin
//! author and signature. Rows travel by uid; slugs are local suggestions.

use chrono::Utc;
use nucleus::{Cause, CauseKind, Fact, NewFact};
use serde::{Deserialize, Serialize};
use store::Store;

use crate::Engine;
use crate::error::EngineError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordSeed {
    pub uid: String,
    pub slug: Option<String>,
    pub kind: String,
    pub head: String,
    pub concept_uid: Option<String>,
    pub unit_uid: Option<String>,
    /// True origin organ, carried through relaying (blueprint: Protein-driven
    /// Sync). `#[serde(default)]` so packages from an older peer still import.
    #[serde(default)]
    pub organ_uid: Option<String>,
}

/// One concept riding a package (blueprint III.2: unknown concepts travel with
/// the data that speaks them; importing adopts uid + lineage).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptSeed {
    pub uid: String,
    pub name: String,
    #[serde(default)]
    pub parents: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub from_organ: String,
    #[serde(default)]
    pub concepts: Vec<ConceptSeed>,
    pub records: Vec<RecordSeed>,
    pub facts: Vec<Fact>,
}

impl Engine {
    /// Export the records visible to `subject` plus their facts (blueprint XV).
    /// The visibility gate is the same one Protein uses — one boundary. The
    /// concepts the records speak (and their ancestors) ride along.
    pub async fn export_package(
        &self,
        subject: &str,
        from_organ: &str,
    ) -> Result<Package, EngineError> {
        let visible = store::visibility::visible_targets(&self.store.pool, subject).await?;
        let mut records = Vec::new();
        let mut facts = Vec::new();
        let mut concept_uids: Vec<String> = Vec::new();
        for r in store::records::list_all(&self.store.pool).await? {
            if !visible.contains(&r.uid) {
                continue;
            }
            for c in [&r.concept_uid, &r.unit_uid].into_iter().flatten() {
                for ancestor in store::concepts::ancestors_including(&self.store.pool, c).await? {
                    if !concept_uids.contains(&ancestor) {
                        concept_uids.push(ancestor);
                    }
                }
            }
            // Lineage: keep the record's true origin if it already has one
            // (relaying through an intermediate organ), else this Cell is the
            // origin — stamp `from_organ` (blueprint: Sync/File Sync carry
            // origin so a downstream Protein `organ_eq` still resolves).
            let organ_uid = r.organ_uid.clone().or_else(|| Some(from_organ.to_string()));
            records.push(RecordSeed {
                uid: r.uid.clone(),
                slug: r.slug,
                kind: r.kind,
                head: r.head,
                concept_uid: r.concept_uid,
                unit_uid: r.unit_uid,
                organ_uid,
            });
            facts.extend(store::facts::for_record(&self.store.pool, &r.uid, 10_000).await?);
        }
        facts.sort_by(|a, b| a.at.cmp(&b.at));
        let all = store::concepts::list_all(&self.store.pool).await?;
        let concepts = concept_uids
            .into_iter()
            .filter_map(|uid| {
                all.iter().find(|c| c.uid == uid).map(|c| ConceptSeed {
                    uid: c.uid.clone(),
                    name: c.canonical_name.clone(),
                    parents: c.parents.clone(),
                })
            })
            .collect();
        Ok(Package {
            from_organ: from_organ.to_string(),
            concepts,
            records,
            facts,
        })
    }

    /// Export a Package selected by an arbitrary Protein instead of a
    /// visibility subject — the primitive File Sync (and any future
    /// Protein-scoped organ sync) shares with `export_package`. No visibility
    /// gating: the caller's Protein IS the selection rule (e.g. combine
    /// `organ_eq` with any other filter to pick exactly what leaves).
    pub async fn export_package_by_protein(
        &self,
        protein: &protein::Protein,
        from_organ: &str,
    ) -> Result<Package, EngineError> {
        let matched = protein::matching_records(&self.store, protein, None).await?;
        let mut records = Vec::new();
        let mut facts = Vec::new();
        let mut concept_uids: Vec<String> = Vec::new();
        for r in matched {
            for c in [&r.concept_uid, &r.unit_uid].into_iter().flatten() {
                for ancestor in store::concepts::ancestors_including(&self.store.pool, c).await? {
                    if !concept_uids.contains(&ancestor) {
                        concept_uids.push(ancestor);
                    }
                }
            }
            let organ_uid = r.organ_uid.clone().or_else(|| Some(from_organ.to_string()));
            records.push(RecordSeed {
                uid: r.uid.clone(),
                slug: r.slug,
                kind: r.kind,
                head: r.head,
                concept_uid: r.concept_uid,
                unit_uid: r.unit_uid,
                organ_uid,
            });
            facts.extend(store::facts::for_record(&self.store.pool, &r.uid, 10_000).await?);
        }
        facts.sort_by(|a, b| a.at.cmp(&b.at));
        let all = store::concepts::list_all(&self.store.pool).await?;
        let concepts = concept_uids
            .into_iter()
            .filter_map(|uid| {
                all.iter().find(|c| c.uid == uid).map(|c| ConceptSeed {
                    uid: c.uid.clone(),
                    name: c.canonical_name.clone(),
                    parents: c.parents.clone(),
                })
            })
            .collect();
        Ok(Package {
            from_organ: from_organ.to_string(),
            concepts,
            records,
            facts,
        })
    }

    /// File Sync (blueprint: Sync/CRDT — Protein-driven target selection):
    /// write the Protein-selected records + their facts to one JSON file at
    /// `path`, the disk-organ analogue of `enqueue_sync_to`. Not queued — the
    /// caller (a Karma signal/frequency, a CLI command) decides the cadence.
    pub async fn sync_to_disk(
        &self,
        path: &std::path::Path,
        protein: &protein::Protein,
    ) -> Result<usize, EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .map(|o| o.uid)
            .unwrap_or_default();
        let package = self.export_package_by_protein(protein, &organ).await?;
        let count = package.records.len();
        let json = serde_json::to_vec_pretty(&package).map_err(EngineError::Json)?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(EngineError::Io)?;
        }
        std::fs::write(path, json).map_err(EngineError::Io)?;
        Ok(count)
    }

    /// Import a package a File Sync wrote to disk (this Cell's own export, or
    /// another organ's, dropped at a shared path) — mirrors `import_package`.
    pub async fn sync_from_disk(&self, path: &std::path::Path) -> Result<Vec<Fact>, EngineError> {
        let raw = std::fs::read(path).map_err(EngineError::Io)?;
        let package: Package = serde_json::from_slice(&raw).map_err(EngineError::Json)?;
        self.import_package(&package).await
    }

    /// Import a package: ensure the records exist locally by uid, then re-append
    /// their facts (idempotent, authorship preserved). Returns facts newly
    /// applied. Rejected rows land in the quarantine list (blueprint XI.1);
    /// packages from blocked organs are rejected wholesale (XV).
    pub async fn import_package(&self, package: &Package) -> Result<Vec<Fact>, EngineError> {
        // blocked rejects everything everywhere (blueprint XV.2)
        if let Some(contact) = store::organs::contact(&self.store.pool, &package.from_organ).await?
        {
            if contact.trust == "blocked" {
                return Err(EngineError::Consequence(format!(
                    "organ {} is blocked",
                    package.from_organ
                )));
            }
        }
        // concepts first: records reference them (one-tap adoption, III.2)
        for concept in &package.concepts {
            store::concepts::adopt(
                &self.store.pool,
                &concept.uid,
                &concept.name,
                Some(&package.from_organ),
                &concept.parents,
            )
            .await?;
        }
        for seed in &package.records {
            ensure_record(&self.store, seed, &package.from_organ).await?;
        }
        let mut applied = Vec::new();
        for fact in &package.facts {
            // two-layer tamper model (XI): the chain step guards content->hash,
            // the signature guards hash->author
            if !nucleus::fact::verify_chain_step(fact) {
                store::organs::quarantine(
                    &self.store.pool,
                    &package.from_organ,
                    "chain step does not verify",
                    &serde_json::to_string(fact).unwrap_or_default(),
                )
                .await?;
                continue;
            }
            if fact.signature.is_some()
                && !crate::trust::verify_fact(&self.store, fact)
                    .await
                    .unwrap_or(false)
            {
                store::organs::quarantine(
                    &self.store.pool,
                    &package.from_organ,
                    "signature does not verify",
                    &serde_json::to_string(fact).unwrap_or_default(),
                )
                .await?;
                continue;
            }
            let imported = NewFact {
                uid: Some(fact.uid.clone()), // idempotent by uid
                record_uid: fact.record_uid.clone(),
                delta: fact.delta,
                at: Some(fact.at),
                actor_uid: fact.actor_uid.clone(), // origin author survives replication
                cause: Cause {
                    kind: CauseKind::Sync,
                    uid: Some(package.from_organ.clone()),
                },
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
                signer
                    .as_ref()
                    .map(|s| s.actor_uid.clone())
                    .unwrap_or_default()
            });
            let prev = store::facts::last_hash(&mut tx).await?;
            let mut sealed = nucleus::fact::seal(news, &prev, Utc::now());
            sealed.signature = fact.signature.clone(); // origin authorship
            store::facts::insert(&mut tx, &sealed).await?;
            store::records::bump_quantity(
                &mut tx,
                &sealed.record_uid,
                sealed.delta,
                &Utc::now().to_rfc3339(),
            )
            .await?;
            tx.commit().await?;
            applied.push(sealed);
        }
        Ok(applied)
    }
}

/// The introduction handshake (blueprint XV.2/XI.1): who I am, where I live,
/// and my public keys — enough for a peer to register me as a contact and
/// verify my signed facts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Introduction {
    pub organ_uid: String,
    pub slug: Option<String>,
    pub head: String,
    pub base_url: String,
    /// `(key_id, public_key_b64)` pairs.
    pub keys: Vec<(String, String)>,
}

impl Engine {
    /// Export my own introduction.
    pub async fn introduction(&self) -> Result<Introduction, EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| EngineError::Consequence("no local organ".into()))?;
        let keys = crate::trust::keys_of(&self.store, &organ.uid).await?;
        Ok(Introduction {
            organ_uid: organ.uid,
            slug: organ.slug,
            head: organ.head,
            base_url: organ.base_url,
            keys,
        })
    }

    /// Adopt a peer's introduction: register the contact under THEIR organ uid
    /// and store their public keys so their signed facts verify.
    pub async fn adopt_introduction(
        &self,
        intro: &Introduction,
        proximity: u32,
    ) -> Result<String, EngineError> {
        let uid = store::organs::add_contact(
            &self.store.pool,
            &intro.organ_uid,
            intro.slug.as_deref(),
            &intro.head,
            &intro.base_url,
            proximity,
        )
        .await?;
        for (key_id, public_key) in &intro.keys {
            crate::trust::adopt_key(&self.store, &intro.organ_uid, key_id, public_key).await?;
        }
        Ok(uid)
    }
}

/// One open promise as it travels to peers' discovery caches (blueprint XV →
/// X). Proximity is NOT here on purpose: the receiver stamps it from its own
/// contact row — offer-ordering never exposes local proximity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenPromiseExport {
    pub promise_uid: String,
    pub concept: Option<String>,
    pub unit: Option<String>,
    pub delta: f64,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    pub confidence: f64,
}

impl Engine {
    /// Queue a visibility-filtered package for a contact. Blocked and
    /// non-`sync_out` contacts are refused. Returns the outbox row uid.
    pub async fn enqueue_sync_to(&self, organ_uid: &str) -> Result<String, EngineError> {
        let contact = store::organs::contact(&self.store.pool, organ_uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(organ_uid.into()))?;
        if contact.trust == "blocked" || !contact.sync_out {
            return Err(EngineError::Consequence(format!(
                "organ {organ_uid} is not a sync-out contact"
            )));
        }
        let from = store::organs::local(&self.store.pool)
            .await?
            .map(|o| o.uid)
            .unwrap_or_default();
        // the ONE visibility gate: the package is what the subject may see
        let package = self.export_package(organ_uid, &from).await?;
        let payload = serde_json::to_string(&package).map_err(EngineError::Json)?;
        Ok(store::organs::outbox_enqueue(&self.store.pool, organ_uid, &payload).await?)
    }

    /// Drain the outbox through a sender (the HTTP boundary in production, an
    /// in-memory wire in tests). Failures stay queued and retry next drain.
    pub async fn drain_outbox<F, Fut>(&self, mut send: F) -> Result<usize, EngineError>
    where
        F: FnMut(store::organs::Contact, Package) -> Fut,
        Fut: std::future::Future<Output = Result<(), String>>,
    {
        let mut sent = 0;
        for row in store::organs::outbox_due(&self.store.pool).await? {
            let Some(contact) = store::organs::contact(&self.store.pool, &row.organ_uid).await?
            else {
                store::organs::outbox_mark(&self.store.pool, &row.uid, false).await?;
                continue;
            };
            let Ok(package) = serde_json::from_str::<Package>(&row.payload) else {
                store::organs::outbox_mark(&self.store.pool, &row.uid, false).await?;
                continue;
            };
            let ok = send(contact, package).await.is_ok();
            store::organs::outbox_mark(&self.store.pool, &row.uid, ok).await?;
            if ok {
                sent += 1;
            }
        }
        Ok(sent)
    }

    /// The open promises `subject` may see — what peers pull into their
    /// discovery caches. Only OPEN, unfilled-party promises travel, and only
    /// through the visibility gate (their target record must be visible).
    pub async fn open_promise_export(
        &self,
        subject: &str,
    ) -> Result<Vec<OpenPromiseExport>, EngineError> {
        let visible = store::visibility::visible_targets(&self.store.pool, subject).await?;
        let mut out = Vec::new();
        for p in store::misc::list_promises(&self.store.pool).await? {
            if p.state != nucleus::PromiseState::Open || p.party_uid.is_some() {
                continue;
            }
            let Some(record_uid) = &p.record_uid else {
                continue;
            };
            if !visible.contains(record_uid) {
                continue;
            }
            let record = store::records::get(&self.store.pool, record_uid).await?;
            out.push(OpenPromiseExport {
                promise_uid: p.uid.clone(),
                concept: record.as_ref().and_then(|r| r.concept_uid.clone()),
                unit: record.as_ref().and_then(|r| r.unit_uid.clone()),
                delta: p.delta,
                window_start: None,
                window_end: p.window_end.clone(),
                confidence: 0.5, // strangers start at the prior; XI history refines
            });
        }
        Ok(out)
    }

    /// Refresh the discovery cache with a contact's open promises (the pull
    /// side of the feed). Proximity is stamped from OUR contact row; blocked
    /// organs are rejected here too.
    pub async fn refresh_discovery(
        &self,
        organ_uid: &str,
        fetched: Vec<OpenPromiseExport>,
    ) -> Result<usize, EngineError> {
        let contact = store::organs::contact(&self.store.pool, organ_uid)
            .await?
            .ok_or_else(|| EngineError::UnknownRecord(organ_uid.into()))?;
        if contact.trust == "blocked" {
            return Err(EngineError::Consequence(format!(
                "organ {organ_uid} is blocked"
            )));
        }
        let mut stored = 0;
        for open in fetched {
            store::senses::upsert_remote_open(
                &self.store.pool,
                &store::senses::RemoteOpenRow {
                    promise_uid: open.promise_uid,
                    organ: organ_uid.to_string(),
                    proximity: contact.proximity,
                    concept: open.concept,
                    unit: open.unit,
                    delta: open.delta,
                    window_start: open.window_start,
                    window_end: open.window_end,
                    confidence: open.confidence,
                },
            )
            .await?;
            stored += 1;
        }
        Ok(stored)
    }
}

async fn ensure_record(
    store: &Store,
    seed: &RecordSeed,
    fallback_organ: &str,
) -> Result<(), EngineError> {
    if store::records::get(&store.pool, &seed.uid).await?.is_some() {
        return Ok(());
    }
    // Insert with the ORIGIN uid so cross-organ joins line up (blueprint XV.2).
    // A slug is a local suggestion, never identity — drop it on collision.
    let slug_taken = match &seed.slug {
        Some(slug) => store::records::resolve(&store.pool, slug).await?.is_some(),
        None => false,
    };
    // Older peers may not send `organ_uid` yet — fall back to the sending
    // organ so lineage still resolves (blueprint: Protein-driven Sync).
    let organ_uid = seed
        .organ_uid
        .clone()
        .or_else(|| Some(fallback_organ.to_string()));
    let now = Utc::now().to_rfc3339();
    store::sqlx::query(
        "INSERT INTO record (uid, slug, kind, head, body, quantity, concept_uid, unit_uid, organ_uid, created_at, updated_at)
         VALUES (?, ?, ?, ?, '', 0, ?, ?, ?, ?, ?)",
    )
    .bind(&seed.uid)
    .bind(if slug_taken { None } else { seed.slug.clone() })
    .bind(&seed.kind)
    .bind(&seed.head)
    .bind(&seed.concept_uid)
    .bind(&seed.unit_uid)
    .bind(&organ_uid)
    .bind(&now)
    .bind(&now)
    .execute(&store.pool)
    .await?;
    Ok(())
}
