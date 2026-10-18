use super::super::serving::AuthorityStamp;
use super::*;
use nucleus::trust_analysis::discovery::{Header, Question};

pub(in crate::trust_analysis) struct RetainedHeader {
    pub header: Header,
    record: String,
    owner: String,
    version: u64,
    offer: Offer,
    artifact: Artifact,
    authority: AuthorityStamp,
}

pub(in crate::trust_analysis) struct RetainedPage {
    pub headers: Vec<RetainedHeader>,
    pub last: Option<i64>,
    pub more: bool,
}

impl Engine {
    pub(in crate::trust_analysis) async fn discover_custody_records(
        &self,
        endpoint: &str,
        node: &str,
        question: &Question,
        upper: i64,
        before: Option<i64>,
        work: usize,
        headers: usize,
        seen: &BTreeSet<nucleus::karma::CanonicalHash>,
        now: i64,
    ) -> Result<RetainedPage, EngineError> {
        let rows:Vec<(i64,String,String,String,String,i64)>=store::sqlx::query_as("SELECT c.sequence,c.record_uid,c.owner_person_uid,c.offer_json,c.artifact_hash,c.byte_count FROM evidence_custody_commitment c JOIN record r ON r.uid=c.record_uid WHERE c.sequence<=? AND (? IS NULL OR c.sequence<?) AND c.retained_until>? AND r.deleted_at IS NULL ORDER BY c.sequence DESC LIMIT 33").bind(upper).bind(before).bind(before).bind(now).fetch_all(&self.store.pool).await?;
        let mut accepted = Vec::new();
        let mut scanned = 0;
        let mut bytes = 0usize;
        let mut last = before;
        for (sequence, record, owner, offer, hash, size) in rows.iter().take(work.min(32)) {
            if accepted.len() >= headers
                || bytes.saturating_add(*size as usize) > nucleus::trust_analysis::MAX_BYTES
            {
                break;
            }
            scanned += 1;
            last = Some(*sequence);
            bytes += *size as usize;
            let offer: Offer = serde_json::from_str(offer)?;
            if seen.contains(&offer.artifact) {
                continue;
            }
            let mut tx = self.store.pool.begin().await?;
            let policy = match self
                .custody_policy_for_on(&mut tx, owner, &offer.sender, None)
                .await
            {
                Ok(policy) => policy,
                Err(_) => continue,
            };
            let body: Option<Vec<u8>> = store::sqlx::query_scalar(
                "SELECT body FROM evidence_custody_payload WHERE artifact_hash=?",
            )
            .bind(hash)
            .fetch_optional(&mut *tx)
            .await?;
            let Some(body) = body else {
                continue;
            };
            let artifact = Artifact::parse(&body, now).map_err(invalid)?;
            if !question.matches(&artifact) {
                continue;
            }
            let authority = authority_stamp(&mut tx, &artifact).await?;
            drop(tx);
            if self
                .fetch_custody_evidence(endpoint, node, &offer.artifact, 0, now)
                .await
                .is_err()
            {
                continue;
            }
            let header = Header {
                hash: offer.artifact.clone(),
                kind: artifact.kind(),
                bytes: offer.bytes,
                expires_at: offer
                    .retained_until
                    .min(artifact.expires_at().map_err(invalid)?),
            };
            accepted.push(RetainedHeader {
                header,
                record: record.clone(),
                owner: owner.clone(),
                version: policy.version,
                offer,
                artifact,
                authority,
            });
        }
        Ok(RetainedPage {
            headers: accepted,
            last,
            more: scanned < rows.len(),
        })
    }

    pub(in crate::trust_analysis) async fn validate_custody_header_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        saved: &RetainedHeader,
        reader: &str,
        now: i64,
    ) -> Result<(), EngineError> {
        let policy = self
            .custody_policy_for_on(tx, &saved.owner, &saved.offer.sender, Some(saved.version))
            .await?;
        self.contact_record_readable_on(tx, &saved.record, Some(&saved.owner))
            .await?;
        let identity = Identity::Organ(reader.into());
        let context = self
            .contact_identity_context_on(tx, &identity, Some(&saved.owner))
            .await?;
        let retained:bool=store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM evidence_custody_commitment WHERE record_uid=? AND retained_until>?)").bind(&saved.record).bind(now).fetch_one(&mut **tx).await?;
        if !retained
            || !context.readable
            || context.blocked
            || !policy.policy.readers.allows(&identity, &context)
            || !saved.offer.readers.allows(&identity, &context)
            || authority_stamp(tx, &saved.artifact).await? != saved.authority
        {
            return Err(invalid("Retained evidence access changed during discovery"));
        }
        Ok(())
    }
}
