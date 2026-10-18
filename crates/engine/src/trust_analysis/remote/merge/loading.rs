use super::*;

impl Engine {
    pub(super) async fn merge_local_originals(
        &self,
        review: &Review,
        actor: Option<&str>,
        originals: &mut Originals,
    ) -> Result<(), EngineError> {
        let cached=self.local_custody_originals(&review.person,actor,None,nucleus::execution::now().timestamp(),MAX_BYTES.saturating_sub(originals.bytes)).await?;
        originals.partial|=!cached.complete;
        originals.local_complete&=cached.complete;
        for input in cached.items {
            let readable=input.readable && self.remote_packet_access(review,actor,&input.artifact).await.is_ok();
            if !readable {
                if let Artifact::Receipt(document)=input.artifact.as_ref() {
                    if let Some(bundle)=document.public_bundle() {originals.withheld_sources.insert((bundle.publication.scope.source.origin.clone(),bundle.publication.scope.source.occurrence.clone()));}
                }
            } else {originals.custody_records.insert(input.record.clone());}
            if !originals.push(input.artifact,readable.then_some(Provenance::Record {record:input.record,version:1}))? {originals.local_complete=false;return Ok(());}
        }
        let series = originals
            .items
            .iter()
            .filter_map(|input| match input.artifact.as_ref() {
                Artifact::Claim(document) => Some((
                    document.statement.issuer.clone(),
                    document.statement.claim.clone(),
                )),
                _ => None,
            })
            .chain(
                review
                    .draft
                    .linked_claims
                    .iter()
                    .map(|series| (series.issuer.clone(), series.claim.clone())),
            )
            .collect::<BTreeSet<_>>();
        {
            let rows: Vec<(String,String,String,String)> = store::sqlx::query_as("SELECT c.record_uid,c.owner_person_uid,c.source_origin,c.source_occurrence FROM transfer_receipt_record c JOIN record r ON r.uid=c.record_uid WHERE c.kind='publication' AND (r.deleted_at IS NULL OR c.owner_person_uid=?) ORDER BY c.created_at DESC,c.record_uid DESC LIMIT 5001").bind(&review.person).fetch_all(&self.store.pool).await?;
            originals.partial |= rows.len() > MAX_DISCLOSURES;
            originals.local_complete &= rows.len() <= MAX_DISCLOSURES;
            for (record, owner, origin, occurrence) in rows.into_iter().take(MAX_DISCLOSURES) {
                let mut tx = self.store.pool.begin().await?;
                if self
                    .contact_record_readable_on(&mut tx, &record, actor)
                    .await
                    .is_err()
                {
                    if owner == review.person {
                        originals.partial = true;
                        originals.withheld_sources.insert((origin, CanonicalHash::parse(occurrence).map_err(|error| invalid(error.to_string()))?));
                    }
                    continue;
                }
                let saved = store::transfer_receipts::get_on(&mut tx, &record, None).await?;
                if saved.document.public_bundle().is_none() {
                    continue;
                }
                drop(tx);
                let artifact = Arc::new(Artifact::Receipt(Box::new(saved.document)));
                if self
                    .remote_packet_access(review, actor, &artifact)
                    .await
                    .is_err()
                {
                    if owner == review.person {
                        originals.partial = true;
                        originals.withheld_sources.insert((origin, CanonicalHash::parse(occurrence).map_err(|error| invalid(error.to_string()))?));
                    }
                    continue;
                }
                if !originals.push(
                    artifact,
                    Some(Provenance::Record {
                        record,
                        version: saved.version,
                    }),
                )? {
                    originals.local_complete = false;
                    return Ok(());
                }
            }
        }
        let contacts: Vec<String> = store::sqlx::query_scalar("SELECT c.record_uid FROM contact_disclosure_record c WHERE c.owner_person_uid=? AND c.revision=(SELECT MAX(n.revision) FROM contact_disclosure_record n WHERE n.owner_person_uid=c.owner_person_uid AND n.relation_hash=c.relation_hash AND n.issuer_kind=c.issuer_kind AND n.issuer_uid=c.issuer_uid) ORDER BY c.relation_hash,c.issuer_kind,c.record_uid LIMIT 5001").bind(&review.person).fetch_all(&self.store.pool).await?;
        originals.partial |= contacts.len() > MAX_DISCLOSURES;
        originals.local_complete &= contacts.len() <= MAX_DISCLOSURES;
        for record in contacts.into_iter().take(MAX_DISCLOSURES) {
            let mut tx = self.store.pool.begin().await?;
            let saved = store::contact_disclosures::get_on(&mut tx, &record).await?;
            let readable = self
                .contact_record_readable_on(&mut tx, &record, actor)
                .await
                .is_ok();
            drop(tx);
            let artifact = Arc::new(Artifact::Contact(saved.document));
            let readable = readable
                && self
                    .remote_packet_access(review, actor, &artifact)
                    .await
                    .is_ok();
            if !originals.push(
                artifact,
                readable.then_some(Provenance::Record { record, version: 1 }),
            )? {
                originals.local_complete = false;
                return Ok(());
            }
        }
        for (issuer, claim) in series {
            let rows: Vec<String> = store::sqlx::query_scalar("SELECT c.record_uid FROM profile_claim_record c WHERE c.owner_person_uid=? AND c.issuer_kind=? AND c.issuer_uid=? AND c.claim_uid=? AND c.revision=(SELECT MAX(n.revision) FROM profile_claim_record n WHERE n.owner_person_uid=c.owner_person_uid AND n.series_hash=c.series_hash) ORDER BY c.record_uid LIMIT 5001").bind(&review.person).bind(issuer.kind()).bind(issuer.uid()).bind(&claim).fetch_all(&self.store.pool).await?;
            originals.partial |= rows.len() > MAX_DISCLOSURES;
            originals.local_complete &= rows.len() <= MAX_DISCLOSURES;
            for record in rows.into_iter().take(MAX_DISCLOSURES) {
                let mut tx = self.store.pool.begin().await?;
                let saved = store::profile_claims::get_on(&mut tx, &record).await?;
                let readable = self
                    .contact_record_readable_on(&mut tx, &record, actor)
                    .await
                    .is_ok();
                drop(tx);
                let artifact = Arc::new(Artifact::Claim(saved.document));
                let readable = readable
                    && self
                        .remote_packet_access(review, actor, &artifact)
                        .await
                        .is_ok();
                let version = match artifact.as_ref() {
                    Artifact::Claim(document) => document.statement.revision,
                    _ => unreachable!(),
                };
                if !originals.push(
                    artifact,
                    readable.then_some(Provenance::Record { record, version }),
                )? {
                    originals.local_complete = false;
                    return Ok(());
                }
            }
        }
        Ok(())
    }
}
