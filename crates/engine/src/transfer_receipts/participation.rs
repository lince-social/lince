use super::*;
use nucleus::transfer::receipt::{
    ApprovalRole, Document, MAX_AUTHORITIES, OriginalAuthority, Participation,
    ParticipationRequest, original_authority_records,
};

impl Engine {
    pub async fn prepare_transfer_participation(
        &self,
        transfer: &str,
        expected_revision: u64,
        party: Option<&str>,
        actor: Option<&str>,
    ) -> Result<ParticipationRequest, EngineError> {
        self.require_permission(actor, "transfer:read").await?;
        let transfer = self.resolve(transfer).await?;
        self.require_transfer_origin_authority(&transfer).await?;
        self.refuse_unreadable(actor, std::slice::from_ref(&transfer))
            .await?;
        let current = store::transfers::get(&self.store.pool, &transfer)
            .await?
            .ok_or_else(|| invalid("Transfer is unavailable"))?;
        if u64::try_from(current.revision).ok() != Some(expected_revision) {
            return Err(invalid(
                "Review the current Transfer revision before preparing participation",
            ));
        }
        let fact = store::transfers::revision_fact(&self.store.pool, &transfer, expected_revision)
            .await?
            .ok_or_else(|| invalid("Original signed Transfer revision is unavailable"))?;
        if fact.record_uid != transfer
            || !nucleus::fact::verify_chain_step(&fact)
            || !crate::trust::verify_fact(&self.store, &fact).await?
        {
            return Err(invalid(
                "Participation requires verified original agreement evidence",
            ));
        }
        let revision: TransferRevisionEvidence = serde_json::from_str(
            fact.payload
                .as_deref()
                .ok_or_else(|| invalid("Original signed agreement is unavailable"))?,
        )?;
        if revision.terms.transfer.uid != transfer || revision.terms.revision != expected_revision {
            return Err(invalid(
                "Participation differs from the original signed agreement",
            ));
        }
        let party = match party {
            Some(party) => Some(self.resolve(party).await?),
            None => None,
        };
        if let Some(party) = party.as_ref() {
            self.refuse_unreadable(actor, std::slice::from_ref(party))
                .await?;
            if let Some(actor) = actor {
                let query = protein::Protein {
                    source: protein::Source::Transfer,
                    filter: vec![protein::Predicate::UidEq(transfer.clone())],
                    fields: None,
                    include: Default::default(),
                    aggregate: None,
                    order: vec![],
                    limit: Some(1),
                };
                let rows = protein::execute_for(&self.store, &query, Some(actor)).await?;
                if !rows.iter().any(|row| {
                    row["parties"].as_array().is_some_and(|parties| {
                        parties
                            .iter()
                            .any(|entry| entry["actor"].as_str() == Some(party))
                    })
                }) {
                    return Err(EngineError::Forbidden(
                        "This Transfer party is not disclosed to you".into(),
                    ));
                }
            }
        }
        let terms = Participation::from_revision(&revision, &fact.hash, party.as_deref())
            .map_err(invalid)?;
        if terms.origin
            != store::organs::local(&self.store.pool)
                .await?
                .ok_or_else(|| invalid("No local Organ"))?
                .uid
        {
            return Err(invalid(
                "Original signed source Organ differs from the canonical origin",
            ));
        }
        let request = ParticipationRequest {
            public_history_terms: revision.terms.transfer.public_history.clone(),
            transfer,
            party,
            terms,
        };
        request.validate().map_err(invalid)?;
        Ok(request)
    }

    pub async fn create_transfer_participation_authority(
        &self,
        root: &crate::trust::Signer,
        cell: &str,
        terms: Participation,
        now: DateTime<Utc>,
    ) -> Result<Authority, EngineError> {
        terms.validate().map_err(invalid)?;
        if terms.organ != root.actor_uid {
            return Err(EngineError::Forbidden(
                "Only the named Organ may approve its original participation".into(),
            ));
        }
        let mut authority = self
            .create_transfer_receipt_authority(root, cell, now)
            .await?;
        authority.participation = Some(terms);
        authority.signature = root.sign_bytes(&authority.signing_bytes().map_err(invalid)?);
        authority.validate(now.timestamp()).map_err(invalid)?;
        self.verify_transfer_receipt_authority(&authority).await?;
        Ok(authority)
    }

    pub(crate) async fn check_transfer_participation_records(
        &self,
        records: &[String],
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<Vec<String>, EngineError> {
        let records = original_authority_records(records).map_err(invalid)?;
        self.refuse_unreadable(actor, &records).await?;
        for uid in &records {
            let saved = store::transfer_receipts::get(&self.store.pool, uid).await?;
            self.verify_transfer_receipt_document(&saved.document, now)
                .await?;
            let Document::Authority(authority) = saved.document else {
                return Err(invalid("Choose an original participation approval"));
            };
            if authority.participation.is_none()
                || authority.issued_at > now.timestamp()
                || authority.expires_at <= now.timestamp()
            {
                return Err(invalid(
                    "Original participation approval is absent or outside its validity interval",
                ));
            }
        }
        Ok(records)
    }

    pub(super) async fn attach_original_participation(
        &self,
        scope: &mut Scope,
        revision: &TransferRevisionEvidence,
        fact_hash: &str,
        activation: &TransferActivationEvidence,
        giver: &str,
        receiver: &str,
    ) -> Result<(), EngineError> {
        if activation.original_authorities.len() > MAX_AUTHORITIES {
            return Err(invalid("Original participation exceeds its proof budget"));
        }
        let mut proofs = Vec::new();
        for (role, person) in [
            (ApprovalRole::Origin, None),
            (ApprovalRole::Giver, Some(giver)),
            (ApprovalRole::Receiver, Some(receiver)),
        ] {
            let expected =
                Participation::from_revision(revision, fact_hash, person).map_err(invalid)?;
            let mut matches = activation
                .original_authorities
                .iter()
                .filter(|authority| authority.participation.as_ref() == Some(&expected));
            if let Some(authority) = matches.next() {
                if matches.next().is_some() {
                    return Err(invalid(
                        "Original participation contains conflicting role authorities",
                    ));
                }
                let proof = OriginalAuthority {
                    role,
                    authority: authority.clone(),
                };
                proof.validate(scope).map_err(invalid)?;
                self.verify_transfer_receipt_authority(authority).await?;
                proofs.push(proof);
            }
        }
        if proofs.len() == 3 {
            scope.party_attribution = PartyAttribution::OriginalOrganAuthority;
            scope.original_authorities = proofs;
        }
        Ok(())
    }
}
