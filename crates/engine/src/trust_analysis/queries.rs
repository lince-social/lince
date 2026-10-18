use super::*;
use crate::actions::{ActionOutcome, VerifiedActionAuthorship};
use chrono::{DateTime, Utc};
use nucleus::trust_analysis::commands::Command;
use serde_json::json;
use std::collections::BTreeMap;

pub(super) struct QueryEvaluation {
    pub analysis: nucleus::trust_analysis::query::Analysis,
    pub proof_records: Option<BTreeMap<nucleus::karma::CanonicalHash, String>>,
    pub revision: u64,
    pub publication_expires_at: Option<i64>,
}

impl Engine {
    pub(super) async fn evaluate_trust_query(
        &self,
        person: &str,
        actor: Option<&str>,
        query: &nucleus::trust_analysis::query::Query,
        as_organ: bool,
        now: DateTime<Utc>,
    ) -> Result<QueryEvaluation, EngineError> {
        self.evaluate_trust_query_mode(person, actor, query, as_organ, now, false)
            .await
    }

    pub(super) async fn evaluate_trust_query_mode(
        &self,
        person: &str,
        actor: Option<&str>,
        query: &nucleus::trust_analysis::query::Query,
        as_organ: bool,
        now: DateTime<Utc>,
        publication_inputs: bool,
    ) -> Result<QueryEvaluation, EngineError> {
        query.validate().map_err(invalid)?;
        let changed = self.watch_query_changes();
        let revision = *changed.borrow();
        let known_filter = [&query.issuers, &query.counterparties]
            .into_iter()
            .any(|filter| {
                filter
                    .include
                    .iter()
                    .chain(&filter.exclude)
                    .any(|condition| condition.known.is_some())
            });
        let known_policy = known_filter.then(|| {
            nucleus::trust_analysis::paths::Policy::for_anchor(Identity::Person(person.into()))
        });
        let contacts = if let Some(policy) = query.contacts.as_ref().or(known_policy.as_ref()) {
            let mut policy = policy.clone();
            policy
                .exclude_everywhere
                .extend(query.exclude_everywhere.clone());
            policy.exclude_everywhere.sort();
            policy.exclude_everywhere.dedup();
            Some(
                self.contact_path_evaluation(person, actor, &policy, as_organ, now)
                    .await?,
            )
        } else {
            None
        };
        let mut reader_tx = self.store.pool.begin().await?;
        let reader = self.contact_reader_on(&mut reader_tx, person, actor, as_organ).await?;
        drop(reader_tx);
        let evidence = self
            .receipt_analysis_evidence(person, query, actor, now, publication_inputs, &reader,nucleus::trust_analysis::MAX_BYTES.saturating_sub(contacts.as_ref().map_or(0,|contacts|contacts.bytes)),nucleus::trust_analysis::MAX_DISCLOSURES.saturating_sub(contacts.as_ref().map_or(0,|contacts|contacts.events)))
            .await?;
        let mut identities = BTreeSet::from([query.subject.clone()]);
        for source in &evidence.sources {
            identities.extend([
                Identity::Organ(source.origin.clone()),
                Identity::Organ(source.giver.clone()),
                Identity::Organ(source.receiver.clone()),
            ]);
            for copy in &source.copies {
                identities.extend(
                    copy.persons
                        .iter()
                        .map(|person| Identity::Person(person.person.clone())),
                );
            }
        }
        if identities.len() > nucleus::trust_analysis::MAX_IDENTITIES {
            return Err(invalid(
                "The evidence identity budget was reached; narrow this query",
            ));
        }
        let mut tx = self.store.pool.begin().await?;
        self.require_permission_on(&mut tx, actor, "record:read")
            .await?;
        self.contact_reader_on(&mut tx, person, actor, as_organ)
            .await?;
        let mut contexts = contacts
            .as_ref()
            .map(|contacts| contacts.contexts.clone())
            .unwrap_or_else(BTreeMap::new);
        if contexts
            .keys()
            .chain(identities.iter())
            .collect::<BTreeSet<_>>()
            .len()
            > nucleus::trust_analysis::MAX_IDENTITIES
        {
            return Err(invalid(
                "The shared graph/evidence identity budget was reached; narrow this query",
            ));
        }
        for identity in identities {
            let mut context = self
                .contact_identity_context_on(&mut tx, &identity, actor)
                .await?;
            context.known |= identity == Identity::Person(person.into());
            if let Some(previous) = contexts.get(&identity) {
                context.known |= previous.known;
            }
            contexts.insert(identity, context);
        }
        for record in evidence.records.iter().chain(
            contacts
                .iter()
                .flat_map(|contacts| contacts.proof_records.values()),
        ) {
            self.contact_record_readable_on(&mut tx, record, actor)
                .await?;
        }
        for record in &evidence.custody_records {self.validate_local_custody_on(&mut tx,record,actor,nucleus::execution::now().timestamp()).await?;}
        for record in contacts.iter().flat_map(|contacts|&contacts.custody_records) {self.validate_local_custody_on(&mut tx,record,actor,nucleus::execution::now().timestamp()).await?;}
        let mut analysis = nucleus::trust_analysis::query::analyze(
            evidence.sources,
            &contexts,
            contacts
                .as_ref()
                .filter(|_| query.contacts.is_some())
                .map(|contacts| &contacts.result),
            query,
        )
        .map_err(invalid)?;
        analysis.partial |=
            evidence.partial || contacts.as_ref().is_some_and(|contacts| contacts.partial);
        if *changed.borrow() != revision {
            return Err(invalid(
                "Evidence or permissions changed during analysis; refresh the local view",
            ));
        }

        Ok(QueryEvaluation {
            analysis,
            proof_records: contacts.map(|contacts| contacts.proof_records),
            revision,
            publication_expires_at: evidence.expires_at,
        })
    }
}
impl Engine {
    pub(super) async fn trust_query_command(
        &self,
        person: &str,
        request_id: &str,
        command: &Command,
        actor: Option<&str>,
        now: DateTime<Utc>,
        authorship: Option<&VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        match command {
            Command::AnalyzeQuery { query, as_organ } => {
                let evaluated = self
                    .evaluate_trust_query(person, actor, query, *as_organ, now)
                    .await?;
                let data = json!({"trust":{"kind":"query_analysis","query":query,"analysis":evaluated.analysis,"proof_records":evaluated.proof_records,"checked_at":now.timestamp(),"meaning":"Permitted local evidence, grouped by exact meaning, unit, object and evidence class. Weighted indices are local judgments, never delivered quantities or fraud probabilities. Missing or withheld evidence is not zero contribution."}});
                if serde_json::to_vec(&data)?.len() > nucleus::trust_analysis::MAX_BYTES {
                    return Err(invalid(
                        "The result byte budget was reached; narrow this query",
                    ));
                }
                Ok(ActionOutcome {
                    data: Some(data),
                    ..Default::default()
                })
            }
            Command::LoadQuery { record } => {
                let record = self.resolve(record).await?;
                let mut tx = self.store.pool.begin().await?;
                self.contact_record_readable_on(&mut tx, &record, actor)
                    .await?;
                let saved = store::trust_queries::get_on(&mut tx, &record).await?;
                if saved.owner != person {
                    return Err(EngineError::Forbidden(
                        "Open your own saved reader query".into(),
                    ));
                }
                Ok(ActionOutcome {
                    data: Some(
                        json!({"trust":{"kind":"saved_query","record":saved.record,"name":saved.name,"query":saved.query,"policy":saved.query.id().map_err(invalid)?}}),
                    ),
                    ..Default::default()
                })
            }
            Command::ListQueries { before } => {
                let principal = serde_json::to_string(&(person, actor))?;
                let before: Option<String> = if let Some(before) = before {
                    Some(
                        self.decode_read_cursor(
                            "trust-queries",
                            &principal,
                            before,
                            now.timestamp(),
                        )
                        .await?,
                    )
                } else {
                    None
                };
                let mut tx = self.store.pool.begin().await?;
                let records:Vec<(String,String)> = store::sqlx::query_as("SELECT q.record_uid,r.head FROM trust_query_record q JOIN record r ON r.uid=q.record_uid WHERE q.owner_person_uid=? AND r.deleted_at IS NULL AND (? IS NULL OR q.record_uid<?) ORDER BY q.record_uid DESC LIMIT 65")
                    .bind(person).bind(&before).bind(&before).fetch_all(&mut *tx).await?;
                let more = records.len() > 64;
                let mut entries = Vec::new();
                let mut last = None;
                for (record, name) in records.into_iter().take(64) {
                    last = Some(record.clone());
                    if self
                        .contact_record_readable_on(&mut tx, &record, actor)
                        .await
                        .is_ok()
                    {
                        let saved = store::trust_queries::get_on(&mut tx, &record).await?;
                        entries.push(json!({"record":record,"name":name,"policy":saved.query.id().map_err(invalid)?}));
                    }
                }
                drop(tx);
                let next = if more {
                    if let Some(last) = last {
                        Some(
                            self.encode_read_cursor(
                                "trust-queries",
                                &principal,
                                &last,
                                now.timestamp(),
                            )
                            .await?,
                        )
                    } else {
                        None
                    }
                } else {
                    None
                };
                Ok(ActionOutcome {
                    data: Some(
                        json!({"trust":{"kind":"query_list","entries":entries,"next":next,"partial":next.is_some()}}),
                    ),
                    ..Default::default()
                })
            }
            Command::SaveQuery { name, query } => {
                query.validate().map_err(invalid)?;
                let signer = self.transfer_person_signer(person, authorship).await?;
                let mut tx = store::write_tx(&self.store.pool).await?;
                self.require_permission_on(&mut tx, actor, "record:create")
                    .await?;
                if let Some(saved) =
                    store::trust_queries::replay_on(&mut tx, person, request_id, name, query)
                        .await?
                {
                    self.contact_record_readable_on(&mut tx, &saved.record, actor)
                        .await?;
                    return Ok(ActionOutcome {
                        created: Some(saved.record.clone()),
                        data: Some(
                            json!({"trust":{"kind":"saved_query","record":saved.record,"name":saved.name,"query":saved.query,"policy":saved.query.id().map_err(invalid)?,"replayed":true}}),
                        ),
                        ..Default::default()
                    });
                }
                let existing = store::trust_queries::find_on(&mut tx, person, query).await?;
                if let Some(saved) = &existing {
                    self.contact_record_readable_on(&mut tx, &saved.record, actor)
                        .await?;
                }
                let record = existing
                    .as_ref()
                    .map(|saved| saved.record.clone())
                    .unwrap_or_else(|| nucleus::new_uid("r"));
                let checkpoint = if existing.is_none() {
                    self.record_checkpoint_on(
                        &mut tx,
                        actor,
                        vec![record.clone()],
                        protein::authority::Operation::Create,
                    )
                    .await?
                } else {
                    None
                };
                let (committed, mut tx) = store::trust_queries::save(
                    tx,
                    store::trust_queries::Input {
                        record,
                        person: person.into(),
                        request_id: request_id.into(),
                        name: name.clone(),
                        query: query.clone(),
                        authorization_intent: authorship.map(|proof| proof.intent_uid.clone()),
                    },
                    now,
                    |hash| signer.as_ref().map(|signer| signer.sign_hash(hash)),
                )
                .await?;
                if committed.created
                    && let Some(checkpoint) = checkpoint
                {
                    checkpoint
                        .bounded()
                        .finish(&mut tx, Default::default())
                        .await?;
                }
                tx.commit().await?;
                Ok(ActionOutcome {
                    created: Some(committed.saved.record.clone()),
                    data: Some(
                        json!({"trust":{"kind":"saved_query","record":committed.saved.record,"name":committed.saved.name,"query":committed.saved.query,"policy":committed.saved.query.id().map_err(invalid)?,"replayed":!committed.created}}),
                    ),
                    facts: if committed.created {
                        self.publish_committed_fact(committed.saved.fact)
                    } else {
                        Vec::new()
                    },
                    ..Default::default()
                })
            }
            _ => Err(invalid("Choose a saved reader query operation")),
        }
    }
}
