use super::*;
use nucleus::trust_analysis::{MAX_BYTES, MAX_DISCLOSURES, query::SourceEvidence};
use std::time::Duration;

pub(crate) struct AnalysisEvidence {
    pub sources: Vec<SourceEvidence>,
    pub records: BTreeSet<String>,
    pub partial: bool,
    pub expires_at: Option<i64>,
    pub custody_records: BTreeSet<String>,
}

impl Engine {
    pub(crate) async fn receipt_analysis_evidence(
        &self,
        person: &str,
        query: &nucleus::trust_analysis::query::Query,
        actor: Option<&str>,
        now: DateTime<Utc>,
        publication_inputs: bool,
        reader: &nucleus::trust_analysis::Identity,
        max_bytes: usize,
        max_events: usize,
    ) -> Result<AnalysisEvidence, EngineError> {
        let byte_limit = MAX_BYTES.min(max_bytes);
        let event_limit = MAX_DISCLOSURES.min(max_events);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
        let cached = self
            .local_custody_originals(
                person,
                actor,
                Some(nucleus::trust_analysis::serving::SourceKind::Receipt),
                now.timestamp(),
                byte_limit,
            )
            .await?;
        let rows:Vec<(String,String,String)> = store::sqlx::query_as("SELECT c.record_uid,c.source_origin,c.source_occurrence FROM transfer_receipt_record c JOIN record r ON r.uid=c.record_uid WHERE c.kind='publication' AND r.deleted_at IS NULL ORDER BY c.created_at DESC,c.record_uid DESC LIMIT 5001")
            .fetch_all(&self.store.pool).await?;
        let mut evidence = AnalysisEvidence {
            sources: Vec::new(),
            records: BTreeSet::new(),
            partial: rows.len() > MAX_DISCLOSURES,
            expires_at: None,
            custody_records: BTreeSet::new(),
        };
        let mut sources = BTreeSet::new();
        let mut retained: BTreeMap<_, Vec<_>> = BTreeMap::new();
        let mut withheld = BTreeSet::new();
        if !cached.complete || cached.events > event_limit {
            evidence.partial = true;
            return Ok(evidence);
        }
        let cached_bytes = cached.bytes;
        let cached_events = cached.events;
        for input in cached.items {
            let nucleus::trust_analysis::serving::Artifact::Receipt(document) =
                input.artifact.as_ref()
            else {
                continue;
            };
            let Some(bundle) = document.public_bundle() else {
                continue;
            };
            let key = (
                bundle.publication.scope.source.origin.clone(),
                bundle.publication.scope.source.occurrence.clone(),
            );
            if !input.readable
                || self
                    .verify_transfer_receipt_document(document, now)
                    .await
                    .is_err()
            {
                withheld.insert(key);
                evidence.partial = true;
                continue;
            }
            sources.insert(key.clone());
            retained.entry(key).or_default().push(input);
        }
        for batch in rows.chunks(64) {
            if tokio::time::Instant::now() >= deadline {
                evidence.partial = true;
                break;
            }
            let candidates = batch
                .iter()
                .map(|(record, _, _)| store::receipt_events::Candidate {
                    record: record.clone(),
                    bytes: 0,
                    created_at: String::new(),
                    version: 1,
                })
                .collect::<Vec<_>>();
            let allowed = super::collection::visible(self, &candidates, actor).await?;
            for (record, origin, occurrence) in batch {
                if allowed.contains(record) {
                    sources.insert((
                        origin.clone(),
                        CanonicalHash::parse(occurrence.clone())
                            .map_err(|error| invalid(error.to_string()))?,
                    ));
                }
            }
        }
        let mut used_bytes = cached_bytes;
        let mut used_events = cached_events;
        let mut projected_bytes = 0usize;
        for (origin, occurrence) in sources {
            if withheld.contains(&(origin.clone(), occurrence.clone())) {
                continue;
            }
            if used_bytes > byte_limit
                || used_events > event_limit
                || tokio::time::Instant::now() >= deadline
            {
                evidence.partial = true;
                break;
            }
            let mut collection = super::collection::Collection::new(&origin, &occurrence);
            collection.restrict_budget(
                byte_limit.saturating_sub(used_bytes),
                event_limit.saturating_sub(used_events),
            );
            let loaded = tokio::time::timeout_at(deadline, async {
                let mut before: Option<(String, String)> = None;
                let mut scanned = 0usize;
                loop {
                    let rows = store::receipt_events::candidates(
                        &self.store.pool,
                        &origin,
                        occurrence.as_str(),
                        before
                            .as_ref()
                            .map(|(date, record)| (date.as_str(), record.as_str())),
                    )
                    .await?;
                    let more = rows.len() > 64;
                    let allowed = super::collection::visible(self, &rows, actor).await?;
                    for row in rows.into_iter().take(64) {
                        scanned += 1;
                        if scanned > MAX_DISCLOSURES {
                            collection.partial = true;
                            break;
                        }
                        before = Some((row.created_at.clone(), row.record.clone()));
                        if !allowed.contains(&row.record) {
                            let owned:bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_receipt_record WHERE record_uid=? AND owner_person_uid=?)")
                                .bind(&row.record).bind(person).fetch_one(&self.store.pool).await?;
                            collection.partial |= owned;
                            continue;
                        }
                        let saved = store::transfer_receipts::get(&self.store.pool, &row.record).await?;
                        if saved.document.public_bundle().is_some_and(|bundle| {
                            !bundle.publication.scope.audience.allows(reader, &nucleus::trust_analysis::Identity::Organ(bundle.publication.scope.source.origin.clone()))
                                || publication_inputs && bundle.publication.scope.audience != nucleus::trust_analysis::contacts::Audience::Public
                        }) {
                            let owned:bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM transfer_receipt_record WHERE record_uid=? AND owner_person_uid=?)").bind(&row.record).bind(person).fetch_one(&self.store.pool).await?;
                            collection.partial |= owned;
                            continue;
                        }
                        if !collection.load(self, &row, true, now).await? {
                            break;
                        }
                    }
                    if !more || collection.partial {
                        break;
                    }
                }
                let resolutions = collection.complete(self, actor, now).await?;
                collection.partial |= resolutions.values().any(|resolution| {
                    !resolution.missing.is_empty() || !resolution.unresolved.is_empty()
                });
                Ok::<(), EngineError>(())
            })
            .await;
            match loaded {
                Ok(result) => result?,
                Err(_) => {
                    evidence.partial = true;
                    break;
                }
            }
            let (bytes, events) = collection.used_budget();
            used_bytes += bytes;
            used_events += events;
            if collection.partial {
                evidence.partial = true;
                continue;
            }
            if publication_inputs
                && collection.artifacts.values().any(|saved| {
                    saved
                        .document
                        .public_bundle()
                        .is_none_or(|bundle| bundle.publication.scope.expires_at <= now.timestamp())
                })
            {
                evidence.partial = true;
                continue;
            }
            let cached = retained.get(&(origin.clone(), occurrence.clone()));
            let mut inputs = collection
                .artifacts
                .values()
                .map(|saved| nucleus::trust_analysis::evidence::Receipt {
                    document: &saved.document,
                    provenance: nucleus::transfer::receipt_history::Provenance::Record {
                        record: saved.record.clone(),
                        version: saved.version,
                    },
                })
                .collect::<Vec<_>>();
            for input in cached.into_iter().flatten() {
                if let nucleus::trust_analysis::serving::Artifact::Receipt(document) =
                    input.artifact.as_ref()
                {
                    inputs.push(nucleus::trust_analysis::evidence::Receipt {
                        document,
                        provenance: nucleus::transfer::receipt_history::Provenance::Record {
                            record: input.record.clone(),
                            version: 1,
                        },
                    });
                }
            }
            if inputs.is_empty() {
                continue;
            }
            let source_expiry = inputs
                .iter()
                .filter_map(|input| {
                    input
                        .document
                        .public_bundle()
                        .map(|bundle| bundle.publication.scope.expires_at)
                })
                .chain(cached.into_iter().flatten().map(|input| input.expires_at))
                .min();
            let mut groups = nucleus::trust_analysis::evidence::receipts(&inputs, now.timestamp())
                .map_err(invalid)?;
            let Some(group) = groups.pop() else {
                continue;
            };
            let giver = group.giver.clone();
            let receiver = group.receiver.clone();
            let partial = group.partial;
            let copies = group.source_evidence().copies;
            let records = inputs
                .iter()
                .filter_map(|input| match &input.provenance {
                    nucleus::transfer::receipt_history::Provenance::Record { record, .. } => {
                        Some(record.clone())
                    }
                    _ => None,
                })
                .collect::<BTreeSet<_>>();
            let size = serde_json::to_vec(&copies)?.len();
            if partial
                || projected_bytes.saturating_add(size) > byte_limit
                || evidence
                    .sources
                    .iter()
                    .map(|source| source.copies.len())
                    .sum::<usize>()
                    + copies.len()
                    > event_limit
            {
                evidence.partial = true;
                if tokio::time::Instant::now() >= deadline {
                    break;
                }
                continue;
            }
            let subject_role = if query.role == nucleus::transfer::outcome::Role::Giver {
                nucleus::transfer::receipt::ApprovalRole::Giver
            } else {
                nucleus::transfer::receipt::ApprovalRole::Receiver
            };
            let subject_matches = match &query.subject {
                nucleus::trust_analysis::Identity::Organ(uid) => {
                    uid == if query.role == nucleus::transfer::outcome::Role::Giver {
                        &giver
                    } else {
                        &receiver
                    }
                }
                nucleus::trust_analysis::Identity::Person(uid) => copies
                    .iter()
                    .flat_map(|copy| &copy.persons)
                    .any(|person| person.role == subject_role && &person.person == uid),
            };
            if !subject_matches {
                continue;
            }
            projected_bytes += size;
            if let Some(expiry) = source_expiry {
                evidence.expires_at = Some(
                    evidence
                        .expires_at
                        .map_or(expiry, |previous| previous.min(expiry)),
                );
            }
            evidence.custody_records.extend(
                cached
                    .into_iter()
                    .flatten()
                    .map(|input| input.record.clone()),
            );
            evidence.records.extend(records);
            evidence.sources.push(SourceEvidence {
                origin,
                occurrence,
                giver,
                receiver,
                copies,
                partial: false,
            });
        }
        Ok(evidence)
    }
}
