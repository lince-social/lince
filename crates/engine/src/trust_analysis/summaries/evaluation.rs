use super::*;
use nucleus::{DecimalValue, karma::canonical_hash};
use recipes::EvaluationState;

impl Engine {
    pub(super) async fn recompute_claim_recipe(
        &self,
        person: &str,
        actor: Option<&str>,
        record: &str,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        self.require_permission(actor, "record:read").await?;
        let mut tx = self.store.pool.begin().await?;
        let saved = self.owned_recipe_on(&mut tx, person, actor, record).await?;
        if !store::claim_recipes::current_on(&mut tx, &saved).await? {
            return Err(invalid("This recipe was hidden or superseded"));
        }
        let state = store::claim_recipes::state_on(&mut tx, &saved).await?;
        let definition = &saved.recipe.definition;
        let terms = &definition.terms;
        let next = now + chrono::TimeDelta::seconds(i64::from(definition.refresh_seconds));
        if state.mode == Mode::Paused || terms.expires_at <= now.timestamp() {
            let status = if state.mode == Mode::Paused {
                "paused"
            } else if state.mode == Mode::Revoked {
                "revoked"
            } else {
                "expired"
            };
            store::claim_recipes::status_on(&mut tx, record, status, now, next, None, None).await?;
            tx.commit().await?;
            return self.recipe_status_result(person, actor, record).await;
        }
        self.contact_issuer_on(&mut tx, &terms.issuer, person, actor)
            .await?;
        self.trust_profiles_on(
            &mut tx,
            vec![terms.issuer.clone(), terms.descriptor.subject.clone()],
            actor,
        )
        .await?;
        if self
            .authenticate_claim_permit_on(&mut tx, &saved.recipe.permit, now.timestamp())
            .await?
            != IdentityAuthenticity::Accepted
        {
            return Err(invalid("The recipe's signing authority is withheld"));
        }
        drop(tx);
        let publishing = self.organ_signer.lock().await.clone().filter(|signer| {
            signer.actor_uid == terms.delegate.organ
                && signer.key_id == terms.delegate.key_id
                && signer.public_key_b64() == terms.delegate.public_key
        });
        let Some(publishing) = publishing else {
            store::claim_recipes::status_on(
                &mut *self.store.pool.acquire().await?,
                record,
                "signing_unavailable",
                now,
                next,
                None,
                None,
            )
            .await?;
            return self.recipe_status_result(person, actor, record).await;
        };
        let query = terms
            .descriptor
            .checking_query(&definition.inputs, now.timestamp())
            .map_err(invalid)?;
        let evaluated = self
            .evaluate_trust_query_mode(person, actor, &query, false, now, true)
            .await?;
        let zero = DecimalValue::from_mantissa(0, 0).unwrap();
        let expires = terms
            .expires_at
            .min(evaluated.publication_expires_at.unwrap_or(i64::MAX))
            .min(
                now.timestamp()
                    .saturating_add(i64::from(terms.lifetime_seconds)),
            );
        let mut statement = claims::Statement {
            category: Default::default(),
            title: terms.title.clone(),
            issuer: terms.issuer.clone(),
            claim: terms.claim.clone(),
            revision: 1,
            descriptor: terms.descriptor.clone(),
            comparator: terms.comparator,
            amount: zero,
            as_of: now.timestamp(),
            issued_at: now.timestamp(),
            expires_at: expires,
            withdrawn: false,
            audience: terms.audience.clone(),
            authorship: Authorship::Manual,
        };
        let support = claims::compare(&statement, &query, &evaluated.analysis).map_err(invalid)?;
        let availability = if state.mode == Mode::Revoked {
            EvaluationState::Revoked
        } else if support.observed.is_some() {
            EvaluationState::Available
        } else if support.state == claims::SupportState::ArithmeticUnavailable {
            EvaluationState::ArithmeticUnavailable
        } else {
            EvaluationState::Unknown
        };
        statement.amount = if availability == EvaluationState::Available {
            support.observed.unwrap()
        } else {
            zero
        };
        statement.withdrawn = availability != EvaluationState::Available;
        statement.authorship = Authorship::Generated {
            recipe: definition.id().map_err(invalid)?,
            revision: definition.revision,
            partial: evaluated.analysis.partial,
            state: availability,
            permit: Box::new(saved.recipe.permit.clone()),
        };
        let digest = canonical_hash(
            "lince.profile-claim.recipe-inputs.v1",
            &(
                &saved.recipe.permit.recipe,
                availability,
                statement.amount,
                evaluated.analysis.partial,
                &evaluated.analysis.rows,
                &evaluated.analysis.protocol,
                &evaluated.analysis.withheld,
            ),
        )
        .map_err(|error| invalid(error.to_string()))?;
        let current_now = nucleus::execution::now();
        let mut tx = store::write_tx(&self.store.pool).await?;
        let current = self.owned_recipe_on(&mut tx, person, actor, record).await?;
        let control = store::claim_recipes::state_on(&mut tx, &current).await?;
        if current.recipe != saved.recipe
            || control.revision != state.revision
            || control.mode != state.mode
            || !store::claim_recipes::current_on(&mut tx, &current).await?
            || *self.watch_query_changes().borrow() != evaluated.revision
            || expires <= current_now.timestamp()
            || evaluated
                .publication_expires_at
                .is_some_and(|expiry| expiry <= current_now.timestamp())
        {
            return Err(invalid(
                "The recipe, source permissions or publication term changed during calculation",
            ));
        }
        self.require_permission_on(&mut tx, actor, "record:create")
            .await?;
        self.contact_issuer_on(&mut tx, &terms.issuer, person, actor)
            .await?;
        self.trust_profiles_on(
            &mut tx,
            vec![terms.issuer.clone(), terms.descriptor.subject.clone()],
            actor,
        )
        .await?;
        if self
            .authenticate_claim_permit_on(&mut tx, &saved.recipe.permit, current_now.timestamp())
            .await?
            != IdentityAuthenticity::Accepted
            || !self
                .recipe_delegate_current_on(&mut tx, &terms.delegate, true)
                .await?
        {
            return Err(invalid(
                "The recipe publisher no longer has current authority",
            ));
        }
        let previous = if let Some(output) = &control.output {
            Some(store::profile_claims::get_on(&mut tx, output).await?)
        } else {
            None
        };
        let expected = store::profile_claims::latest_on(&mut tx, person, &statement).await?;
        let superseded = if let Some(previous) = &previous {
            !store::profile_claims::current_on(&mut tx, person, &previous.document.statement)
                .await?
        } else {
            expected != definition.starting_claim_revision
        };
        if superseded {
            store::claim_recipes::status_on(
                &mut tx,
                record,
                "output_superseded",
                current_now,
                next,
                None,
                None,
            )
            .await?;
            tx.commit().await?;
            return self.recipe_status_result(person, actor, record).await;
        }
        let renewal_limit = terms
            .expires_at
            .min(evaluated.publication_expires_at.unwrap_or(i64::MAX));
        let no_change = control.input_hash.as_deref() == Some(digest.as_str())
            && previous.as_ref().is_some_and(|previous| {
                previous.document.statement.expires_at > current_now.timestamp()
                    && (previous.document.statement.expires_at
                        > current_now.timestamp().saturating_add(30)
                        || previous.document.statement.expires_at >= renewal_limit)
            });
        let next_at = next
            .timestamp()
            .min(terms.expires_at)
            .min(evaluated.publication_expires_at.unwrap_or(i64::MAX))
            .min(
                previous
                    .as_ref()
                    .filter(|_| no_change)
                    .map(|previous| {
                        if previous.document.statement.expires_at >= renewal_limit {
                            previous.document.statement.expires_at
                        } else {
                            previous.document.statement.expires_at.saturating_sub(30)
                        }
                    })
                    .unwrap_or(i64::MAX),
            );
        let next_at =
            DateTime::from_timestamp(next_at.max(current_now.timestamp().saturating_add(1)), 0)
                .ok_or_else(|| {
                    invalid("Next recipe refresh is outside the supported clock range")
                })?;
        if no_change {
            store::claim_recipes::status_on(
                &mut tx,
                record,
                if state.mode == Mode::Revoked {
                    "revoked"
                } else if availability == EvaluationState::Unknown {
                    "unknown"
                } else if availability == EvaluationState::ArithmeticUnavailable {
                    "arithmetic_unavailable"
                } else {
                    "unchanged"
                },
                current_now,
                next_at,
                Some(digest.as_str()),
                None,
            )
            .await?;
            tx.commit().await?;
            return self.recipe_status_result(person, actor, record).await;
        }
        statement.revision = expected
            .checked_add(1)
            .ok_or_else(|| invalid("Claim revision overflow"))?;
        let mut document = claims::Document {
            statement,
            key_id: publishing.key_id.clone(),
            public_key: publishing.public_key_b64(),
            signature: String::new(),
        };
        document.signature = publishing.sign_bytes(&document.signing_bytes().map_err(invalid)?);
        if self
            .authenticate_generated_claim_on(&mut tx, &document, current_now.timestamp())
            .await?
            != IdentityAuthenticity::Accepted
        {
            return Err(invalid("Generated output authority is withheld"));
        }
        let output = nucleus::new_uid("r");
        let checkpoint = self
            .record_checkpoint_on(
                &mut tx,
                actor,
                vec![output.clone()],
                protein::authority::Operation::Create,
            )
            .await?;
        let request = json!({"recipe":record,"input":digest,"expected_revision":expected});
        let request_id = format!("claim-recipe:{record}:{}:{expected}", digest.as_str());
        let (commit, mut tx) = store::profile_claims::save(
            tx,
            store::profile_claims::Input {
                record: output,
                person: person.into(),
                request_id,
                request,
                document,
                expected_revision: Some(expected),
                authorization_intent: None,
                generation_recipe: Some(record.into()),
            },
            current_now,
            |hash| Some(publishing.sign_hash(hash)),
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
        let status = match availability {
            EvaluationState::Available => {
                if evaluated.analysis.partial {
                    "partial"
                } else {
                    "available"
                }
            }
            EvaluationState::Unknown => "unknown",
            EvaluationState::ArithmeticUnavailable => "arithmetic_unavailable",
            EvaluationState::Revoked => "revoked",
            EvaluationState::Expired => "expired",
        };
        store::claim_recipes::status_on(
            &mut tx,
            record,
            status,
            current_now,
            next_at,
            Some(digest.as_str()),
            Some(&commit.saved.record),
        )
        .await?;
        tx.commit().await?;
        let mut outcome = self.recipe_status_result(person, actor, record).await?;
        outcome.created = Some(commit.saved.record.clone());
        if commit.created {
            outcome.facts = self.publish_committed_fact(commit.saved.fact);
        }
        Ok(outcome)
    }

    async fn recipe_status_result(
        &self,
        person: &str,
        actor: Option<&str>,
        record: &str,
    ) -> Result<ActionOutcome, EngineError> {
        let mut tx = self.store.pool.begin().await?;
        let saved = self.owned_recipe_on(&mut tx, person, actor, record).await?;
        let state = store::claim_recipes::state_on(&mut tx, &saved).await?;
        Ok(ActionOutcome {
            data: Some(
                json!({"trust":{"kind":"recipe_document","record":record,"recipe":saved.recipe,"state":state}}),
            ),
            ..Default::default()
        })
    }
}
