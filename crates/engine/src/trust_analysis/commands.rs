use super::*;
use crate::actions::{ActionOutcome, VerifiedActionAuthorship};
use chrono::{DateTime, Utc};
use nucleus::trust_analysis::{
    commands::{Command, ContactReview, ProfileVersion},
    contacts::Statement,
};
use serde_json::json;
use store::sqlx::{Sqlite, Transaction};

impl Engine {
    pub(super) async fn contact_profiles_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        document: &Document,
        actor: Option<&str>,
    ) -> Result<Vec<ProfileVersion>, EngineError> {
        self.trust_profiles_on(
            tx,
            vec![
                document.statement.issuer.clone(),
                document.statement.contact.clone(),
            ],
            actor,
        )
        .await
    }

    pub(super) async fn trust_profiles_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        mut profiles: Vec<Identity>,
        actor: Option<&str>,
    ) -> Result<Vec<ProfileVersion>, EngineError> {
        profiles.sort();
        profiles.dedup();
        let records = profiles
            .iter()
            .map(|identity| identity.uid().to_owned())
            .collect::<Vec<_>>();
        let graph = crate::access::policy_graph_on(tx, &records)
            .await
            .map_err(|error| invalid(error.to_string()))?;
        let allowed = self.readable_on(tx, actor, &graph).await?;
        let mut versions = Vec::new();
        for identity in profiles {
            if !allowed.contains(identity.uid()) {
                return Err(EngineError::Forbidden(
                    "A contact profile is unavailable".into(),
                ));
            }
            let kind: Option<String> = store::sqlx::query_scalar(
                "SELECT kind FROM record WHERE uid=? AND deleted_at IS NULL",
            )
            .bind(identity.uid())
            .fetch_optional(&mut **tx)
            .await?;
            if kind.as_deref() != Some(identity.kind()) {
                return Err(invalid(
                    "Choose existing readable profiles of the declared identity kind",
                ));
            }
            let context = self
                .contact_identity_context_on(tx, &identity, actor)
                .await?;
            if context.blocked {
                return Err(EngineError::Forbidden("This contact is blocked".into()));
            }
            let revision = store::record_revisions::get_on(tx, identity.uid())
                .await?
                .revision;
            versions.push(ProfileVersion { identity, revision });
        }
        Ok(versions)
    }

    pub(super) async fn contact_issuer_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        issuer: &Identity,
        person: &str,
        actor: Option<&str>,
    ) -> Result<(), EngineError> {
        match issuer {
            Identity::Person(uid) if uid == person => Ok(()),
            Identity::Organ(uid) => {
                self.require_permission_on(tx, actor, "organ:update")
                    .await?;
                let local: Option<String> = store::sqlx::query_scalar(
                    "SELECT uid FROM record WHERE slug=? AND kind='organ' AND deleted_at IS NULL",
                )
                .bind(store::organs::LOCAL_ORGAN_SLUG)
                .fetch_optional(&mut **tx)
                .await?;
                if local.as_deref() != Some(uid) {
                    return Err(EngineError::Forbidden(
                        "Publish only for this Organ under explicit administration".into(),
                    ));
                }
                Ok(())
            }
            _ => Err(EngineError::Forbidden(
                "Publish only your own Person contact disclosure".into(),
            )),
        }
    }

    pub(crate) async fn trust_analysis_command(
        &self,
        explicit: Option<&str>,
        request_id: String,
        command: Command,
        actor: Option<&str>,
        now: DateTime<Utc>,
        authorship: Option<&VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        if matches!(command, Command::Context) {
            self.require_permission(actor, "record:read").await?;
            let query = protein::Protein {
                source: protein::Source::Record,
                filter: vec![protein::Predicate::Any(vec![
                    protein::Predicate::KindEq("person".into()),
                    protein::Predicate::KindEq("organ".into()),
                ])],
                fields: Some(vec!["uid".into(), "head".into(), "kind".into()]),
                include: Default::default(),
                aggregate: None,
                order: Vec::new(),
                limit: Some(129),
            };
            let mut profiles = protein::execute_for(&self.store, &query, actor).await?;
            let partial = profiles.len() > 128;
            profiles.truncate(128);
            let local = store::organs::local(&self.store.pool).await?;
            let organ = if let Some(local) = local {
                self.may_read_record(actor, &local.uid)
                    .await?
                    .then_some(local.uid)
            } else {
                None
            };
            return Ok(ActionOutcome {
                data: Some(
                    json!({"trust":{"kind":"contact_context","person":self.actor_person(actor).await?,"organ":organ,"profiles":profiles,"partial":partial,"organ_admin":self.require_permission(actor,"organ:update").await.is_ok()}}),
                ),
                ..Default::default()
            });
        }
        let person = self.transfer_action_person(actor, explicit, None).await?;
        self.require_permission(actor, command.permission()).await?;
        if !store::people::is_active(&self.store.pool, &person).await? {
            return Err(EngineError::Forbidden(
                "Use an active Person for trust controls".into(),
            ));
        }
        if request_id.is_empty()
            || request_id.len() > 256
            || request_id.chars().any(char::is_control)
        {
            return Err(invalid("Choose a request ID of 1–256 characters"));
        }
        match &command {
            Command::MatchSuppliers { request } => {
                return self.match_suppliers(&person, request, actor, now).await;
            }
            Command::Custody { request } => {
                return self
                    .evidence_custody_command(&person, request, actor, now, authorship)
                    .await;
            }
            Command::Remote { request } => {
                return self
                    .remote_trust_command(&person, &request_id, request, actor, now)
                    .await;
            }
            Command::Serving { request } => {
                return self
                    .evidence_serving_command(&person, &request_id, request, actor, now, authorship)
                    .await;
            }
            Command::Claims { request } => {
                return self
                    .profile_claim_command(&person, &request_id, request, actor, now, authorship)
                    .await;
            }
            Command::SaveQuery { .. }
            | Command::LoadQuery { .. }
            | Command::ListQueries { .. }
            | Command::AnalyzeQuery { .. } => {
                return self
                    .trust_query_command(&person, &request_id, &command, actor, now, authorship)
                    .await;
            }
            Command::ListContacts { before, as_organ } => {
                return self
                    .contact_listing(&person, actor, before.as_deref(), *as_organ, now)
                    .await;
            }
            Command::ContactPaths { policy, as_organ } => {
                return self
                    .contact_path_search(&person, actor, policy, *as_organ, now)
                    .await;
            }
            Command::InspectContact { record, as_organ }
            | Command::ExportContact { record, as_organ } => {
                let uid = self.resolve(record).await?;
                let mut tx = self.store.pool.begin().await?;
                let reader = self
                    .contact_reader_on(&mut tx, &person, actor, *as_organ)
                    .await?;
                self.contact_record_readable_on(&mut tx, &uid, actor)
                    .await?;
                let saved = store::contact_disclosures::get_on(&mut tx, &uid).await?;
                self.contact_profiles_on(&mut tx, &saved.document, actor)
                    .await?;
                if !saved
                    .document
                    .statement
                    .audience
                    .allows(&reader, &saved.document.statement.issuer)
                {
                    return Err(EngineError::Forbidden(
                        "This disclosure does not grant the selected reader access".into(),
                    ));
                }
                let authenticity = self
                    .authenticate_contact_disclosure_on(&mut tx, &saved.document, now.timestamp())
                    .await?;
                let mut data = json!({"kind":"contact_document","record":saved.record,"document":saved.document,"authority_current":authenticity==IdentityAuthenticity::Accepted,"checked_at":now.timestamp()});
                if matches!(command, Command::ExportContact { .. }) {
                    if saved.owner != person
                        || authenticity != IdentityAuthenticity::Accepted
                        || saved.document.statement.expires_at <= now.timestamp()
                    {
                        return Err(EngineError::Forbidden("Export your own retained, currently authorized and unexpired disclosure".into()));
                    }
                    data["export"] = json!(serde_json::to_string(&saved.document)?);
                }
                return Ok(ActionOutcome {
                    data: Some(json!({"trust":data})),
                    ..Default::default()
                });
            }
            _ => {}
        }
        if let Command::PreviewContact { draft } = &command {
            if draft.lifetime_seconds == 0
                || i64::from(draft.lifetime_seconds)
                    > nucleus::trust_analysis::contacts::MAX_TERM_SECONDS
            {
                return Err(invalid("Choose a disclosure term of at most 366 days"));
            }
            let mut document = Document {
                statement: Statement {
                    issuer: draft.issuer.clone(),
                    contact: draft.contact.clone(),
                    revision: draft
                        .expected_revision
                        .checked_add(1)
                        .ok_or_else(|| invalid("Disclosure revision overflow"))?,
                    issued_at: now.timestamp(),
                    expires_at: now
                        .timestamp()
                        .saturating_add(i64::from(draft.lifetime_seconds)),
                    withdrawn: draft.withdrawn,
                    audience: draft.audience.clone(),
                },
                key_id: draft.key_id.clone(),
                public_key: draft.public_key.clone(),
                signature: String::new(),
            };
            if matches!(draft.issuer, Identity::Organ(_)) {
                self.require_permission(actor, "organ:update").await?;
                let root = self
                    .root_signer()
                    .await?
                    .ok_or_else(|| invalid("Use the Cell holding this Organ's owner key"))?;
                if root.actor_uid != draft.issuer.uid() {
                    return Err(EngineError::Forbidden("Use this Organ's owner key".into()));
                }
                document.key_id = root.key_id.clone();
                document.public_key = root.public_key_b64();
            } else if document.key_id.is_empty() || document.public_key.is_empty() {
                let signer = self
                    .transfer_person_signer(&person, authorship)
                    .await?
                    .ok_or_else(|| invalid("Select the Person signing identity on your device"))?;
                document.key_id = signer.key_id.clone();
                document.public_key = signer.public_key_b64();
            }
            document.signing_bytes().map_err(invalid)?;
            let mut tx = self.store.pool.begin().await?;
            self.contact_issuer_on(&mut tx, &draft.issuer, &person, actor)
                .await?;
            let profiles = self.contact_profiles_on(&mut tx, &document, actor).await?;
            if store::contact_disclosures::latest_revision_on(&mut tx, &person, &document.statement)
                .await?
                != draft.expected_revision
            {
                return Err(invalid("The contact changed; inspect its current revision"));
            }
            let recognized = Self::identity_key_authority_on(
                &mut tx,
                &draft.issuer,
                &document.key_id,
                &document.public_key,
            )
            .await?
            .1;
            if !recognized {
                return Err(invalid(
                    "Independently recognize a current signing identity first",
                ));
            }
            let review = ContactReview {
                person,
                document,
                expected_revision: draft.expected_revision,
                reviewed_at: now.timestamp(),
                profiles,
            };
            return Ok(ActionOutcome {
                data: Some(
                    json!({"trust":{"kind":"contact_review","hash":review.hash().map_err(invalid)?,"review":review}}),
                ),
                ..Default::default()
            });
        }
        let signer = self.transfer_person_signer(&person, authorship).await?;
        let request = serde_json::to_value(&command)?;
        let expected = match &command {
            Command::PublishContact { review, .. } => Some(review.expected_revision),
            _ => None,
        };
        let organ_signer = if matches!(&command,Command::PublishContact {review,..} if matches!(review.document.statement.issuer,Identity::Organ(_)))
        {
            self.require_permission(actor, "organ:update").await?;
            self.root_signer().await?
        } else {
            None
        };
        let mut tx = store::write_tx(&self.store.pool).await?;
        self.require_permission_on(&mut tx, actor, "record:create")
            .await?;
        if let Some(saved) =
            store::contact_disclosures::replay_on(&mut tx, &person, &request_id, &request, expected)
                .await?
        {
            self.contact_record_readable_on(&mut tx, &saved.record, actor)
                .await?;
            self.contact_profiles_on(&mut tx, &saved.document, actor)
                .await?;
            match &command {
                Command::PublishContact { .. } => {
                    self.contact_issuer_on(
                        &mut tx,
                        &saved.document.statement.issuer,
                        &person,
                        actor,
                    )
                    .await?
                }
                Command::ImportContact { as_organ, .. } => {
                    let reader = self
                        .contact_reader_on(&mut tx, &person, actor, *as_organ)
                        .await?;
                    if !saved
                        .document
                        .statement
                        .audience
                        .allows(&reader, &saved.document.statement.issuer)
                    {
                        return Err(EngineError::Forbidden(
                            "This disclosure is not available to the selected reader".into(),
                        ));
                    }
                }
                _ => {}
            }
            let authenticity = self
                .authenticate_contact_disclosure_on(&mut tx, &saved.document, now.timestamp())
                .await?;
            return Ok(ActionOutcome {
                created: Some(saved.record.clone()),
                data: Some(
                    json!({"trust":{"kind":"contact_document","record":saved.record,"document":saved.document,"authority_current":authenticity==IdentityAuthenticity::Accepted,"replayed":true}}),
                ),
                ..Default::default()
            });
        }
        let (document, authority_current) = match &command {
            Command::PublishContact {
                review,
                expected_review,
            } => {
                if review.person != person
                    || review.reviewed_at > now.timestamp()
                    || now.timestamp().saturating_sub(review.reviewed_at) > 300
                    || review.document.statement.issued_at != review.reviewed_at
                    || &review.hash().map_err(invalid)? != expected_review
                {
                    return Err(invalid(
                        "This contact review changed or expired; preview it again",
                    ));
                }
                self.contact_issuer_on(&mut tx, &review.document.statement.issuer, &person, actor)
                    .await?;
                if self
                    .contact_profiles_on(&mut tx, &review.document, actor)
                    .await?
                    != review.profiles
                {
                    return Err(invalid("A reviewed profile changed; preview again"));
                }
                let mut document = review.document.clone();
                if document.signature.is_empty() {
                    let signing = match &document.statement.issuer {
                        Identity::Person(_) => signer.clone().ok_or_else(|| {
                            invalid("Sign the minimal contact disclosure on your own device")
                        })?,
                        Identity::Organ(_) => organ_signer.clone().ok_or_else(|| {
                            invalid("Use the owner-key Cell for Organ publication")
                        })?,
                    };
                    sign_contact_document(&mut document, &signing)?;
                }
                if self
                    .authenticate_contact_disclosure_on(&mut tx, &document, now.timestamp())
                    .await?
                    != IdentityAuthenticity::Accepted
                {
                    return Err(invalid("The signing authority is withheld"));
                }
                (document, true)
            }
            Command::ImportContact { document, as_organ } => {
                let document = Document::parse(document.as_bytes()).map_err(invalid)?;
                let reader = self
                    .contact_reader_on(&mut tx, &person, actor, *as_organ)
                    .await?;
                self.contact_profiles_on(&mut tx, &document, actor).await?;
                if !document
                    .statement
                    .audience
                    .allows(&reader, &document.statement.issuer)
                {
                    return Err(EngineError::Forbidden(
                        "This disclosure does not grant the selected reader access".into(),
                    ));
                }
                let authenticity = self
                    .authenticate_contact_disclosure_on(&mut tx, &document, now.timestamp())
                    .await?;
                (document, authenticity == IdentityAuthenticity::Accepted)
            }
            _ => return Err(invalid("Unsupported trust mutation")),
        };
        let existing = store::contact_disclosures::find_on(&mut tx, &person, &document).await?;
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
        let (committed, mut tx) = store::contact_disclosures::save(
            tx,
            store::contact_disclosures::Input {
                record,
                person,
                request_id,
                request,
                document,
                expected_revision: expected,
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
                json!({"trust":{"kind":"contact_document","record":committed.saved.record,"document":committed.saved.document,"authority_current":authority_current,"replayed":!committed.created}}),
            ),
            facts: if committed.created {
                self.publish_committed_fact(committed.saved.fact)
            } else {
                Vec::new()
            },
            ..Default::default()
        })
    }
}
