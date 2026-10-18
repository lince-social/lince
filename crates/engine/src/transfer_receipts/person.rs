use super::*;
use crate::{actions::Action, trust::Signer};
use nucleus::transfer::receipt::{ApprovalRole, Command, Document, PersonApproval};

pub fn sign_person_consent(action: &mut Action, signer: &Signer) -> Result<(), EngineError> {
    let Action::TransferReceipt {
        person,
        request: Command::ApprovePerson { approval, .. },
        ..
    } = action
    else {
        return Ok(());
    };
    if approval.person != signer.actor_uid
        || person
            .as_ref()
            .is_some_and(|person| person != &signer.actor_uid)
    {
        return Err(EngineError::Forbidden(
            "A Person can sign only their own public attribution consent".into(),
        ));
    }
    complete_signature(approval, signer)
}

fn complete_signature(approval: &mut PersonApproval, signer: &Signer) -> Result<(), EngineError> {
    if approval.person != signer.actor_uid
        || (!approval.key_id.is_empty() && approval.key_id != signer.key_id)
    {
        return Err(EngineError::Forbidden(
            "Consent requires the selected Person's accepted signing key".into(),
        ));
    }
    if approval.signature.is_empty() {
        approval.key_id = signer.key_id.clone();
        approval.signature = signer.sign_bytes(&approval.signing_bytes().map_err(invalid)?);
    }
    Ok(())
}

impl Engine {
    pub(super) async fn verify_receipt_person_approval(
        &self,
        scope: &Scope,
        approval: &PersonApproval,
        now: DateTime<Utc>,
    ) -> Result<(), EngineError> {
        approval.validate(scope, now.timestamp()).map_err(invalid)?;
        match store::records::get(&self.store.pool,&approval.person).await? {
            Some(record) if record.kind == "person" => {},
            Some(_) => return Err(invalid("Public Person consent cannot use an Organ or another Record kind as a Person")),
            None => return Err(EngineError::Conflict {code:"transfer_receipt_unknown_identity",message:"Independently recognize this Person profile and its signing key before accepting attribution".into()}),
        }
        let key = crate::trust::key_of(&self.store,&approval.person,&approval.key_id).await?.ok_or_else(||EngineError::Conflict {code:"transfer_receipt_unknown_identity",message:"Independently verify this Person's signing identity before accepting their attribution consent".into()})?;
        let revoked = store::roster::is_revoked(&self.store.pool, &approval.person, &key).await?;
        if revoked {
            return Err(withheld("The Person's attribution signing key is revoked"));
        }
        if !crate::trust::verify_bytes_strict(
            &key,
            &approval.signing_bytes().map_err(invalid)?,
            &approval.signature,
        ) {
            return Err(invalid("Invalid personal attribution consent signature"));
        }
        Ok(())
    }

    pub(super) async fn approve_receipt_person(
        &self,
        person: &str,
        record: &str,
        version: u64,
        approval: &PersonApproval,
        private_transfer: Option<&str>,
        actor: Option<&str>,
        now: DateTime<Utc>,
        signer: Option<&Signer>,
    ) -> Result<(Document, String, u64), EngineError> {
        let uid = self.resolve(record).await?;
        self.refuse_unreadable(actor, std::slice::from_ref(&uid))
            .await?;
        let saved = store::transfer_receipts::get(&self.store.pool, &uid).await?;
        if saved.owner != person || saved.version != version {
            return Err(invalid(
                "Review your own current receipt version before consenting",
            ));
        }
        self.verify_transfer_receipt_document(&saved.document, now)
            .await?;
        let Document::Review(mut review) = saved.document.clone() else {
            return Err(invalid(
                "Person attribution consent requires a publication review",
            ));
        };
        if review.scope.expires_at <= now.timestamp()
            || approval.person != person
            || approval.role == ApprovalRole::Origin
            || !review
                .scope
                .public_persons
                .iter()
                .any(|target| target.role == approval.role && target.person == person)
            || review
                .person_approvals
                .iter()
                .any(|prior| prior.role == approval.role)
        {
            return Err(EngineError::Forbidden(
                "Consent must name your own unapproved role in this unexpired scope".into(),
            ));
        }
        if review.scope.party_attribution == PartyAttribution::OriginalOrganAuthority {
            let mut own_role = private_transfer
                .map(|transfer| {
                    review
                        .scope
                        .opens_original_party(approval.role, transfer, person)
                })
                .transpose()
                .map_err(invalid)?
                .unwrap_or(false);
            if private_transfer.is_some() && !own_role {
                return Err(EngineError::Forbidden(
                    "The private reference does not open your original role".into(),
                ));
            }
            if !own_role {
                let source =
                    store::transfer_receipts::source_for_scope(&self.store.pool, &saved.document)
                        .await?;
                if let Some(mut source) = source {
                    source.outcome_records.clear();
                    source.stages = vec![nucleus::transfer::receipt::Stage::Settled];
                    let checked = self
                        .prepare_transfer_receipt_scope(
                            source,
                            actor,
                            DateTime::from_timestamp(review.scope.issued_at, 0)
                                .ok_or_else(|| invalid("Invalid scope time"))?,
                        )
                        .await?;
                    own_role = checked.source == review.scope.source
                        && checked.original_authorities == review.scope.original_authorities
                        && checked.public_persons == review.scope.public_persons;
                }
            }
            if !own_role {
                return Err(EngineError::Forbidden(
                    "Use the private Transfer reference to consent as the actual original party"
                        .into(),
                ));
            }
        }
        let mut approval = approval.clone();
        if approval.signature.is_empty() {
            complete_signature(
                &mut approval,
                signer.ok_or_else(|| {
                    invalid(
                        "Sign this minimal consent on your Person's device before submitting it",
                    )
                })?,
            )?;
        }
        self.verify_receipt_person_approval(&review.scope, &approval, now)
            .await?;
        review.person_approvals.push(approval);
        let document = saved
            .document
            .merge(&Document::Review(review), now.timestamp())
            .map_err(invalid)?;
        Ok((document, uid, saved.version))
    }
}
