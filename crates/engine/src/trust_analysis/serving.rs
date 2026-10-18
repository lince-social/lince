use super::*;
use crate::actions::{ActionOutcome, VerifiedActionAuthorship};
use chrono::{DateTime, Utc};
use nucleus::trust_analysis::serving::{
    self as model, Artifact, Command, Review, Scope, Source, SourceKind,
};
use serde_json::{Value, json};
mod discovery;
mod recipes;

pub(super) struct Snapshot {
    pub(super) artifact: Artifact,
    pub(super) version: u64,
    pub(super) revision: i64,
    pub(super) authority: AuthorityStamp,
}

#[derive(PartialEq, Eq)]
pub(super) struct AuthorityStamp {
    counts: (i64, i64, i64, i64, i64, i64),
    organs: Vec<(
        String,
        nucleus::identity::roots::State,
        Option<(String, String)>,
    )>,
}

pub(super) async fn authority_stamp(
    connection: &mut store::sqlx::SqliteConnection,
    artifact: &Artifact,
) -> Result<AuthorityStamp, EngineError> {
    authority_stamp_many(connection, std::iter::once(artifact)).await
}

pub(super) async fn authority_stamp_many<'a>(
    connection: &mut store::sqlx::SqliteConnection,
    artifacts: impl IntoIterator<Item = &'a Artifact>,
) -> Result<AuthorityStamp, EngineError> {
    let counts=store::sqlx::query_as("SELECT (SELECT COUNT(*) FROM identity_key),(SELECT COUNT(*) FROM identity_revocation),(SELECT COUNT(*) FROM identity_succession),(SELECT COUNT(*) FROM profile_claim_record),(SELECT COUNT(*) FROM contact_disclosure_record),(SELECT COUNT(*) FROM transfer_receipt_version)").fetch_one(&mut *connection).await?;
    let mut organs = BTreeSet::new();
    for artifact in artifacts {
        match artifact {
            Artifact::Contact(document) => {
                if let Identity::Organ(organ) = &document.statement.issuer {
                    organs.insert(organ.clone());
                }
            }
            Artifact::Claim(document) => {
                if let Identity::Organ(organ) = &document.statement.issuer {
                    organs.insert(organ.clone());
                }
                if let nucleus::trust_analysis::claims::Authorship::Generated { permit, .. } =
                    &document.statement.authorship
                {
                    organs.insert(permit.terms.delegate.organ.clone());
                }
            }
            Artifact::Receipt(document) => {
                if let Some(bundle) = document.public_bundle() {
                    organs.extend(
                        bundle
                            .authorities
                            .iter()
                            .map(|authority| authority.organ.clone()),
                    );
                    let scope = &bundle.publication.scope;
                    organs.extend([
                        scope.source.origin.clone(),
                        scope.giver.clone(),
                        scope.receiver.clone(),
                    ]);
                }
            }
        }
    }
    let mut states = Vec::new();
    for organ in organs {
        let root = store::identity_roots::state_on(connection, &organ).await?;
        let roster =
            store::sqlx::query_as("SELECT payload,signature FROM organ_roster WHERE organ_uid=?")
                .bind(&organ)
                .fetch_optional(&mut *connection)
                .await?;
        states.push((organ, root, roster));
    }
    Ok(AuthorityStamp {
        counts,
        organs: states,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nucleus::trust_analysis::{
        claims,
        query::{CounterpartyBasis, EvidenceLevel},
    };

    #[tokio::test]
    async fn authority_stamp_observes_delegate_roster_changes_for_person_generated_claims() {
        let engine = Engine::open_memory().await.unwrap();
        let organ = store::organs::ensure_local(&engine.store.pool, "http://stamp.test")
            .await
            .unwrap()
            .uid;
        let cell = store::cells::local(&engine.store.pool)
            .await
            .unwrap()
            .unwrap()
            .uid;
        let root = Signer::generate(&organ, crate::roster::ROOT_KEY_ID);
        engine.publish_root_key(&root).await.unwrap();
        let publisher = Signer::generate(&organ, &crate::roster::cell_key_id(&cell));
        engine
            .publish_roster(
                &root,
                vec![crate::roster::CellEntry {
                    cell_uid: cell.clone(),
                    node_id: iroh::SecretKey::from_bytes(&[149; 32]).public().to_string(),
                    label: "Publisher".into(),
                    operational_key: publisher.public_key_b64(),
                    sealing_key: None,
                    front_door: false,
                    capabilities: crate::roster::full_capabilities(),
                }],
            )
            .await
            .unwrap();
        let issuer = Identity::Person(nucleus::new_uid("r"));
        let person = Signer::generate(issuer.uid(), "person:test");
        let descriptor = claims::Descriptor {
            subject: issuer.clone(),
            role: nucleus::transfer::outcome::Role::Giver,
            measurement: nucleus::transfer::receipt::Measurement {
                meaning: nucleus::vocabulary::reference::MeaningReference {
                    vocabulary_uid: nucleus::new_uid("g"),
                    concept_uid: nucleus::new_uid("c"),
                    revision: 1,
                    definition_hash: "a".repeat(64),
                },
                unit: None,
                object_commitment: None,
            },
            level: EvidenceLevel::NeedLinkedReceiver,
            metric: claims::Metric::Quantity,
            counterparty_basis: CounterpartyBasis::Organ,
            from: None,
            until: None,
        };
        let hash = nucleus::karma::canonical_hash("lince.test.recipe.v1", &"recipe").unwrap();
        let mut permit = claims::recipes::Permit {
            recipe: hash.clone(),
            revision: 1,
            terms: claims::recipes::Terms {
                issuer: issuer.clone(),
                claim: nucleus::new_uid("r"),
                title: "Generated history".into(),
                descriptor: descriptor.clone(),
                comparator: claims::Comparator::AtLeast,
                audience: nucleus::trust_analysis::contacts::Audience::Public,
                issued_at: 1000,
                expires_at: 2000,
                lifetime_seconds: 1000,
                delegate: claims::recipes::Delegate {
                    organ: organ.clone(),
                    cell,
                    key_id: publisher.key_id.clone(),
                    public_key: publisher.public_key_b64(),
                },
            },
            key_id: person.key_id.clone(),
            public_key: person.public_key_b64(),
            signature: String::new(),
        };
        crate::trust_analysis::sign_recipe_permit(&mut permit, &person).unwrap();
        let mut document = claims::Document {
            statement: claims::Statement {
                title: permit.terms.title.clone(),
                issuer,
                claim: permit.terms.claim.clone(),
                revision: 1,
                descriptor,
                comparator: claims::Comparator::AtLeast,
                amount: nucleus::DecimalValue::parse_inferred("0").unwrap(),
                as_of: 1000,
                issued_at: 1000,
                expires_at: 2000,
                withdrawn: false,
                audience: permit.terms.audience.clone(),
                authorship: claims::Authorship::Generated {
                    recipe: hash,
                    revision: 1,
                    partial: false,
                    state: claims::recipes::EvaluationState::Available,
                    permit: Box::new(permit),
                },
            },
            key_id: publisher.key_id.clone(),
            public_key: publisher.public_key_b64(),
            signature: String::new(),
        };
        document.signature = publisher.sign_bytes(&document.signing_bytes().unwrap());
        document.validate().unwrap();
        let artifact = Artifact::Claim(document);
        let before = authority_stamp(&mut *engine.store.pool.acquire().await.unwrap(), &artifact)
            .await
            .unwrap();
        engine.publish_roster(&root, Vec::new()).await.unwrap();
        let after = authority_stamp(&mut *engine.store.pool.acquire().await.unwrap(), &artifact)
            .await
            .unwrap();
        assert_eq!(before.counts, after.counts);
        assert!(before != after);
    }
}

#[derive(PartialEq, Eq)]
pub(super) struct PeerAuthority {
    pub(super) payload: String,
    pub(super) signature: String,
    pub(super) anchor: nucleus::identity::roots::State,
    pub(super) revocations: i64,
}

struct ServingReader<'a> {
    endpoint: &'a str,
    organ: &'a str,
    authority: &'a PeerAuthority,
    epoch: u64,
}

async fn head(
    connection: &mut store::sqlx::SqliteConnection,
    person: &str,
    source: &Source,
) -> Result<Option<String>, EngineError> {
    Ok(store::sqlx::query_scalar("SELECT record_uid FROM record_evidence_serving WHERE owner_person_uid=? AND source_record_uid=? AND source_kind=? ORDER BY sequence DESC LIMIT 1").bind(person).bind(&source.record).bind(source.kind.as_str()).fetch_optional(&mut *connection).await?)
}

impl Engine {
    pub(crate) async fn require_known_server_endpoint(&self,organ:&str,endpoint:&str)->Result<(),EngineError> {
        if !nucleus::valid_uid(organ,"r") {return Err(invalid("Choose a recognized Organ identity for the helper"));}
        self.serving_peer_authority(organ,endpoint).await.map(|_|())
    }

    pub(super) async fn serving_peer_authority(
        &self,
        organ: &str,
        endpoint: &str,
    ) -> Result<PeerAuthority, EngineError> {
        let mut connection = self.store.pool.acquire().await?;
        let anchor = store::identity_roots::state_on(&mut connection, organ).await?;
        let revocations: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM identity_revocation WHERE organ_uid=?")
                .bind(organ)
                .fetch_one(&mut *connection)
                .await?;
        drop(connection);
        let stored = store::roster::get(&self.store.pool, organ)
            .await?
            .ok_or_else(|| invalid("Refresh the independently accepted reader Organ roster"))?;
        let signed = crate::roster::SignedRoster {
            roster: serde_json::from_str(&stored.payload)?,
            signature: stored.signature.clone(),
        };
        let members = signed
            .roster
            .cells
            .iter()
            .filter(|member| member.node_id == endpoint)
            .collect::<Vec<_>>();
        if signed.roster.organ_uid != organ
            || signed.roster.root_key != stored.root_key
            || signed.roster.version != stored.version
            || !crate::roster::roster_signature_is_valid(&signed)
            || !self.key_chains(organ, &signed.roster.root_key).await?
            || members.len() != 1
            || !members[0].may(crate::roster::CAP_REPRESENT)
            || store::roster::is_revoked(&self.store.pool, organ, &members[0].operational_key)
                .await?
        {
            return Err(invalid(
                "The query endpoint lacks current representative authority for its Organ",
            ));
        }
        Ok(PeerAuthority {
            payload: stored.payload,
            signature: stored.signature,
            anchor,
            revocations,
        })
    }

    pub(super) async fn serving_snapshot(
        &self,
        person: &str,
        request: &model::Request,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Snapshot, EngineError> {
        self.require_permission(actor, "record:read").await?;
        let mut tx = self.store.pool.begin().await?;
        let source = &request.source;
        let reader = self
            .contact_reader_on(&mut tx, person, actor, request.as_organ)
            .await?;
        self.contact_record_readable_on(&mut tx, &source.record, actor)
            .await?;
        let revision = store::record_revisions::get_on(&mut tx, &source.record)
            .await?
            .revision;
        let (artifact, version) = if request.scope == Scope::Recipe {
            self.serving_recipe_on(&mut tx, person, source, actor, now.timestamp())
                .await?
        } else {
            match source.kind {
                SourceKind::Contact => {
                    let saved = store::contact_disclosures::get_on(&mut tx, &source.record).await?;
                    let statement = &saved.document.statement;
                    let latest =
                        store::contact_disclosures::latest_revision_on(&mut tx, person, statement)
                            .await?;
                    let forks:i64=store::sqlx::query_scalar("SELECT COUNT(DISTINCT statement_hash) FROM contact_disclosure_record WHERE owner_person_uid=? AND relation_hash=? AND issuer_kind=? AND issuer_uid=? AND revision=?").bind(person).bind(statement.relation().map_err(invalid)?.as_str()).bind(statement.issuer.kind()).bind(statement.issuer.uid()).bind(latest as i64).fetch_one(&mut *tx).await?;
                    if saved.owner != person
                        || latest != statement.revision
                        || forks != 1
                        || self
                            .authenticate_contact_disclosure_on(
                                &mut tx,
                                &saved.document,
                                now.timestamp(),
                            )
                            .await?
                            != IdentityAuthenticity::Accepted
                    {
                        return Err(invalid(
                            "Serve your own retained current contact evidence with accepted authority",
                        ));
                    }
                    self.contact_profiles_on(&mut tx, &saved.document, actor)
                        .await?;
                    let version = statement.revision;
                    (Artifact::Contact(saved.document), version)
                }
                SourceKind::Claim => {
                    let saved = store::profile_claims::get_on(&mut tx, &source.record).await?;
                    if saved.owner != person
                        || !store::profile_claims::current_on(
                            &mut tx,
                            person,
                            &saved.document.statement,
                        )
                        .await?
                        || self
                            .authenticate_profile_claim_on(
                                &mut tx,
                                &saved.document,
                                now.timestamp(),
                            )
                            .await?
                            != IdentityAuthenticity::Accepted
                    {
                        return Err(invalid(
                            "Serve your own retained current claim with accepted authority",
                        ));
                    }
                    self.trust_profiles_on(
                        &mut tx,
                        vec![
                            saved.document.statement.issuer.clone(),
                            saved.document.statement.descriptor.subject.clone(),
                        ],
                        actor,
                    )
                    .await?;
                    let version = saved.document.statement.revision;
                    (Artifact::Claim(saved.document), version)
                }
                SourceKind::Receipt => {
                    let saved =
                        store::transfer_receipts::get_on(&mut tx, &source.record, None).await?;
                    if saved.owner != person {
                        return Err(invalid("Serve your own retained evidence copy"));
                    }
                    let bundle=saved.document.public_bundle().ok_or_else(||invalid("Private reviews and signing authority are not transferable outcome evidence"))?;
                    let scope = &bundle.publication.scope;
                    let profiles = [&scope.source.origin, &scope.giver, &scope.receiver]
                        .into_iter()
                        .map(|uid| Identity::Organ(uid.clone()))
                        .chain(
                            scope
                                .public_persons
                                .iter()
                                .map(|person| Identity::Person(person.person.clone())),
                        )
                        .collect();
                    self.trust_profiles_on(&mut tx, profiles, actor).await?;
                    (Artifact::Receipt(Box::new(saved.document)), saved.version)
                }
            }
        };
        artifact.validate(now.timestamp()).map_err(invalid)?;
        if !artifact.allows_reader(&reader) {
            return Err(invalid(
                "The original evidence audience does not admit this source reader",
            ));
        }
        let authority = authority_stamp(&mut tx, &artifact).await?;
        drop(tx);
        if let Artifact::Receipt(document) = &artifact {
            self.verify_transfer_receipt_document(document, now).await?;
        }
        Ok(Snapshot {
            artifact,
            version,
            revision,
            authority,
        })
    }

    pub(super) async fn evidence_serving_command(
        &self,
        person: &str,
        request_id: &str,
        command: &Command,
        actor: Option<&str>,
        now: DateTime<Utc>,
        authorship: Option<&VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        if let Command::Preview { request } = command {
            request.validate().map_err(invalid)?;
            let snapshot = self.serving_snapshot(person, request, actor, now).await?;
            let previous = head(
                &mut *self.store.pool.acquire().await?,
                person,
                &request.source,
            )
            .await?;
            let mut review = Review {
                person: person.into(),
                previous,
                request: request.clone(),
                artifact: snapshot.artifact,
                source_version: snapshot.version,
                source_revision: snapshot.revision,
                reviewed_at: now.timestamp(),
                expires_at: 0,
            };
            review.expires_at = review.term_boundary().map_err(invalid)?.min(
                now.timestamp()
                    .saturating_add(i64::from(request.lifetime_seconds)),
            );
            return Ok(ActionOutcome {
                data: Some(
                    json!({"trust":{"kind":"serving_preview","review":review,"hash":review.hash().map_err(invalid)?,"artifact_hash":review.artifact.hash().map_err(invalid)?,"bytes":review.artifact.bytes().map_err(invalid)?.len()}}),
                ),
                ..Default::default()
            });
        }
        if let Command::Publish {
            review,
            expected_review,
        } = command
        {
            let request = serde_json::to_value(command)?;
            let mut tx = self.store.pool.begin().await?;
            if let Some(saved) =
                store::evidence_serving::replay_on(&mut tx, person, request_id, &request).await?
            {
                self.contact_record_readable_on(&mut tx, &saved.record, actor)
                    .await?;
                return Ok(ActionOutcome {
                    created: Some(saved.record.clone()),
                    data: Some(
                        json!({"trust":{"kind":"serving_permission","record":saved.record,"scope":saved.review.request.scope,"artifact_hash":saved.review.artifact.hash().map_err(invalid)?,"expires_at":saved.review.expires_at,"current":saved.current && saved.active,"replayed":true}}),
                    ),
                    ..Default::default()
                });
            }
            drop(tx);
            let changed = self.watch_query_changes();
            let epoch = *changed.borrow();
            let snapshot = self
                .serving_snapshot(person, &review.request, actor, now)
                .await?;
            let signer = self.transfer_person_signer(person, authorship).await?;
            let mut tx = store::write_tx(&self.store.pool).await?;
            self.require_permission_on(&mut tx, actor, "record:create")
                .await?;
            self.require_permission_on(&mut tx, actor, "record:read")
                .await?;
            if let Some(saved) =
                store::evidence_serving::replay_on(&mut tx, person, request_id, &request).await?
            {
                self.contact_record_readable_on(&mut tx, &saved.record, actor)
                    .await?;
                return Ok(ActionOutcome {
                    created: Some(saved.record.clone()),
                    data: Some(
                        json!({"trust":{"kind":"serving_permission","record":saved.record,"scope":saved.review.request.scope,"artifact_hash":saved.review.artifact.hash().map_err(invalid)?,"expires_at":saved.review.expires_at,"current":saved.current && saved.active,"replayed":true}}),
                    ),
                    ..Default::default()
                });
            }
            self.contact_record_readable_on(&mut tx, &review.request.source.record, actor)
                .await?;
            let reader = self
                .contact_reader_on(&mut tx, person, actor, review.request.as_organ)
                .await?;
            if !snapshot.artifact.allows_reader(&reader) {
                return Err(invalid(
                    "The original evidence audience does not admit this source reader",
                ));
            }
            let current_revision =
                store::record_revisions::get_on(&mut tx, &review.request.source.record)
                    .await?
                    .revision;
            let current_time = nucleus::execution::now().timestamp();
            self.recheck_serving_recipe_on(&mut tx, review, &snapshot, actor, current_time)
                .await?;
            if review.person != person
                || review.reviewed_at > current_time
                || current_time.saturating_sub(review.reviewed_at) > 300
                || review.expires_at <= current_time
                || review.hash().map_err(invalid)? != *expected_review
                || review.artifact != snapshot.artifact
                || review.source_version != snapshot.version
                || review.source_revision != snapshot.revision
                || current_revision != snapshot.revision
                || review.previous != head(&mut tx, person, &review.request.source).await?
                || *changed.borrow() != epoch
                || authority_stamp(&mut tx, &review.artifact).await? != snapshot.authority
            {
                return Err(invalid(
                    "The source, evidence, audience, prior permission or review changed; preview again",
                ));
            }
            let record = nucleus::new_uid("r");
            let checkpoint = self
                .record_checkpoint_on(
                    &mut tx,
                    actor,
                    vec![record.clone()],
                    protein::authority::Operation::Create,
                )
                .await?;
            let (saved, mut tx) = store::evidence_serving::save(
                tx,
                store::evidence_serving::Input {
                    record,
                    request_id: request_id.into(),
                    request,
                    review: review.as_ref().clone(),
                    authorization_intent: authorship.map(|proof| proof.intent_uid.clone()),
                },
                now,
                |hash| signer.as_ref().map(|signer| signer.sign_hash(hash)),
            )
            .await?;
            if let Some(checkpoint) = checkpoint {
                checkpoint
                    .bounded()
                    .finish(&mut tx, Default::default())
                    .await?;
            }
            tx.commit().await?;
            return Ok(ActionOutcome {
                created: Some(saved.record.clone()),
                facts: self.publish_committed_fact(saved.fact),
                data: Some(
                    json!({"trust":{"kind":"serving_permission","record":saved.record,"scope":saved.review.request.scope,"artifact_hash":saved.review.artifact.hash().map_err(invalid)?,"expires_at":saved.review.expires_at,"current":true,"replayed":false}}),
                ),
                ..Default::default()
            });
        }
        let mut tx = self.store.pool.begin().await?;
        if let Command::Inspect { record } = command {
            self.contact_record_readable_on(&mut tx, record, actor)
                .await?;
            let saved = store::evidence_serving::get_on(&mut tx, record).await?;
            if saved.owner != person {
                return Err(invalid("Inspect your own private serving approval"));
            }
            let reader = self
                .contact_reader_on(&mut tx, person, actor, saved.review.request.as_organ)
                .await?;
            if !saved.review.artifact.allows_reader(&reader) {
                return Err(invalid(
                    "The original evidence audience does not admit this source reader",
                ));
            }
            return Ok(ActionOutcome {
                data: Some(
                    json!({"trust":{"kind":"serving_inspection","record":saved.record,"scope":saved.review.request.scope,"review":saved.review,"artifact_hash":saved.review.artifact.hash().map_err(invalid)?,"expires_at":saved.review.expires_at,"current":saved.active && saved.current}}),
                ),
                ..Default::default()
            });
        }
        if let Command::List { before } = command {
            let principal = serde_json::to_string(&(person, actor))?;
            let before: Option<String> = match before {
                Some(before) => Some(
                    self.decode_read_cursor(
                        "evidence-serving",
                        &principal,
                        before,
                        now.timestamp(),
                    )
                    .await?,
                ),
                None => None,
            };
            let rows:Vec<String>=store::sqlx::query_scalar("SELECT a.record_uid FROM record_evidence_serving a JOIN record r ON r.uid=a.record_uid WHERE a.owner_person_uid=? AND r.deleted_at IS NULL AND (? IS NULL OR a.record_uid<?) ORDER BY a.record_uid DESC LIMIT 65").bind(person).bind(&before).bind(&before).fetch_all(&mut *tx).await?;
            let more = rows.len() > 64;
            let last = rows.iter().take(64).last().cloned();
            let mut entries = Vec::new();
            for record in rows.into_iter().take(64) {
                if self
                    .contact_record_readable_on(&mut tx, &record, actor)
                    .await
                    .is_err()
                {
                    continue;
                }
                let saved = store::evidence_serving::get_on(&mut tx, &record).await?;
                entries.push(json!({"record":saved.record,"source":saved.review.request.source,"scope":saved.review.request.scope,"artifact_hash":saved.review.artifact.hash().map_err(invalid)?,"expires_at":saved.review.expires_at,"current":saved.active && saved.current}));
            }
            let next = if more {
                match last {
                    Some(last) => Some(
                        self.encode_read_cursor(
                            "evidence-serving",
                            &principal,
                            &last,
                            now.timestamp(),
                        )
                        .await?,
                    ),
                    None => None,
                }
            } else {
                None
            };
            return Ok(ActionOutcome {
                data: Some(json!({"trust":{"kind":"serving_list","entries":entries,"next":next}})),
                ..Default::default()
            });
        }
        Err(invalid("Choose an evidence serving operation"))
    }

    pub(crate) async fn fetch_served_evidence(
        &self,
        source: &str,
        node: &str,
        hash: &nucleus::karma::CanonicalHash,
        offset: u32,
        now: i64,
    ) -> Result<Value, EngineError> {
        tokio::time::timeout(
            std::time::Duration::from_secs(3),
            self.fetch_served_evidence_inner(source, node, hash, offset, now),
        )
        .await
        .map_err(|_| invalid("Evidence serving work limit exceeded"))?
    }

    async fn fetch_served_evidence_inner(
        &self,
        source: &str,
        node: &str,
        hash: &nucleus::karma::CanonicalHash,
        offset: u32,
        now: i64,
    ) -> Result<Value, EngineError> {
        use base64::{Engine as _, engine::general_purpose::STANDARD};
        if offset as usize % model::CHUNK_BYTES != 0 || offset as usize >= model::MAX_ARTIFACT_BYTES
        {
            return Err(invalid("Choose a bounded evidence chunk offset"));
        }
        let peer = self.social_ask_peer(source, false).await?;
        let _slot = self
            .evidence_serving_slots
            .try_acquire()
            .map_err(|_| invalid("Evidence serving is at its concurrent work limit"))?;
        self.serving_work_budget(&peer.organ).await?;
        let reader_authority = self.serving_peer_authority(&peer.organ, source).await?;
        let changed = self.watch_query_changes();
        let epoch = *changed.borrow();
        let candidates:Vec<String>=store::sqlx::query_scalar("SELECT a.record_uid FROM record_evidence_serving a JOIN record r ON r.uid=a.record_uid WHERE (a.artifact_hash=? OR json_extract(a.review_json,'$.request.scope')='recipe') AND a.expires_at>? AND r.deleted_at IS NULL AND NOT EXISTS(SELECT 1 FROM record_evidence_serving n WHERE n.owner_person_uid=a.owner_person_uid AND n.source_record_uid=a.source_record_uid AND n.source_kind=a.source_kind AND n.sequence>a.sequence) ORDER BY a.sequence DESC LIMIT ?").bind(hash.as_str()).bind(now).bind(model::MAX_PERMISSIONS as i64).fetch_all(&self.store.pool).await?;
        for record in candidates {
            let mut tx = self.store.pool.begin().await?;
            let saved = store::evidence_serving::get_on(&mut tx, &record).await?;
            if !saved.current
                || !saved.active
                || !store::people::is_active_on(&mut tx, &saved.owner).await?
            {
                continue;
            }
            let context = self
                .contact_identity_context_on(
                    &mut tx,
                    &Identity::Organ(peer.organ.clone()),
                    Some(&saved.owner),
                )
                .await?;
            if !context.readable
                || context.blocked
                || saved.review.request.audience.include.is_empty()
                || !saved
                    .review
                    .request
                    .audience
                    .allows(&Identity::Organ(peer.organ.clone()), &context)
                || !saved.review.artifact.allows(&peer.organ)
                || self
                    .contact_record_readable_on(&mut tx, &record, Some(&saved.owner))
                    .await
                    .is_err()
            {
                continue;
            }
            drop(tx);
            let current_time = nucleus::execution::now();
            let snapshot = match self
                .serving_snapshot(
                    &saved.owner,
                    &saved.review.request,
                    Some(&saved.owner),
                    current_time,
                )
                .await
            {
                Ok(snapshot) => snapshot,
                Err(_) => continue,
            };
            if !saved
                .review
                .allows_current(&snapshot.artifact, snapshot.version, snapshot.revision)
                || snapshot.artifact.hash().map_err(invalid)? != *hash
                || saved.review.expires_at <= current_time.timestamp()
            {
                continue;
            }
            let bytes = snapshot.artifact.bytes().map_err(invalid)?;
            if offset as usize >= bytes.len() {
                return Err(invalid("Evidence chunk is unavailable"));
            }
            let end = (offset as usize + model::CHUNK_BYTES).min(bytes.len());
            let chunk = model::Chunk {
                hash: hash.clone(),
                offset,
                total: bytes.len() as u32,
                bytes: STANDARD.encode(&bytes[offset as usize..end]),
            };
            let mut tx = self.store.pool.begin().await?;
            self.validate_served_evidence_on(
                &mut tx,
                &saved,
                &snapshot,
                &ServingReader {
                    endpoint: source,
                    organ: &peer.organ,
                    authority: &reader_authority,
                    epoch,
                },
            )
            .await?;
            return Ok(json!({"service":node,"chunk":chunk}));
        }
        self.fetch_custody_evidence(source,node,hash,offset,now).await
    }

    async fn serving_work_budget(&self, peer: &str) -> Result<(), EngineError> {
        let local = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("Evidence serving requires a local Organ"))?;
        for organ in [local.uid.as_str(), peer] {
            let kind = store::contact_rate::RateKind::EvidenceServe;
            if store::contact_rate::backing_off(&self.store.pool, organ, kind)
                .await?
                .is_some()
                || store::contact_rate::spend(&self.store.pool, organ, kind)
                    .await?
                    .is_some()
            {
                return Err(invalid("Evidence serving request work budget is exhausted"));
            }
        }
        Ok(())
    }

    async fn validate_served_evidence_on(
        &self,
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        saved: &store::evidence_serving::Saved,
        snapshot: &Snapshot,
        reader: &ServingReader<'_>,
    ) -> Result<(), EngineError> {
        self.social_ask_peer_on(tx, reader.endpoint, reader.organ, false)
            .await?;
        let roster: Option<(String, String)> =
            store::sqlx::query_as("SELECT payload,signature FROM organ_roster WHERE organ_uid=?")
                .bind(reader.organ)
                .fetch_optional(&mut **tx)
                .await?;
        let revocations: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM identity_revocation WHERE organ_uid=?")
                .bind(reader.organ)
                .fetch_one(&mut **tx)
                .await?;
        let current_anchor = store::identity_roots::state_on(tx, reader.organ).await?;
        let roster_valid = if let Some((payload, signature)) = &roster {
            let signed = crate::roster::SignedRoster {
                roster: serde_json::from_str(payload)?,
                signature: signature.clone(),
            };
            payload == &reader.authority.payload
                && signature == &reader.authority.signature
                && crate::roster::roster_signature_is_valid(&signed)
        } else {
            false
        };
        if !roster_valid
            || revocations != reader.authority.revocations
            || current_anchor != reader.authority.anchor
        {
            return Err(invalid(
                "Reader Organ authority changed while preparing evidence",
            ));
        }
        self.require_permission_on(tx, Some(&saved.owner), "record:read")
            .await?;
        let principal = self
            .contact_reader_on(
                tx,
                &saved.owner,
                Some(&saved.owner),
                saved.review.request.as_organ,
            )
            .await?;
        if !snapshot.artifact.allows_reader(&principal) || !snapshot.artifact.allows(reader.organ) {
            return Err(invalid(
                "The original evidence audience does not admit this source reader",
            ));
        }
        self.contact_record_readable_on(tx, &saved.record, Some(&saved.owner))
            .await?;
        self.contact_record_readable_on(
            tx,
            &saved.review.request.source.record,
            Some(&saved.owner),
        )
        .await?;
        let context = self
            .contact_identity_context_on(
                tx,
                &Identity::Organ(reader.organ.into()),
                Some(&saved.owner),
            )
            .await?;
        if !context.readable
            || context.blocked
            || saved.review.request.audience.include.is_empty()
            || !saved
                .review
                .request
                .audience
                .allows(&Identity::Organ(reader.organ.into()), &context)
            || !store::people::is_active_on(tx, &saved.owner).await?
        {
            return Err(invalid("Current serving audience or controller changed"));
        }
        let final_state = store::evidence_serving::get_on(tx, &saved.record).await?;
        let source_revision =
            store::record_revisions::get_on(tx, &saved.review.request.source.record)
                .await?
                .revision;
        match &snapshot.artifact {
            Artifact::Contact(document) => {
                self.contact_profiles_on(tx, document, Some(&saved.owner))
                    .await?;
            }
            Artifact::Claim(document) => {
                self.trust_profiles_on(
                    tx,
                    vec![
                        document.statement.issuer.clone(),
                        document.statement.descriptor.subject.clone(),
                    ],
                    Some(&saved.owner),
                )
                .await?;
            }
            Artifact::Receipt(document) => {
                let scope = &document
                    .public_bundle()
                    .ok_or_else(|| invalid("Choose outcome evidence"))?
                    .publication
                    .scope;
                self.trust_profiles_on(
                    tx,
                    [&scope.source.origin, &scope.giver, &scope.receiver]
                        .into_iter()
                        .map(|uid| Identity::Organ(uid.clone()))
                        .chain(
                            scope
                                .public_persons
                                .iter()
                                .map(|person| Identity::Person(person.person.clone())),
                        )
                        .collect(),
                    Some(&saved.owner),
                )
                .await?;
            }
        }
        self.recheck_serving_recipe_on(
            tx,
            &saved.review,
            snapshot,
            Some(&saved.owner),
            nucleus::execution::now().timestamp(),
        )
        .await?;
        if !final_state.current
            || !final_state.active
            || *self.watch_query_changes().borrow() != reader.epoch
            || saved.review.expires_at <= nucleus::execution::now().timestamp()
            || snapshot.artifact.expires_at().map_err(invalid)?
                <= nucleus::execution::now().timestamp()
            || authority_stamp(tx, &snapshot.artifact).await? != snapshot.authority
            || source_revision != snapshot.revision
            || !saved
                .review
                .allows_current(&snapshot.artifact, snapshot.version, snapshot.revision)
        {
            return Err(invalid(
                "Evidence serving permissions changed; retry deliberately",
            ));
        }
        Ok(())
    }
}
