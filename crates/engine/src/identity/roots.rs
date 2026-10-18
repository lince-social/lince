use super::*;
use crate::{actions::VerifiedActionAuthorship, trust::Signer};
use nucleus::identity::roots::{self as model, Proof, Review};

pub fn sign_organ_root_proof(proof: &mut Proof, signer: &Signer) -> Result<(), EngineError> {
    if signer.actor_uid != proof.organ
        || signer.key_id != crate::roster::ROOT_KEY_ID
        || signer.public_key_b64() != proof.public_key
    {
        return Err(invalid(
            "Use this Organ's declared root key to sign its key-control proof",
        ));
    }
    if proof.signature.is_empty() {
        proof.signature = signer.sign_bytes(&proof.signing_bytes().map_err(invalid)?);
    }
    Ok(())
}

fn verify(proof: &Proof, now: i64) -> Result<(), EngineError> {
    proof.validate().map_err(invalid)?;
    validate(&PersonKey {
        subject: proof.organ.clone(),
        key_id: crate::roster::ROOT_KEY_ID.into(),
        public_key: proof.public_key.clone(),
    })?;
    if proof.issued_at > now
        || !crate::trust::verify_bytes_strict(
            &proof.public_key,
            &proof.signing_bytes().map_err(invalid)?,
            &proof.signature,
        )
    {
        return Err(invalid(
            "The Organ key-control proof is forged or not yet valid",
        ));
    }
    Ok(())
}

impl Engine {
    pub async fn current_root_key(&self, organ: &str) -> Result<Option<String>, EngineError> {
        let state =
            store::identity_roots::state_on(&mut *self.store.pool.acquire().await?, organ).await?;
        Ok(if state.available {
            state
                .current
                .filter(|key| !key.revoked)
                .map(|key| key.public_key)
        } else {
            None
        })
    }

    async fn root_profile_on(
        &self,
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        subject: &str,
        actor: Option<&str>,
    ) -> Result<i64, EngineError> {
        let graph = crate::access::policy_graph_on(tx, &[subject.into()])
            .await
            .map_err(|error| invalid(error.to_string()))?;
        if !self.readable_on(tx, actor, &graph).await?.contains(subject) {
            return Err(EngineError::Forbidden(
                "The selected Organ is unavailable".into(),
            ));
        }
        let kind: Option<String> =
            store::sqlx::query_scalar("SELECT kind FROM record WHERE uid=? AND deleted_at IS NULL")
                .bind(subject)
                .fetch_optional(&mut **tx)
                .await?;
        if kind.as_deref() != Some("organ") {
            return Err(invalid(
                "Choose an existing Organ profile for root recognition",
            ));
        }
        Ok(store::record_revisions::get_on(tx, subject).await?.revision)
    }

    pub(super) async fn organ_root_command(
        &self,
        request: &Command,
        actor: Option<&str>,
        now: DateTime<Utc>,
        authorship: Option<&VerifiedActionAuthorship>,
    ) -> Result<ActionOutcome, EngineError> {
        self.require_permission(actor, request.permission()).await?;
        let subject = self.resolve(request.subject()).await?;
        if subject != request.subject() {
            return Err(invalid(
                "Use the canonical Organ UID for independent key comparison",
            ));
        }
        let exporting_root =
            if matches!(request, Command::ExportOrganRoot { .. }) {
                Some(self.root_signer().await?.ok_or_else(|| {
                    invalid("Use the device holding this Organ's current root key")
                })?)
            } else {
                None
            };
        let preview_person = if matches!(request, Command::PreviewOrganRoot { .. }) {
            let derived = self.signer_actor_uid().await;
            Some(
                self.transfer_action_person(actor, None, derived.as_deref())
                    .await?,
            )
        } else {
            None
        };
        if let Command::AcceptOrganRoot {
            request_id,
            review,
            expected_review,
        } = request
        {
            let person = self
                .transfer_action_person(actor, Some(&review.person), None)
                .await?;
            let signer = self.transfer_person_signer(&person, authorship).await?;
            let mut tx = store::write_tx(&self.store.pool).await?;
            self.require_permission_on(&mut tx, actor, "organ:update")
                .await?;
            self.require_permission_on(&mut tx, actor, "record:create")
                .await?;
            let revision = self.root_profile_on(&mut tx, &subject, actor).await?;
            let encoded = serde_json::to_value(request)?;
            if let Some(saved) =
                store::identity_roots::replay_on(&mut tx, &person, request_id, &encoded).await?
            {
                let graph = crate::access::policy_graph_on(&mut tx, &[saved.record.clone()])
                    .await
                    .map_err(|error| invalid(error.to_string()))?;
                if !self
                    .readable_on(&mut tx, actor, &graph)
                    .await?
                    .contains(&saved.record)
                {
                    return Err(EngineError::Forbidden(
                        "The original root recognition is unavailable".into(),
                    ));
                }
                let state = store::identity_roots::state_on(&mut tx, &subject).await?;
                return Ok(ActionOutcome {
                    created: Some(saved.record.clone()),
                    data: Some(
                        json!({"identity":{"kind":"organ_root_accepted","record":saved.record,"revision":saved.revision,"state":state,"current":saved.revision==state.revision && state.available && state.current.as_ref().is_some_and(|key|!key.revoked),"replayed":true}}),
                    ),
                    ..Default::default()
                });
            }
            verify(&review.proof, now.timestamp())?;
            let state = store::identity_roots::state_on(&mut tx, &subject).await?;
            let revoked:bool=store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM identity_revocation WHERE organ_uid=? AND revoked_key=?)").bind(&subject).bind(&review.proof.public_key).fetch_one(&mut *tx).await?;
            if revoked
                || review.person != person
                || revision != review.profile_revision
                || state != review.previous
                || review.reviewed_at > now.timestamp()
                || now.timestamp().saturating_sub(review.reviewed_at) > 300
                || &review.hash().map_err(invalid)? != expected_review
            {
                return Err(invalid(
                    "The reviewed Organ, root state, authority or proof changed; compare a fresh review",
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
            let (saved, mut tx) = store::identity_roots::save(
                tx,
                store::identity_roots::Input {
                    record,
                    request_id: request_id.clone(),
                    request: encoded,
                    review: review.clone(),
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
            let state = store::identity_roots::state_on(&mut tx, &subject).await?;
            tx.commit().await?;
            return Ok(ActionOutcome {
                created: Some(saved.record.clone()),
                facts: self.publish_committed_fact(saved.fact),
                data: Some(
                    json!({"identity":{"kind":"organ_root_accepted","record":saved.record,"revision":saved.revision,"state":state,"current":true,"replayed":false,"meaning":"The independently compared owner key is recognized. Original keys, revocations and signed history remain retained. This decision does not add Cells, settle a Transfer or establish reliability."}}),
                ),
                ..Default::default()
            });
        }
        let mut tx = self.store.pool.begin().await?;
        let revision = self.root_profile_on(&mut tx, &subject, actor).await?;
        let state = store::identity_roots::state_on(&mut tx, &subject).await?;
        match request {
            Command::OrganRoots { .. } => {
                let history = store::identity_roots::history_on(&mut tx, &subject).await?;
                let mut roots = Vec::new();
                for saved in history {
                    let revoked:bool=store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM identity_revocation WHERE organ_uid=? AND revoked_key=?)").bind(&subject).bind(&saved.review.proof.public_key).fetch_one(&mut *tx).await?;
                    roots.push(json!({"revision":saved.revision,"public_key":saved.review.proof.public_key,"fingerprint":saved.review.proof.fingerprint().map_err(invalid)?,"revoked":revoked,"active_record":saved.active}));
                }
                Ok(ActionOutcome {
                    data: Some(
                        json!({"identity":{"kind":"organ_roots","subject":subject,"state":state,"history":roots,"profile_revision":revision}}),
                    ),
                    ..Default::default()
                })
            }
            Command::PreviewOrganRoot { document, .. } => {
                let proof = Proof::parse(document.as_bytes()).map_err(invalid)?;
                verify(&proof, now.timestamp())?;
                if proof.organ != subject || state.original.is_none() {
                    return Err(invalid(
                        "Compare a replacement for this already recognized Organ, not another profile or an unpaired identity",
                    ));
                }
                let revoked:bool=store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM identity_revocation WHERE organ_uid=? AND revoked_key=?)").bind(&subject).bind(&proof.public_key).fetch_one(&mut *tx).await?;
                if revoked {
                    return Err(invalid("The proposed root has a known revocation"));
                }
                let person =
                    preview_person.ok_or_else(|| invalid("Choose the reviewing Person"))?;
                let review = Review {
                    person,
                    proof,
                    profile_revision: revision,
                    previous: state,
                    reviewed_at: now.timestamp(),
                };
                Ok(ActionOutcome {
                    data: Some(
                        json!({"identity":{"kind":"organ_root_preview","review":review,"review_hash":review.hash().map_err(invalid)?,"new_fingerprint":review.proof.fingerprint().map_err(invalid)?,"previous_fingerprint":review.previous.current.as_ref().map(|key|model::fingerprint(&subject,&key.public_key)).transpose().map_err(invalid)?,"meaning":"A signature proves control of the proposed key. Independently compare the Organ and full fingerprint before accepting this owner identity. Future owner signatures use the selected root; membership updates remain governed by the roster protocol."}}),
                    ),
                    ..Default::default()
                })
            }
            Command::ExportOrganRoot { .. } => {
                let local: Option<String> = store::sqlx::query_scalar(
                    "SELECT uid FROM record WHERE kind='organ' AND slug=? AND deleted_at IS NULL",
                )
                .bind(store::organs::LOCAL_ORGAN_SLUG)
                .fetch_optional(&mut *tx)
                .await?;
                if local.as_deref() != Some(&subject) {
                    return Err(EngineError::Forbidden(
                        "Issue a key-control proof only for this Cell's Organ".into(),
                    ));
                }
                let root =
                    exporting_root.ok_or_else(|| invalid("Use this Organ's owner device"))?;
                let revoked:bool=store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM identity_revocation WHERE organ_uid=? AND revoked_key=?)").bind(&subject).bind(root.public_key_b64()).fetch_one(&mut *tx).await?;
                if revoked {
                    return Err(invalid("The configured root key has a known revocation"));
                }
                let mut proof = Proof {
                    organ: subject,
                    public_key: root.public_key_b64(),
                    nonce: nucleus::new_uid("r"),
                    issued_at: now.timestamp(),
                    signature: String::new(),
                };
                sign_organ_root_proof(&mut proof, &root)?;
                verify(&proof, now.timestamp())?;
                Ok(ActionOutcome {
                    data: Some(
                        json!({"identity":{"kind":"organ_root_proof","proof":proof,"fingerprint":proof.fingerprint().map_err(invalid)?,"document":serde_json::to_string(&proof)?,"meaning":"Public proof of key control, not proof of identity. Compare its full fingerprint independently on the receiving Lince."}}),
                    ),
                    ..Default::default()
                })
            }
            _ => Err(invalid("Choose an Organ identity operation")),
        }
    }
}
