use super::claims::claim_invalid as invalid;
use super::*;
use crate::actions::{ActionOutcome, VerifiedActionAuthorship};
use chrono::{DateTime, Utc};
use nucleus::trust_analysis::claims::{
    self, Authorship,
    recipes::{
        self, Definition, Delegate, Mode, Permit, Recipe, Terms,
        commands::{Command as Request, Review},
    },
};
use serde_json::json;
mod authoring;
mod evaluation;
mod runtime;

pub(super) fn prepare_recipe_action(
    person: Option<&str>,
    request: &mut Request,
    signer: &Signer,
) -> Result<(), EngineError> {
    match request {
        Request::Preview { draft } if matches!(draft.claim.issuer, Identity::Person(_)) => {
            if draft.claim.issuer.uid() != signer.actor_uid
                || person.is_some_and(|person| person != signer.actor_uid)
            {
                return Err(EngineError::Forbidden(
                    "Review only your own Person's publication permit".into(),
                ));
            }
            draft.claim.key_id = signer.key_id.clone();
            draft.claim.public_key = signer.public_key_b64();
        }
        Request::Save { review, .. }
            if matches!(review.recipe.definition.terms.issuer, Identity::Person(_)) =>
        {
            if review.recipe.definition.controller != signer.actor_uid
                || person.is_some_and(|person| person != signer.actor_uid)
            {
                return Err(EngineError::Forbidden(
                    "Sign only your own reviewed recipe permit".into(),
                ));
            }
            sign_recipe_permit(&mut review.recipe.permit, signer)?;
        }
        _ => {}
    }
    Ok(())
}

pub fn sign_recipe_permit(permit: &mut Permit, signer: &Signer) -> Result<(), EngineError> {
    if permit.terms.issuer.uid() != signer.actor_uid
        || permit.key_id != signer.key_id
        || permit.public_key != signer.public_key_b64()
        || (matches!(permit.terms.issuer, Identity::Organ(_))
            && !crate::roster::is_root_key_id(&signer.key_id))
    {
        return Err(EngineError::Forbidden(
            "Use the exact reviewed Person key or Organ owner key for this permit".into(),
        ));
    }
    if permit.signature.is_empty() {
        permit.signature = signer.sign_bytes(&permit.signing_bytes().map_err(invalid)?);
    }
    Ok(())
}

impl Engine {
    async fn recipe_delegate(&self) -> Result<Delegate, EngineError> {
        let signer = self
            .organ_signer
            .lock()
            .await
            .clone()
            .ok_or_else(|| invalid("This Cell's publication signing key is unavailable"))?;
        let cell = store::cells::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("Select the local signing Cell"))?;
        if signer.key_id != crate::roster::cell_key_id(&cell.uid) {
            return Err(invalid("Use this Cell's ordinary operational signing key"));
        }
        let public_key = signer.public_key_b64();
        let delegate = Delegate {
            organ: signer.actor_uid,
            cell: cell.uid,
            key_id: signer.key_id,
            public_key,
        };
        if !self
            .recipe_delegate_current_on(&mut *self.store.pool.acquire().await?, &delegate, true)
            .await?
        {
            return Err(invalid(
                "This Cell no longer has current publication authority",
            ));
        }
        Ok(delegate)
    }

    pub(super) async fn recipe_delegate_current_on(
        &self,
        connection: &mut store::sqlx::SqliteConnection,
        delegate: &Delegate,
        local: bool,
    ) -> Result<bool, EngineError> {
        let revoked: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM identity_revocation WHERE organ_uid=? AND revoked_key=?)",
        )
        .bind(&delegate.organ)
        .bind(&delegate.public_key)
        .fetch_one(&mut *connection)
        .await?;
        if revoked {
            return Ok(false);
        }
        let stored: Option<(String, String)> =
            store::sqlx::query_as("SELECT payload,signature FROM organ_roster WHERE organ_uid=?")
                .bind(&delegate.organ)
                .fetch_optional(&mut *connection)
                .await?;
        if let Some((payload, signature)) = stored {
            let roster: crate::roster::Roster = serde_json::from_str(&payload)?;
            let signed = crate::roster::SignedRoster { roster, signature };
            let (recognized, accepted) = Self::root_identity_authority_on(
                connection,
                &delegate.organ,
                &signed.roster.root_key,
            )
            .await?;
            if recognized && (!accepted || !crate::roster::roster_signature_is_valid(&signed)) {
                return Ok(false);
            }
            if accepted {
                if local
                    && DateTime::parse_from_rfc3339(&signed.roster.not_after)
                        .map_err(|error| invalid(error.to_string()))?
                        .with_timezone(&Utc)
                        <= nucleus::execution::now()
                {
                    return Ok(false);
                }
                let mut entries = signed
                    .roster
                    .cells
                    .iter()
                    .filter(|entry| entry.cell_uid == delegate.cell);
                return Ok(entries.next().is_some_and(|entry| {
                    entry.operational_key == delegate.public_key
                        && entry.may(crate::roster::CAP_REPRESENT)
                }) && entries.next().is_none());
            }
        }
        Ok(!local)
    }

    async fn authenticate_claim_permit_on(
        &self,
        connection: &mut store::sqlx::SqliteConnection,
        permit: &Permit,
        now: i64,
    ) -> Result<IdentityAuthenticity, EngineError> {
        permit.validate().map_err(invalid)?;
        if permit.terms.issued_at > now {
            return Err(invalid("The recipe permit is not effective yet"));
        }
        let bytes = permit.signing_bytes().map_err(invalid)?;
        let accepted = self
            .authenticate_identity_signature_on(
                connection,
                SignedIdentity {
                    issuer: &permit.terms.issuer,
                    key_id: &permit.key_id,
                    public_key: &permit.public_key,
                    signature: &permit.signature,
                    payload: &bytes,
                },
            )
            .await?;
        let revoked: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM identity_revocation WHERE organ_uid=? AND revoked_key=?)",
        )
        .bind(permit.terms.issuer.uid())
        .bind(&permit.terms.delegate.public_key)
        .fetch_one(&mut *connection)
        .await?;
        if revoked
            || !self
                .recipe_delegate_current_on(connection, &permit.terms.delegate, false)
                .await?
        {
            return Ok(IdentityAuthenticity::WithheldAuthority);
        }
        Ok(accepted)
    }

    pub(super) async fn authenticate_generated_claim_on(
        &self,
        connection: &mut store::sqlx::SqliteConnection,
        document: &claims::Document,
        now: i64,
    ) -> Result<IdentityAuthenticity, EngineError> {
        let Authorship::Generated { permit, .. } = &document.statement.authorship else {
            return Err(invalid(
                "Choose a generated statement with an explicit signing permit",
            ));
        };
        let authority = self
            .authenticate_claim_permit_on(connection, permit, now)
            .await?;
        if !crate::trust::verify_bytes_strict(
            &document.public_key,
            &document.signing_bytes().map_err(invalid)?,
            &document.signature,
        ) {
            return Err(invalid(
                "Invalid signature from the reviewed recipe publisher",
            ));
        }
        Ok(authority)
    }

    async fn owned_recipe_on(
        &self,
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        person: &str,
        actor: Option<&str>,
        record: &str,
    ) -> Result<store::claim_recipes::Saved, EngineError> {
        self.contact_record_readable_on(tx, record, actor).await?;
        let saved = store::claim_recipes::get_on(tx, record).await?;
        if saved.owner != person {
            return Err(EngineError::Forbidden(
                "Open your own private publication recipe".into(),
            ));
        }
        Ok(saved)
    }

    pub(super) async fn claim_recipe_command(
        &self,
        person: &str,
        id: &str,
        request: &Request,
        actor: Option<&str>,
        now: DateTime<Utc>,
        authorship: Option<&VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        if let Request::Recompute { record } = request {
            return self
                .recompute_claim_recipe(person, actor, record, now)
                .await;
        }
        if let Request::Preview { draft } = request {
            let delegate = self.recipe_delegate().await?;
            let mut key_id = draft.claim.key_id.clone();
            let mut public_key = draft.claim.public_key.clone();
            if matches!(draft.claim.issuer, Identity::Organ(_)) {
                self.require_permission(actor, "organ:update").await?;
                let root = self.root_signer().await?.ok_or_else(|| {
                    invalid("Use the owner-key Cell to review this Organ's recipe permit")
                })?;
                if root.actor_uid != draft.claim.issuer.uid() {
                    return Err(invalid("Review only this Organ's signing permit"));
                }
                key_id = root.key_id.clone();
                public_key = root.public_key_b64();
            } else if key_id.is_empty() || public_key.is_empty() {
                let signer = self
                    .transfer_person_signer(person, authorship)
                    .await?
                    .ok_or_else(|| invalid("Choose the Person signing key on your device"))?;
                key_id = signer.key_id.clone();
                public_key = signer.public_key_b64();
            }
            let mut inputs = draft
                .claim
                .descriptor
                .checking_query(&draft.inputs, now.timestamp())
                .map_err(invalid)?;
            inputs.until = draft.claim.descriptor.until;
            let mut definition = Definition {
                starting_claim_revision: 0,
                recipe: draft
                    .recipe
                    .clone()
                    .unwrap_or_else(|| nucleus::new_uid("r")),
                revision: draft
                    .expected_revision
                    .checked_add(1)
                    .ok_or_else(|| invalid("Recipe revision overflow"))?,
                controller: person.into(),
                terms: Terms {
                    issuer: draft.claim.issuer.clone(),
                    claim: draft
                        .claim
                        .claim
                        .clone()
                        .unwrap_or_else(|| nucleus::new_uid("r")),
                    title: draft.claim.title.clone(),
                    descriptor: draft.claim.descriptor.clone(),
                    comparator: draft.claim.comparator,
                    audience: draft.claim.audience.clone(),
                    issued_at: now.timestamp(),
                    expires_at: now
                        .timestamp()
                        .saturating_add(i64::from(draft.permit_seconds)),
                    lifetime_seconds: draft.claim.lifetime_seconds,
                    delegate,
                },
                inputs,
                start_enabled: draft.start_enabled,
                refresh_seconds: draft.refresh_seconds,
            };
            definition.terms.validate().map_err(invalid)?;
            let series = nucleus::karma::canonical_hash(
                "lince.profile-claim.series.v1",
                &(&definition.terms.issuer, &definition.terms.claim),
            )
            .map_err(|error| invalid(error.to_string()))?;
            let revision:i64=store::sqlx::query_scalar("SELECT COALESCE(MAX(revision),0) FROM profile_claim_record WHERE owner_person_uid=? AND series_hash=?").bind(person).bind(series.as_str()).fetch_one(&self.store.pool).await?;
            definition.starting_claim_revision =
                u64::try_from(revision).map_err(|error| invalid(error.to_string()))?;
            let recipe = Recipe {
                permit: Permit {
                    recipe: definition.id().map_err(invalid)?,
                    revision: definition.revision,
                    terms: definition.terms.clone(),
                    key_id,
                    public_key,
                    signature: String::new(),
                },
                definition,
            };
            recipe.validate_unsigned().map_err(invalid)?;
            let mut tx = self.store.pool.begin().await?;
            self.contact_issuer_on(&mut tx, &recipe.definition.terms.issuer, person, actor)
                .await?;
            self.contact_record_readable_on(
                &mut tx,
                &recipe.definition.terms.delegate.organ,
                actor,
            )
            .await?;
            self.contact_record_readable_on(&mut tx, &recipe.definition.terms.delegate.cell, actor)
                .await?;
            if !Self::identity_key_authority_on(
                &mut tx,
                &recipe.definition.terms.issuer,
                &recipe.permit.key_id,
                &recipe.permit.public_key,
            )
            .await?
            .1
            {
                return Err(invalid(
                    "Independently recognize the current permit-signing identity",
                ));
            }
            if store::claim_recipes::latest_on(&mut tx, person, &recipe.definition.recipe).await?
                != draft.expected_revision
            {
                return Err(invalid("The recipe changed; review its latest revision"));
            }
            let profiles = self
                .trust_profiles_on(
                    &mut tx,
                    vec![
                        recipe.definition.terms.issuer.clone(),
                        recipe.definition.terms.descriptor.subject.clone(),
                        Identity::Person(person.into()),
                    ],
                    actor,
                )
                .await?;
            let review = Review {
                recipe,
                expected_revision: draft.expected_revision,
                reviewed_at: now.timestamp(),
                profiles,
            };
            return Ok(ActionOutcome {
                data: Some(
                    json!({"trust":{"kind":"recipe_review","hash":review.hash().map_err(invalid)?,"review":review}}),
                ),
                ..Default::default()
            });
        }
        if let Request::List { before } = request {
            let principal = serde_json::to_string(&(person, actor))?;
            let before: Option<String> = if let Some(before) = before {
                Some(
                    self.decode_read_cursor("claim-recipes", &principal, before, now.timestamp())
                        .await?,
                )
            } else {
                None
            };
            let mut tx = self.store.pool.begin().await?;
            let rows:Vec<String>=store::sqlx::query_scalar("SELECT c.record_uid FROM profile_claim_recipe_record c JOIN record r ON r.uid=c.record_uid WHERE c.owner_person_uid=? AND r.deleted_at IS NULL AND c.revision=(SELECT MAX(n.revision) FROM profile_claim_recipe_record n WHERE n.owner_person_uid=c.owner_person_uid AND n.recipe_uid=c.recipe_uid) AND (? IS NULL OR c.record_uid<?) ORDER BY c.record_uid DESC LIMIT 65").bind(person).bind(&before).bind(&before).fetch_all(&mut *tx).await?;
            let more = rows.len() > 64;
            let mut last = None;
            let mut entries = Vec::new();
            for record in rows.into_iter().take(64) {
                last = Some(record.clone());
                match self.owned_recipe_on(&mut tx, person, actor, &record).await {
                    Ok(saved) => {
                        let state = store::claim_recipes::state_on(&mut tx, &saved).await?;
                        entries.push(json!({"record":record,"recipe":saved.recipe,"state":state}));
                    }
                    Err(EngineError::Forbidden(_) | EngineError::UnknownRecord(_)) => {}
                    Err(error) => return Err(error),
                }
            }
            drop(tx);
            let next = if more {
                if let Some(last) = last {
                    Some(
                        self.encode_read_cursor(
                            "claim-recipes",
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
            return Ok(ActionOutcome {
                data: Some(json!({"trust":{"kind":"recipe_list","entries":entries,"next":next}})),
                ..Default::default()
            });
        }
        if let Request::Inspect { record } = request {
            let mut tx = self.store.pool.begin().await?;
            let saved = self.owned_recipe_on(&mut tx, person, actor, record).await?;
            let state = store::claim_recipes::state_on(&mut tx, &saved).await?;
            let current = store::claim_recipes::current_on(&mut tx, &saved).await?;
            let authority = self
                .authenticate_claim_permit_on(&mut tx, &saved.recipe.permit, now.timestamp())
                .await?;
            return Ok(ActionOutcome {
                data: Some(
                    json!({"trust":{"kind":"recipe_document","record":record,"recipe":saved.recipe,"state":state,"current":current,"authority_current":authority==IdentityAuthenticity::Accepted}}),
                ),
                ..Default::default()
            });
        }
        self.save_or_control_claim_recipe(person, id, request, actor, now, authorship)
            .await
    }
}
