use super::*;
use crate::actions::ActionOutcome;
use chrono::{DateTime, Utc};
use nucleus::trust_analysis::{
    IdentityContext,
    contacts::{CheckedDisclosure, permitted_edges},
    paths,
};
use serde_json::json;
use std::collections::BTreeMap;
use store::sqlx::{Sqlite, Transaction};

pub(super) struct ContactEvaluation {
    pub result: paths::ResultSet,
    pub contexts: BTreeMap<Identity, IdentityContext>,
    pub proof_records: BTreeMap<nucleus::karma::CanonicalHash, String>,
    pub partial: bool,
    pub custody_records:BTreeSet<String>,
    pub bytes:usize,
    pub events:usize,
}

impl Engine {
    pub(super) async fn contact_reader_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        person: &str,
        actor: Option<&str>,
        as_organ: bool,
    ) -> Result<Identity, EngineError> {
        if !as_organ {
            return Ok(Identity::Person(person.into()));
        }
        self.require_permission_on(tx, actor, "organ:update")
            .await?;
        let organ: String = store::sqlx::query_scalar(
            "SELECT uid FROM record WHERE slug=? AND kind='organ' AND deleted_at IS NULL",
        )
        .bind(store::organs::LOCAL_ORGAN_SLUG)
        .fetch_one(&mut **tx)
        .await?;
        Ok(Identity::Organ(organ))
    }

    pub(super) async fn contact_record_readable_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        record: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        let graph = crate::access::policy_graph_on(tx, &[record.into()])
            .await
            .map_err(|error| invalid(error.to_string()))?;
        if !self.readable_on(tx, actor, &graph).await?.contains(record) {
            return Err(EngineError::Forbidden(
                "Contact evidence is unavailable".into(),
            ));
        }
        Ok(())
    }

    pub(super) async fn contact_identity_context_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        identity: &Identity,
        actor: Option<&str>,
    ) -> Result<IdentityContext, EngineError> {
        let record: Option<(String, Option<String>)> = store::sqlx::query_as(
            "SELECT kind,organ_uid FROM record WHERE uid=? AND deleted_at IS NULL",
        )
        .bind(identity.uid())
        .fetch_optional(&mut **tx)
        .await?;
        let Some((kind, organ)) = record else {
            return Ok(IdentityContext::default());
        };
        if kind != identity.kind() {
            return Ok(IdentityContext::default());
        }
        let graph = crate::access::policy_graph_on(tx, &[identity.uid().into()])
            .await
            .map_err(|error| invalid(error.to_string()))?;
        let readable = self
            .readable_on(tx, actor, &graph)
            .await?
            .contains(identity.uid());
        let network_owner = match identity {
            Identity::Organ(uid) => Some(uid.as_str()),
            Identity::Person(_) => organ.as_deref(),
        };
        let contact: Option<(String, i64)> = if let Some(organ) = network_owner {
            store::sqlx::query_as("SELECT trust,proximity FROM organ_contact WHERE record_uid=?")
                .bind(organ)
                .fetch_optional(&mut **tx)
                .await?
        } else {
            None
        };
        Ok(IdentityContext {
            readable,
            blocked: contact
                .as_ref()
                .is_some_and(|(trust, _)| trust == "blocked"),
            known: matches!(identity, Identity::Organ(_))
                && contact.as_ref().is_some_and(|(trust, _)| trust == "known"),
            proximity: if matches!(identity, Identity::Organ(_)) {
                contact.and_then(|(_, proximity)| u32::try_from(proximity).ok())
            } else {
                None
            },
        })
    }

    pub(super) async fn contact_listing(
        &self,
        person: &str,
        actor: Option<&str>,
        before: Option<&str>,
        as_organ: bool,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        let principal = serde_json::to_string(&(actor, person, as_organ))?;
        let position: Option<String> = match before {
            Some(token) => Some(
                self.decode_read_cursor("contact-disclosures", &principal, token, now.timestamp())
                    .await?,
            ),
            None => None,
        };
        let mut tx = self.store.pool.begin().await?;
        let reader = self
            .contact_reader_on(&mut tx, person, actor, as_organ)
            .await?;
        let candidates:Vec<String>=store::sqlx::query_scalar("SELECT record_uid FROM contact_disclosure_record WHERE owner_person_uid=? AND (? IS NULL OR record_uid<?) ORDER BY record_uid DESC LIMIT 257")
            .bind(person).bind(&position).bind(&position).fetch_all(&mut *tx).await?;
        let more = candidates.len() > 256;
        let mut entries = Vec::new();
        let mut last = None;
        let mut examined = 0;
        for record in candidates.iter().take(256) {
            examined += 1;
            last = Some(record.clone());
            if self
                .contact_record_readable_on(&mut tx, record, actor)
                .await
                .is_err()
            {
                continue;
            }
            let saved = store::contact_disclosures::get_on(&mut tx, record).await?;
            if !saved
                .document
                .statement
                .audience
                .allows(&reader, &saved.document.statement.issuer)
            {
                continue;
            }
            if self
                .contact_profiles_on(&mut tx, &saved.document, actor)
                .await
                .is_err()
            {
                continue;
            }
            let status = self
                .authenticate_contact_disclosure_on(&mut tx, &saved.document, now.timestamp())
                .await;
            let state = match status {
                Ok(IdentityAuthenticity::Accepted) => "recognized_signature",
                Ok(IdentityAuthenticity::WithheldAuthority) => "withheld_authority",
                Err(EngineError::Conflict {
                    code: "contact_disclosure_unknown_identity",
                    ..
                }) => "unknown_identity",
                Err(_) => "invalid_proof",
            };
            let latest = store::contact_disclosures::latest_revision_on(
                &mut tx,
                person,
                &saved.document.statement,
            )
            .await?;
            let current_revision = (latest == saved.document.statement.revision).then_some(latest);
            entries.push(json!({"record":record,"document":saved.document,"state":state,"current_revision":current_revision}));
            if entries.len() == 64 {
                break;
            }
        }
        drop(tx);
        let continuation = if more || examined < candidates.len() {
            if let Some(last) = last {
                Some(
                    self.encode_read_cursor(
                        "contact-disclosures",
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
                json!({"trust":{"kind":"contact_list","entries":entries,"next":continuation,"partial":continuation.is_some(),"checked_at":now.timestamp()}}),
            ),
            ..Default::default()
        })
    }

    pub(super) async fn contact_path_search(
        &self,
        person: &str,
        actor: Option<&str>,
        policy: &paths::Policy,
        as_organ: bool,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        let evaluated = self
            .contact_path_evaluation(person, actor, policy, as_organ, now)
            .await?;
        Ok(ActionOutcome {
            data: Some(
                json!({"trust":{"kind":"contact_paths","result":evaluated.result,"proof_records":evaluated.proof_records,"partial":evaluated.partial,"checked_at":now.timestamp(),"meaning":"Shortest permitted observed paths. Mutual signatures do not establish reliability or independent people."}}),
            ),
            ..Default::default()
        })
    }

    pub(super) async fn contact_path_evaluation(
        &self,
        person: &str,
        actor: Option<&str>,
        policy: &paths::Policy,
        as_organ: bool,
        now: DateTime<Utc>,
    ) -> Result<ContactEvaluation, EngineError> {
        policy.validate().map_err(invalid)?;
        let cached=self.local_custody_originals(person,actor,Some(nucleus::trust_analysis::serving::SourceKind::Contact),now.timestamp(),nucleus::trust_analysis::MAX_BYTES).await?;
        let started = tokio::time::Instant::now();
        let mut tx = self.store.pool.begin().await?;
        let reader = self
            .contact_reader_on(&mut tx, person, actor, as_organ)
            .await?;
        let mut contexts = BTreeMap::new();
        contexts.insert(
            policy.anchor.clone(),
            self.contact_identity_context_on(&mut tx, &policy.anchor, actor)
                .await?,
        );
        let rows:Vec<(String,String)>=store::sqlx::query_as("SELECT c.record_uid,c.relation_hash FROM contact_disclosure_record c WHERE c.owner_person_uid=? AND c.revision=(SELECT MAX(n.revision) FROM contact_disclosure_record n WHERE n.owner_person_uid=c.owner_person_uid AND n.relation_hash=c.relation_hash AND n.issuer_kind=c.issuer_kind AND n.issuer_uid=c.issuer_uid) ORDER BY c.relation_hash,c.issuer_kind,c.issuer_uid,c.document_hash LIMIT 5001")
            .bind(person).fetch_all(&mut *tx).await?;
        let mut partial = rows.len() > nucleus::trust_analysis::MAX_DISCLOSURES || !cached.complete;
        let boundary = if partial {
            rows.get(nucleus::trust_analysis::MAX_DISCLOSURES)
                .map(|(_, relation)| relation.clone())
        } else {
            None
        };
        let mut checked: Vec<CheckedDisclosure> = Vec::new();
        let mut proof_records = BTreeMap::new();
        let mut failed = BTreeSet::new();
        let mut authority = BTreeMap::new();
        let mut bytes = cached.bytes;
        let mut events=cached.events;
        let mut custody_records=BTreeSet::new();
        for (record, relation) in rows
            .into_iter()
            .take(nucleus::trust_analysis::MAX_DISCLOSURES)
        {
            events+=1;
            if events>nucleus::trust_analysis::MAX_DISCLOSURES {partial=true;break;}
            if boundary.as_ref() == Some(&relation) {
                continue;
            }
            if started.elapsed().as_secs() >= 15 {
                partial = true;
                failed.insert(relation);
                break;
            }
            let saved = store::contact_disclosures::get_on(&mut tx, &record).await?;
            bytes = bytes.saturating_add(serde_json::to_vec(&saved.document)?.len());
            if bytes > nucleus::trust_analysis::MAX_BYTES {
                partial = true;
                failed.insert(relation);
                break;
            }
            let mut readable = self
                .contact_record_readable_on(&mut tx, &record, actor)
                .await
                .is_ok()
                && saved
                    .document
                    .statement
                    .audience
                    .allows(&reader, &saved.document.statement.issuer);
            let key = (
                saved.document.statement.issuer.clone(),
                saved.document.key_id.clone(),
                saved.document.public_key.clone(),
            );
            let status = if let Some(status) = authority.get(&key) {
                if !crate::trust::verify_bytes_strict(
                    &saved.document.public_key,
                    &saved.document.signing_bytes().map_err(invalid)?,
                    &saved.document.signature,
                ) {
                    Err(invalid("Invalid contact signature"))
                } else {
                    Ok(*status)
                }
            } else {
                let status = self
                    .authenticate_contact_disclosure_on(&mut tx, &saved.document, now.timestamp())
                    .await;
                if let Ok(status) = status {
                    authority.insert(key, status);
                }
                status
            };
            let status = match status {
                Ok(status) => status,
                Err(_) => {
                    if readable {
                        partial = true;
                    }
                    failed.insert(relation);
                    continue;
                }
            };
            if readable {
                let mut endpoints = Vec::new();
                for identity in [
                    &saved.document.statement.issuer,
                    &saved.document.statement.contact,
                ] {
                    let mut context = if let Some(context) = contexts.get(identity) {
                        context.clone()
                    } else {
                        self.contact_identity_context_on(&mut tx, identity, actor)
                            .await?
                    };
                    context.known |= identity == &reader;
                    if !context.readable || context.blocked {
                        readable = false;
                        break;
                    }
                    endpoints.push((identity.clone(), context));
                }
                if readable {
                    let additions = endpoints
                        .iter()
                        .filter(|(identity, _)| !contexts.contains_key(identity))
                        .count();
                    if contexts.len() + additions > nucleus::trust_analysis::MAX_IDENTITIES {
                        partial = true;
                        failed.insert(relation);
                        break;
                    }
                    contexts.extend(endpoints);
                    let statement = &saved.document.statement;
                    if statement.issuer == reader
                        && !statement.withdrawn
                        && statement.expires_at > now.timestamp()
                        && status == IdentityAuthenticity::Accepted
                    {
                        if let Some(context) = contexts.get_mut(&statement.contact) {
                            context.known = true;
                        }
                    }
                }
            }
            if readable && status == IdentityAuthenticity::Accepted {
                proof_records
                    .entry(saved.document.statement.id().map_err(invalid)?)
                    .or_insert(saved.record.clone());
            }
            checked.push(CheckedDisclosure {
                document: saved.document,
                readable,
                authority_current: status == IdentityAuthenticity::Accepted,
            });
        }
        for input in &cached.items {
            let nucleus::trust_analysis::serving::Artifact::Contact(document)=input.artifact.as_ref()else{continue;};
            let status=self.authenticate_contact_disclosure_on(&mut tx,document,now.timestamp()).await;
            let accepted=matches!(status,Ok(IdentityAuthenticity::Accepted));
            let mut readable=input.readable && self.contact_record_readable_on(&mut tx,&input.record,actor).await.is_ok();
            let mut endpoints=Vec::new();
            for identity in [&document.statement.issuer,&document.statement.contact] {
                let mut context=self.contact_identity_context_on(&mut tx,identity,actor).await?;
                context.known|=identity==&reader;
                if !context.readable || context.blocked {readable=false;}
                endpoints.push((identity.clone(),context));
            }
            if readable && accepted {
                let additions=endpoints.iter().filter(|(identity,_)|!contexts.contains_key(identity)).count();
                if contexts.len()+additions>nucleus::trust_analysis::MAX_IDENTITIES {partial=true;break;}
                contexts.extend(endpoints);
                proof_records.entry(document.statement.id().map_err(invalid)?).or_insert(input.record.clone());
                custody_records.insert(input.record.clone());
            }
            checked.push(CheckedDisclosure {document:document.clone(),readable,authority_current:accepted});
        }
        if !cached.complete {checked.clear();}
        for record in &custody_records {self.validate_local_custody_on(&mut tx,record,actor,nucleus::execution::now().timestamp()).await?;}
        checked.retain(|proof| {
            proof
                .document
                .statement
                .relation()
                .is_ok_and(|relation| !failed.contains(relation.as_str()))
        });
        let edges =
            permitted_edges(&checked, &contexts, &reader, now.timestamp()).map_err(invalid)?;
        let result = paths::search(&edges, &contexts, policy).map_err(invalid)?;
        let used = result
            .paths
            .iter()
            .flat_map(|path| path.proofs.iter().flatten())
            .cloned()
            .collect::<BTreeSet<_>>();
        proof_records.retain(|proof, _| used.contains(proof));
        Ok(ContactEvaluation {
            custody_records,
            bytes,
            events,
            partial: partial || result.identity_limit_reached || result.depth_limit_reached,
            result,
            contexts,
            proof_records,
        })
    }
}
