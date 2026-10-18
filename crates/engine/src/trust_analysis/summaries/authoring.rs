use super::*;

impl Engine {
    pub(super) async fn save_or_control_claim_recipe(
        &self,
        person: &str,
        id: &str,
        request: &Request,
        actor: Option<&str>,
        now: DateTime<Utc>,
        authorship: Option<&VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        let signer = self.transfer_person_signer(person, authorship).await?;
        let encoded = serde_json::to_value(request)?;
        let root = if matches!(request,Request::Save {review,..} if matches!(review.recipe.definition.terms.issuer,Identity::Organ(_)))
        {
            self.root_signer().await?
        } else {
            None
        };
        let mut tx = store::write_tx(&self.store.pool).await?;
        if let Some(saved) = store::claim_recipes::replay_on(&mut tx, person, id, &encoded).await? {
            self.owned_recipe_on(&mut tx, person, actor, &saved.record)
                .await?;
            let state = store::claim_recipes::state_on(&mut tx, &saved).await?;
            return Ok(ActionOutcome {
                created: Some(saved.record.clone()),
                data: Some(
                    json!({"trust":{"kind":"recipe_document","record":saved.record,"recipe":saved.recipe,"state":state,"replayed":true}}),
                ),
                ..Default::default()
            });
        }
        if let Request::Control {
            record,
            expected_revision,
            mode,
        } = request
        {
            let saved = self.owned_recipe_on(&mut tx, person, actor, record).await?;
            self.require_permission_on(&mut tx, actor, "record:update")
                .await?;
            if *mode == Mode::Enabled {
                self.contact_issuer_on(
                    &mut tx,
                    &saved.recipe.definition.terms.issuer,
                    person,
                    actor,
                )
                .await?;
                if saved.recipe.definition.terms.expires_at <= now.timestamp()
                    || self
                        .authenticate_claim_permit_on(
                            &mut tx,
                            &saved.recipe.permit,
                            now.timestamp(),
                        )
                        .await?
                        != IdentityAuthenticity::Accepted
                {
                    return Err(invalid(
                        "Review a current authorized recipe before enabling publication",
                    ));
                }
            }
            let checkpoint = self
                .record_checkpoint_on(
                    &mut tx,
                    actor,
                    vec![record.clone()],
                    protein::authority::Operation::Update,
                )
                .await?;
            let fact = store::claim_recipes::control(
                &mut tx,
                &saved,
                store::claim_recipes::Control {
                    id,
                    expected: *expected_revision,
                    mode: *mode,
                    request: &encoded,
                    intent: authorship.map(|proof| proof.intent_uid.as_str()),
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
            let state = store::claim_recipes::state_on(&mut tx, &saved).await?;
            tx.commit().await?;
            return Ok(ActionOutcome {
                data: Some(
                    json!({"trust":{"kind":"recipe_document","record":record,"recipe":saved.recipe,"state":state}}),
                ),
                facts: fact
                    .map(|fact| self.publish_committed_fact(fact))
                    .unwrap_or_default(),
                ..Default::default()
            });
        }
        let Request::Save {
            review,
            expected_review,
        } = request
        else {
            return Err(invalid("Choose a reviewed recipe operation"));
        };
        let definition = &review.recipe.definition;
        if definition.controller != person
            || review.reviewed_at > now.timestamp()
            || now.timestamp().saturating_sub(review.reviewed_at) > 300
            || definition.terms.issued_at != review.reviewed_at
            || &review.hash().map_err(invalid)? != expected_review
        {
            return Err(invalid(
                "The recipe review changed or expired; review its exact scope again",
            ));
        }
        self.require_permission_on(&mut tx, actor, "record:create")
            .await?;
        self.contact_issuer_on(&mut tx, &definition.terms.issuer, person, actor)
            .await?;
        if self
            .trust_profiles_on(
                &mut tx,
                vec![
                    definition.terms.issuer.clone(),
                    definition.terms.descriptor.subject.clone(),
                    Identity::Person(person.into()),
                ],
                actor,
            )
            .await?
            != review.profiles
        {
            return Err(invalid("A reviewed recipe profile changed"));
        }
        let current_delegate = self
            .organ_signer
            .lock()
            .await
            .clone()
            .ok_or_else(|| invalid("The reviewed publishing key is unavailable"))?;
        let delegate = &definition.terms.delegate;
        self.contact_record_readable_on(&mut tx, &delegate.organ, actor)
            .await?;
        self.contact_record_readable_on(&mut tx, &delegate.cell, actor)
            .await?;
        if current_delegate.actor_uid != delegate.organ
            || current_delegate.key_id != delegate.key_id
            || current_delegate.public_key_b64() != delegate.public_key
            || !self
                .recipe_delegate_current_on(&mut tx, delegate, true)
                .await?
        {
            return Err(invalid(
                "The publishing Cell or its authority changed after review",
            ));
        }
        let mut recipe = review.recipe.clone();
        if recipe.permit.signature.is_empty() {
            let signing = if matches!(definition.terms.issuer, Identity::Organ(_)) {
                root.as_ref()
            } else {
                signer.as_ref()
            }
            .ok_or_else(|| invalid("Sign the exact recipe permit on the issuer's device"))?;
            sign_recipe_permit(&mut recipe.permit, signing)?;
        }
        if self
            .authenticate_claim_permit_on(&mut tx, &recipe.permit, now.timestamp())
            .await?
            != IdentityAuthenticity::Accepted
        {
            return Err(invalid(
                "The permit's current signing authority is withheld",
            ));
        }
        let count:i64=store::sqlx::query_scalar("SELECT COUNT(*) FROM profile_claim_recipe_record c JOIN record r ON r.uid=c.record_uid WHERE r.deleted_at IS NULL AND c.revision=(SELECT MAX(n.revision) FROM profile_claim_recipe_record n WHERE n.owner_person_uid=c.owner_person_uid AND n.recipe_uid=c.recipe_uid)").fetch_one(&mut *tx).await?;
        if count >= 64 && review.expected_revision == 0 {
            return Err(invalid(
                "This Cell's 64-recipe limit was reached; remove an unused recipe before adding another",
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
        let (commit, mut tx) = store::claim_recipes::save(
            tx,
            store::claim_recipes::Input {
                record,
                person: person.into(),
                request_id: id.into(),
                request: encoded,
                recipe,
                expected_revision: review.expected_revision,
                authorization_intent: authorship.map(|proof| proof.intent_uid.clone()),
            },
            now,
            |hash| signer.as_ref().map(|signer| signer.sign_hash(hash)),
        )
        .await?;
        if commit.created
            && let Some(checkpoint) = checkpoint
        {
            checkpoint
                .bounded()
                .finish(&mut tx, Default::default())
                .await?;
        }
        let state = store::claim_recipes::state_on(&mut tx, &commit.saved).await?;
        tx.commit().await?;
        Ok(ActionOutcome {
            created: Some(commit.saved.record.clone()),
            data: Some(
                json!({"trust":{"kind":"recipe_document","record":commit.saved.record,"recipe":commit.saved.recipe,"state":state}}),
            ),
            facts: if commit.created {
                self.publish_committed_fact(commit.saved.fact)
            } else {
                Vec::new()
            },
            ..Default::default()
        })
    }
}
