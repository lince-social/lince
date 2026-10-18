use super::*;
use nucleus::trust_analysis::serving::SourceKind;

pub(crate) struct LocalOriginal {
    pub artifact: Arc<Artifact>,
    pub record: String,
    pub readable: bool,
    pub expires_at: i64,
}
pub(crate) struct LocalOriginals {
    pub items: Vec<LocalOriginal>,
    pub complete: bool,
    pub bytes: usize,
    pub events: usize,
}

impl Engine {
    pub(crate) async fn custody_claim_is_current_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        document: &nucleus::trust_analysis::claims::Document,
        cached: &LocalOriginals,
        actor: Option<&str>,
        now: i64,
    ) -> Result<bool, EngineError> {
        if !cached.complete {
            return Ok(false);
        }
        for input in &cached.items {
            let Artifact::Claim(current) = input.artifact.as_ref() else {
                continue;
            };
            if current.statement.issuer != document.statement.issuer
                || current.statement.claim != document.statement.claim
                || current.statement.revision < document.statement.revision
            {
                continue;
            }
            if !input.readable
                || current.statement.revision > document.statement.revision
                || current.statement.id().map_err(invalid)?
                    != document.statement.id().map_err(invalid)?
            {
                return Ok(false);
            }
            self.validate_local_custody_on(tx, &input.record, actor, now)
                .await?;
            if self.authenticate_profile_claim_on(tx, current, now).await?
                != IdentityAuthenticity::Accepted
            {
                return Ok(false);
            }
        }
        Ok(true)
    }
    pub(crate) async fn local_custody_originals(
        &self,
        person: &str,
        actor: Option<&str>,
        kind: Option<SourceKind>,
        now: i64,
        max_bytes: usize,
    ) -> Result<LocalOriginals, EngineError> {
        let rows:Vec<(String,String,String,i64,i64)>=store::sqlx::query_as("SELECT c.record_uid,c.owner_person_uid,c.artifact_hash,c.retained_until,c.byte_count FROM evidence_custody_commitment c JOIN record r ON r.uid=c.record_uid JOIN evidence_custody_payload p ON p.artifact_hash=c.artifact_hash WHERE c.retained_until>? AND (? IS NULL OR p.kind=?) AND (r.deleted_at IS NULL OR c.owner_person_uid=?) ORDER BY c.sequence DESC LIMIT 4097").bind(now).bind(kind.map(SourceKind::as_str)).bind(kind.map(SourceKind::as_str)).bind(person).fetch_all(&self.store.pool).await?;
        let mut result = LocalOriginals {
            items: Vec::new(),
            complete: rows.len() <= nucleus::trust_analysis::custody::MAX_ITEMS as usize,
            bytes: 0,
            events: 0,
        };
        let deadline = Instant::now() + Duration::from_secs(15);
        for (record, owner, hash, expires_at, size) in rows
            .into_iter()
            .take(nucleus::trust_analysis::custody::MAX_ITEMS as usize)
        {
            if Instant::now() >= deadline
                || result.bytes.saturating_add(size as usize)
                    > max_bytes.min(nucleus::trust_analysis::MAX_BYTES)
            {
                result.complete = false;
                break;
            }
            let mut tx = self.store.pool.begin().await?;
            let readable = self
                .contact_record_readable_on(&mut tx, &record, actor)
                .await
                .is_ok();
            if !readable && owner != person {
                continue;
            }
            let body: Vec<u8> = store::sqlx::query_scalar(
                "SELECT body FROM evidence_custody_payload WHERE artifact_hash=?",
            )
            .bind(&hash)
            .fetch_one(&mut *tx)
            .await?;
            let artifact = Artifact::parse(&body, now).map_err(invalid)?;
            if artifact.hash().map_err(invalid)?.as_str() != hash || body.len() != size as usize {
                return Err(invalid(
                    "Retained original differs from its immutable acknowledgment",
                ));
            }
            let events = match &artifact {
                Artifact::Receipt(document) => document
                    .public_bundle()
                    .map_or(0, |bundle| bundle.publication.events.len()),
                _ => 1,
            };
            if result.events.saturating_add(events) > nucleus::trust_analysis::MAX_DISCLOSURES {
                result.complete = false;
                break;
            }
            result.bytes += body.len();
            result.events += events;
            result.items.push(LocalOriginal {
                artifact: Arc::new(artifact),
                record,
                readable,
                expires_at,
            });
        }
        Ok(result)
    }

    pub(crate) async fn validate_local_custody_on(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        record: &str,
        actor: Option<&str>,
        now: i64,
    ) -> Result<(), EngineError> {
        self.contact_record_readable_on(tx, record, actor).await?;
        let retained:bool=store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM evidence_custody_commitment c JOIN evidence_custody_payload p ON p.artifact_hash=c.artifact_hash WHERE c.record_uid=? AND c.retained_until>?)").bind(record).bind(now).fetch_one(&mut **tx).await?;
        if !retained {
            return Err(invalid("A retained original expired during analysis"));
        }
        Ok(())
    }
}
