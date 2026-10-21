use super::claims::claim_invalid;
use super::*;
use crate::actions::ActionOutcome;
use chrono::{DateTime, Utc};
use nucleus::trust_analysis::{claims, query::Query};
use serde_json::json;

impl Engine {
    async fn readable_claim_snapshot_on(
        &self,
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        person: &str,
        actor: Option<&str>,
        document: &claims::Document,
        as_organ: bool,
        now: DateTime<Utc>,
    ) -> Result<(IdentityAuthenticity, bool), EngineError> {
        let reader = self.contact_reader_on(tx, person, actor, as_organ).await?;
        self.trust_profiles_on(
            tx,
            vec![
                document.statement.issuer.clone(),
                document.statement.descriptor.subject.clone(),
            ],
            actor,
        )
        .await?;
        if !document
            .statement
            .audience
            .allows(&reader, &document.statement.issuer)
        {
            return Err(EngineError::Forbidden(
                "This signed claim does not grant the selected reader access".into(),
            ));
        }
        let authority = self
            .authenticate_profile_claim_on(tx, document, now.timestamp())
            .await?;
        let latest = store::profile_claims::latest_on(tx, person, &document.statement).await?;
        let current = if latest < document.statement.revision {
            true
        } else if latest == document.statement.revision {
            let hashes: Vec<String> = store::sqlx::query_scalar("SELECT DISTINCT statement_hash FROM profile_claim_record WHERE owner_person_uid=? AND series_hash=? AND revision=? LIMIT 2")
                .bind(person).bind(document.statement.series().map_err(claim_invalid)?.as_str())
                .bind(latest as i64).fetch_all(&mut **tx).await?;
            hashes.len() == 1
                && hashes[0] == document.statement.id().map_err(claim_invalid)?.as_str()
        } else {
            false
        };
        Ok((authority, current))
    }

    pub(super) async fn profile_claim_snapshot(
        &self,
        person: &str,
        actor: Option<&str>,
        encoded: &str,
        sources: Option<&Query>,
        as_organ: bool,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        let document = claims::Document::parse(encoded.as_bytes()).map_err(claim_invalid)?;
        let mut tx = self.store.pool.begin().await?;
        let (authority, current) = self
            .readable_claim_snapshot_on(&mut tx, person, actor, &document, as_organ, now)
            .await?;
        let mut result = json!({"kind":"claim_document","document":document,"authority_current":authority==IdentityAuthenticity::Accepted,"not_known_superseded":current,"retained":false,"checked_at":now.timestamp()});
        if let Some(sources) = sources {
            if !current
                || authority != IdentityAuthenticity::Accepted
                || document.statement.withdrawn
                || document.statement.expires_at <= now.timestamp()
            {
                return Err(claim_invalid(
                    "This snapshot is superseded, conflicted, withheld, withdrawn or expired; inspect newer signed evidence",
                ));
            }
            if document.statement.category != claims::Category::Experience {
                sources.validate().map_err(claim_invalid)?;
                let statement = &document.statement;
                let context = self
                    .contact_identity_context_on(&mut tx, &statement.issuer, actor)
                    .await?;
                let trusted = claims::trusted_issuer_for_activity(
                    statement,
                    sources,
                    &context,
                    now.timestamp(),
                )
                .map_err(claim_invalid)?;
                result["kind"] = json!("credential_check");
                result["trusted_issuer_for_activity"] = json!(trusted);
                result["delivery_quantity_added"] = json!("0");
                return Ok(ActionOutcome {
                    data: Some(json!({"trust":result})),
                    ..Default::default()
                });
            }
            drop(tx);
            let query = document
                .statement
                .descriptor
                .checking_query(sources, document.statement.as_of)
                .map_err(claim_invalid)?;
            let evaluated = self
                .evaluate_trust_query(person, actor, &query, as_organ, now)
                .await?;
            let support = claims::compare(&document.statement, &query, &evaluated.analysis)
                .map_err(claim_invalid)?;
            let finished_at = nucleus::execution::now();
            let mut tx = self.store.pool.begin().await?;
            let (authority, current) = self
                .readable_claim_snapshot_on(
                    &mut tx,
                    person,
                    actor,
                    &document,
                    as_organ,
                    finished_at,
                )
                .await?;
            if !current
                || authority != IdentityAuthenticity::Accepted
                || document.statement.expires_at <= finished_at.timestamp()
                || *self.watch_query_changes().borrow() != evaluated.revision
            {
                return Err(claim_invalid(
                    "The claim, evidence or current authority changed while checking; refresh",
                ));
            }
            result["kind"] = json!("claim_check");
            result["support"] = json!(support);
            result["query"] = json!(query);
            result["analysis"] = json!(evaluated.analysis);
            result["proof_records"] = json!(evaluated.proof_records);
            result["checked_at"] = json!(finished_at.timestamp());
            result["evidence_revision"] = json!(evaluated.revision);
            result["meaning"] = json!(
                "A reader-owned local snapshot, separate from the publisher's statement. Missing evidence does not prove missing activity. No import, identity adoption or remote request was made."
            );
        }
        let data = json!({"trust":result});
        if serde_json::to_vec(&data)?.len() > nucleus::trust_analysis::MAX_BYTES {
            return Err(claim_invalid(
                "The claim-check result exceeded its byte budget",
            ));
        }
        Ok(ActionOutcome {
            data: Some(data),
            ..Default::default()
        })
    }
}
