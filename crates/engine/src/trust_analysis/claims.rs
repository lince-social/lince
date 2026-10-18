use super::*;
use crate::actions::{ActionOutcome, VerifiedActionAuthorship};
use chrono::{DateTime, Utc};
use nucleus::trust_analysis::claims::{
    self, Authorship, Document as Claim, Statement as ClaimStatement,
    commands::{Command as Request, Review},
};
use serde_json::json;

pub(super) fn claim_invalid(message: impl Into<String>) -> EngineError {
    EngineError::Conflict {
        code: "profile_claim_invalid",
        message: message.into(),
    }
}

pub(super) fn prepare_claim_action(
    person: Option<&str>,
    request: &mut Request,
    signer: &Signer,
) -> Result<(), EngineError> {
    match request {
        Request::Recipes { request } => {
            return super::summaries::prepare_recipe_action(person, request, signer);
        }
        Request::Preview { draft } if matches!(draft.issuer, Identity::Person(_)) => {
            if draft.issuer.uid() != signer.actor_uid
                || person.is_some_and(|person| person != signer.actor_uid)
            {
                return Err(EngineError::Forbidden(
                    "Use your own Person claim-signing identity".into(),
                ));
            }
            draft.key_id = signer.key_id.clone();
            draft.public_key = signer.public_key_b64();
        }
        Request::Publish { review, .. }
            if matches!(review.document.statement.issuer, Identity::Person(_)) =>
        {
            if review.person != signer.actor_uid
                || person.is_some_and(|person| person != signer.actor_uid)
            {
                return Err(EngineError::Forbidden(
                    "Sign a claim only under your own Person identity".into(),
                ));
            }
            sign_claim_document(&mut review.document, signer)?;
        }
        _ => {}
    }
    Ok(())
}

pub fn sign_claim_document(document: &mut Claim, signer: &Signer) -> Result<(), EngineError> {
    if document.statement.issuer.uid() != signer.actor_uid
        || (!document.key_id.is_empty() && document.key_id != signer.key_id)
        || (!document.public_key.is_empty() && document.public_key != signer.public_key_b64())
        || (matches!(document.statement.issuer, Identity::Organ(_))
            && !crate::roster::is_root_key_id(&signer.key_id))
    {
        return Err(EngineError::Forbidden(
            "Use the declared Person key or this Organ's owner authority".into(),
        ));
    }
    if document.signature.is_empty() {
        document.key_id = signer.key_id.clone();
        document.public_key = signer.public_key_b64();
        document.signature = signer.sign_bytes(&document.signing_bytes().map_err(claim_invalid)?);
    }
    Ok(())
}

impl Engine {
    pub async fn authenticate_profile_claim(
        &self,
        document: &Claim,
        now: i64,
    ) -> Result<IdentityAuthenticity, EngineError> {
        self.authenticate_profile_claim_on(&mut *self.store.pool.acquire().await?, document, now)
            .await
    }

    pub(super) async fn authenticate_profile_claim_on(
        &self,
        connection: &mut store::sqlx::SqliteConnection,
        document: &Claim,
        now: i64,
    ) -> Result<IdentityAuthenticity, EngineError> {
        document.validate().map_err(claim_invalid)?;
        if document.statement.issued_at > now {
            return Err(claim_invalid("This claim is not effective yet"));
        }
        if matches!(document.statement.authorship, Authorship::Generated { .. }) {
            return self
                .authenticate_generated_claim_on(connection, document, now)
                .await;
        }
        let payload = document.signing_bytes().map_err(claim_invalid)?;
        self.authenticate_identity_signature_on(
            connection,
            SignedIdentity {
                issuer: &document.statement.issuer,
                key_id: &document.key_id,
                public_key: &document.public_key,
                signature: &document.signature,
                payload: &payload,
            },
        )
        .await
        .map_err(|error| match error {
            EngineError::Conflict {
                code: "contact_disclosure_unknown_identity",
                message,
            } => EngineError::Conflict {
                code: "profile_claim_unknown_identity",
                message,
            },
            EngineError::Conflict { message, .. } => claim_invalid(message),
            other => other,
        })
    }

    async fn permitted_claim_on(
        &self,
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        person: &str,
        actor: Option<&str>,
        record: &str,
        as_organ: bool,
        now: DateTime<Utc>,
    ) -> Result<(store::profile_claims::Saved, IdentityAuthenticity), EngineError> {
        let reader = self.contact_reader_on(tx, person, actor, as_organ).await?;
        self.contact_record_readable_on(tx, record, actor).await?;
        let saved = store::profile_claims::get_on(tx, record).await?;
        self.trust_profiles_on(
            tx,
            vec![
                saved.document.statement.issuer.clone(),
                saved.document.statement.descriptor.subject.clone(),
            ],
            actor,
        )
        .await?;
        if !saved
            .document
            .statement
            .audience
            .allows(&reader, &saved.document.statement.issuer)
        {
            return Err(EngineError::Forbidden(
                "The claim does not grant this reader access".into(),
            ));
        }
        let authority = self
            .authenticate_profile_claim_on(tx, &saved.document, now.timestamp())
            .await?;
        Ok((saved, authority))
    }

    pub(super) async fn profile_claim_command(
        &self,
        person: &str,
        id: &str,
        request: &Request,
        actor: Option<&str>,
        now: DateTime<Utc>,
        authorship: Option<&VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        match request {
            Request::InspectDocument { document, as_organ } => {
                return self
                    .profile_claim_snapshot(person, actor, document, None, *as_organ, now)
                    .await;
            }
            Request::CheckDocument {
                document,
                sources,
                as_organ,
            } => {
                return self
                    .profile_claim_snapshot(person, actor, document, Some(sources), *as_organ, now)
                    .await;
            }
            Request::Recipes { request } => {
                return self
                    .claim_recipe_command(person, id, request, actor, now, authorship)
                    .await;
            }
            Request::Preview { draft } => {
                if draft.amount.len() > 128
                    || draft.lifetime_seconds == 0
                    || i64::from(draft.lifetime_seconds)
                        > nucleus::trust_analysis::contacts::MAX_TERM_SECONDS
                {
                    return Err(claim_invalid(
                        "Choose a bounded amount and a claim term of 1–366 days",
                    ));
                }
                let mut document = Claim {
                    statement: ClaimStatement {
                        title: draft.title.clone(),
                        issuer: draft.issuer.clone(),
                        claim: draft.claim.clone().unwrap_or_else(|| nucleus::new_uid("r")),
                        revision: draft
                            .expected_revision
                            .checked_add(1)
                            .ok_or_else(|| claim_invalid("Claim revision overflow"))?,
                        descriptor: draft.descriptor.clone(),
                        comparator: draft.comparator,
                        amount: nucleus::DecimalValue::parse_inferred(&draft.amount)
                            .map_err(|error| claim_invalid(error.to_string()))?,
                        as_of: draft.as_of.unwrap_or(now.timestamp()),
                        issued_at: now.timestamp(),
                        expires_at: now
                            .timestamp()
                            .saturating_add(i64::from(draft.lifetime_seconds)),
                        withdrawn: draft.withdrawn,
                        audience: draft.audience.clone(),
                        authorship: Authorship::Manual,
                    },
                    key_id: draft.key_id.clone(),
                    public_key: draft.public_key.clone(),
                    signature: String::new(),
                };
                if matches!(draft.issuer, Identity::Organ(_)) {
                    self.require_permission(actor, "organ:update").await?;
                    let root = self.root_signer().await?.ok_or_else(|| {
                        claim_invalid("Use the Cell holding this Organ's owner key")
                    })?;
                    if root.actor_uid != draft.issuer.uid() {
                        return Err(EngineError::Forbidden(
                            "Publish under this Organ's owner authority".into(),
                        ));
                    }
                    document.key_id = root.key_id.clone();
                    document.public_key = root.public_key_b64();
                } else if document.key_id.is_empty() || document.public_key.is_empty() {
                    let signer = self
                        .transfer_person_signer(person, authorship)
                        .await?
                        .ok_or_else(|| {
                            claim_invalid("Choose the Person signing identity on your device")
                        })?;
                    document.key_id = signer.key_id.clone();
                    document.public_key = signer.public_key_b64();
                }
                document.signing_bytes().map_err(claim_invalid)?;
                let mut tx = self.store.pool.begin().await?;
                self.contact_issuer_on(&mut tx, &draft.issuer, person, actor)
                    .await?;
                if !Self::identity_key_authority_on(
                    &mut tx,
                    &draft.issuer,
                    &document.key_id,
                    &document.public_key,
                )
                .await?
                .1
                {
                    return Err(claim_invalid(
                        "Independently recognize a current claim-signing identity first",
                    ));
                }
                let profiles = self
                    .trust_profiles_on(
                        &mut tx,
                        vec![
                            document.statement.issuer.clone(),
                            document.statement.descriptor.subject.clone(),
                        ],
                        actor,
                    )
                    .await?;
                if store::profile_claims::latest_on(&mut tx, person, &document.statement).await?
                    != draft.expected_revision
                {
                    return Err(claim_invalid(
                        "This claim changed; review its latest revision",
                    ));
                }
                let review = Review {
                    person: person.into(),
                    document,
                    expected_revision: draft.expected_revision,
                    reviewed_at: now.timestamp(),
                    profiles,
                };
                return Ok(ActionOutcome {
                    data: Some(
                        json!({"trust":{"kind":"claim_review","review":review,"hash":review.hash().map_err(claim_invalid)?}}),
                    ),
                    ..Default::default()
                });
            }
            Request::Inspect { record, as_organ }
            | Request::Export { record, as_organ }
            | Request::Check {
                record, as_organ, ..
            } => {
                let record = self.resolve(record).await?;
                let cached=self.local_custody_originals(person,actor,Some(nucleus::trust_analysis::serving::SourceKind::Claim),now.timestamp(),nucleus::trust_analysis::MAX_BYTES).await?;
                let mut tx = self.store.pool.begin().await?;
                let (saved, authority) = self
                    .permitted_claim_on(&mut tx, person, actor, &record, *as_organ, now)
                    .await?;
                let current =
                    store::profile_claims::current_on(&mut tx, person, &saved.document.statement)
                        .await? && self.custody_claim_is_current_on(&mut tx,&saved.document,&cached,actor,now.timestamp()).await?;
                let mut result = json!({"kind":"claim_document","record":saved.record,"document":saved.document,"authority_current":authority==IdentityAuthenticity::Accepted,"current_retained":current,"checked_at":now.timestamp()});
                if matches!(request, Request::Export { .. }) {
                    if saved.owner != person
                        || authority != IdentityAuthenticity::Accepted
                        || !current
                        || saved.document.statement.expires_at <= now.timestamp()
                    {
                        return Err(EngineError::Forbidden(
                            "Export an owned, current, authorized and unexpired claim".into(),
                        ));
                    }
                    result["export"] = json!(serde_json::to_string(&saved.document)?);
                }
                if let Request::Check { sources, .. } = request {
                    if !current
                        || authority != IdentityAuthenticity::Accepted
                        || saved.document.statement.withdrawn
                        || saved.document.statement.expires_at <= now.timestamp()
                    {
                        return Err(claim_invalid(
                            "The current retained claim is withheld, withdrawn, conflicted or expired",
                        ));
                    }
                    drop(tx);
                    let query = saved
                        .document
                        .statement
                        .descriptor
                        .checking_query(sources, saved.document.statement.as_of)
                        .map_err(claim_invalid)?;
                    let evaluated = self
                        .evaluate_trust_query(person, actor, &query, *as_organ, now)
                        .await?;
                    let support =
                        claims::compare(&saved.document.statement, &query, &evaluated.analysis)
                            .map_err(claim_invalid)?;
                    let finished_at = nucleus::execution::now();
                    let mut tx = self.store.pool.begin().await?;
                    let (checked, accepted) = self
                        .permitted_claim_on(&mut tx, person, actor, &record, *as_organ, finished_at)
                        .await?;
                    if checked.document != saved.document
                        || accepted != IdentityAuthenticity::Accepted
                        || !self.custody_claim_is_current_on(&mut tx,&checked.document,&cached,actor,finished_at.timestamp()).await?
                        || checked.document.statement.expires_at <= finished_at.timestamp()
                        || *self.watch_query_changes().borrow() != evaluated.revision
                        || !store::profile_claims::current_on(
                            &mut tx,
                            person,
                            &checked.document.statement,
                        )
                        .await?
                    {
                        return Err(claim_invalid(
                            "The claim or its authority changed while checking; refresh",
                        ));
                    }
                    result["kind"] = json!("claim_check");
                    result["support"] = json!(support);
                    result["query"] = json!(query);
                    result["analysis"] = json!(evaluated.analysis);
                    result["proof_records"] = json!(evaluated.proof_records);
                    result["checked_at"] = json!(finished_at.timestamp());
                    result["meaning"] = json!(
                        "The publisher's statement and the reader's visible support are separate. Missing evidence does not prove missing activity. No remote request was made."
                    );
                }
                let data = json!({"trust":result});
                if serde_json::to_vec(&data)?.len() > nucleus::trust_analysis::MAX_BYTES {
                    return Err(claim_invalid(
                        "The claim-check result exceeded its byte budget",
                    ));
                }
                return Ok(ActionOutcome {
                    data: Some(data),
                    ..Default::default()
                });
            }
            Request::List {
                subject,
                before,
                as_organ,
            } => {
                return self
                    .profile_claim_listing(
                        person,
                        actor,
                        subject,
                        before.as_deref(),
                        *as_organ,
                        now,
                    )
                    .await;
            }
            _ => {}
        }
        let signer = self.transfer_person_signer(person, authorship).await?;
        let root = if matches!(request,Request::Publish {review,..} if matches!(review.document.statement.issuer,Identity::Organ(_)))
        {
            self.require_permission(actor, "organ:update").await?;
            self.root_signer().await?
        } else {
            None
        };
        let expected = if let Request::Publish { review, .. } = request {
            Some(review.expected_revision)
        } else {
            None
        };
        let encoded = serde_json::to_value(request)?;
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_permission_on(&mut tx, actor, "record:create")
            .await?;
        if let Some(saved) =
            store::profile_claims::replay_on(&mut tx, person, id, &encoded, expected).await?
        {
            let as_organ = matches!(request, Request::Import { as_organ: true, .. })
                || matches!(request,Request::Publish {review,..} if matches!(review.document.statement.issuer,Identity::Organ(_)));
            let (saved, authority) = self
                .permitted_claim_on(&mut tx, person, actor, &saved.record, as_organ, now)
                .await?;
            if matches!(request, Request::Publish { .. }) {
                self.contact_issuer_on(&mut tx, &saved.document.statement.issuer, person, actor)
                    .await?;
            }
            return Ok(ActionOutcome {
                created: Some(saved.record.clone()),
                data: Some(
                    json!({"trust":{"kind":"claim_document","record":saved.record,"document":saved.document,"authority_current":authority==IdentityAuthenticity::Accepted,"replayed":true}}),
                ),
                ..Default::default()
            });
        }
        let document = match request {
            Request::Publish {
                review,
                expected_review,
            } => {
                if review.person != person
                    || review.reviewed_at > now.timestamp()
                    || now.timestamp().saturating_sub(review.reviewed_at) > 300
                    || review.document.statement.issued_at != review.reviewed_at
                    || &review.hash().map_err(claim_invalid)? != expected_review
                    || !matches!(review.document.statement.authorship, Authorship::Manual)
                {
                    return Err(claim_invalid(
                        "The reviewed manual claim changed or expired; preview again",
                    ));
                }
                self.contact_issuer_on(&mut tx, &review.document.statement.issuer, person, actor)
                    .await?;
                if self
                    .trust_profiles_on(
                        &mut tx,
                        vec![
                            review.document.statement.issuer.clone(),
                            review.document.statement.descriptor.subject.clone(),
                        ],
                        actor,
                    )
                    .await?
                    != review.profiles
                {
                    return Err(claim_invalid("A reviewed profile changed; preview again"));
                }
                let mut document = review.document.clone();
                if document.signature.is_empty() {
                    let signing = if matches!(document.statement.issuer, Identity::Organ(_)) {
                        root.as_ref()
                    } else {
                        signer.as_ref()
                    }
                    .ok_or_else(|| {
                        claim_invalid("Sign the minimal claim under its declared authority")
                    })?;
                    sign_claim_document(&mut document, signing)?;
                }
                document
            }
            Request::Import { document, as_organ } => {
                let document = Claim::parse(document.as_bytes()).map_err(claim_invalid)?;
                let reader = self
                    .contact_reader_on(&mut tx, person, actor, *as_organ)
                    .await?;
                self.trust_profiles_on(
                    &mut tx,
                    vec![
                        document.statement.issuer.clone(),
                        document.statement.descriptor.subject.clone(),
                    ],
                    actor,
                )
                .await?;
                if !document
                    .statement
                    .audience
                    .allows(&reader, &document.statement.issuer)
                {
                    return Err(EngineError::Forbidden(
                        "This claim does not grant the chosen reader access".into(),
                    ));
                }
                document
            }
            _ => return Err(claim_invalid("Choose a supported profile-claim operation")),
        };
        let authority = self
            .authenticate_profile_claim_on(&mut tx, &document, now.timestamp())
            .await?;
        if expected.is_some() && authority != IdentityAuthenticity::Accepted {
            return Err(claim_invalid(
                "The issuer's current signing authority is withheld",
            ));
        }
        let existing = store::profile_claims::find_on(&mut tx, person, &document).await?;
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
        let (committed, mut tx) = store::profile_claims::save(
            tx,
            store::profile_claims::Input {
                record,
                person: person.into(),
                request_id: id.into(),
                request: encoded,
                document,
                expected_revision: expected,
                authorization_intent: authorship.map(|proof| proof.intent_uid.clone()),
                generation_recipe: None,
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
                json!({"trust":{"kind":"claim_document","record":committed.saved.record,"document":committed.saved.document,"authority_current":authority==IdentityAuthenticity::Accepted,"replayed":!committed.created}}),
            ),
            facts: if committed.created {
                self.publish_committed_fact(committed.saved.fact)
            } else {
                Vec::new()
            },
            ..Default::default()
        })
    }

    async fn profile_claim_listing(
        &self,
        person: &str,
        actor: Option<&str>,
        subject: &Identity,
        before: Option<&str>,
        as_organ: bool,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        subject.validate().map_err(claim_invalid)?;
        let cached=self.local_custody_originals(person,actor,Some(nucleus::trust_analysis::serving::SourceKind::Claim),now.timestamp(),nucleus::trust_analysis::MAX_BYTES).await?;
        let principal = serde_json::to_string(&(person, actor, subject, as_organ))?;
        let before: Option<String> = if let Some(token) = before {
            Some(
                self.decode_read_cursor("profile-claims", &principal, token, now.timestamp())
                    .await?,
            )
        } else {
            None
        };
        let mut tx = self.store.pool.begin().await?;
        self.contact_reader_on(&mut tx, person, actor, as_organ)
            .await?;
        let context = self
            .contact_identity_context_on(&mut tx, subject, actor)
            .await?;
        if !context.readable || context.blocked {
            return Err(EngineError::Forbidden(
                "The selected profile is unavailable".into(),
            ));
        }
        let rows:Vec<String>=store::sqlx::query_scalar("SELECT c.record_uid FROM profile_claim_record c JOIN record r ON r.uid=c.record_uid WHERE c.owner_person_uid=? AND c.subject_kind=? AND c.subject_uid=? AND r.deleted_at IS NULL AND c.revision=(SELECT MAX(n.revision) FROM profile_claim_record n WHERE n.owner_person_uid=c.owner_person_uid AND n.series_hash=c.series_hash) AND (? IS NULL OR c.record_uid<?) ORDER BY c.record_uid DESC LIMIT 65")
            .bind(person).bind(subject.kind()).bind(subject.uid()).bind(&before).bind(&before).fetch_all(&mut *tx).await?;
        let more = rows.len() > 64;
        let mut last = None;
        let mut entries = Vec::new();
        for record in rows.into_iter().take(64) {
            last = Some(record.clone());
            let permitted = self
                .permitted_claim_on(&mut tx, person, actor, &record, as_organ, now)
                .await;
            let (saved, authority) = match permitted {
                Ok(permitted) => permitted,
                Err(
                    EngineError::Forbidden(_)
                    | EngineError::UnknownRecord(_)
                    | EngineError::Conflict { .. },
                ) => continue,
                Err(error) => return Err(error),
            };
            let current =
                store::profile_claims::current_on(&mut tx, person, &saved.document.statement)
                    .await? && self.custody_claim_is_current_on(&mut tx,&saved.document,&cached,actor,now.timestamp()).await?;
            entries.push(json!({"record":saved.record,"document":saved.document,"authority_current":authority==IdentityAuthenticity::Accepted,"current_retained":current}));
        }
        drop(tx);
        let next = if more {
            if let Some(last) = last {
                Some(
                    self.encode_read_cursor("profile-claims", &principal, &last, now.timestamp())
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
                json!({"trust":{"kind":"claim_list","subject":subject,"entries":entries,"next":next,"partial":next.is_some(),"checked_at":now.timestamp(),"meaning":"Permitted claim documents retained by this reader. Signatures identify issuers; they do not establish physical truth or global coverage."}}),
            ),
            ..Default::default()
        })
    }
}
