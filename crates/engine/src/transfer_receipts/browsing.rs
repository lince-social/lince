use super::*;
use crate::actions::ActionOutcome;
use nucleus::transfer::receipt::Document;
use serde_json::json;

pub(super) fn principal(person: &str, actor: Option<&str>) -> Result<String, EngineError> {
    Ok(serde_json::to_string(&(person, actor))?)
}

impl Engine {
    pub(super) async fn inspect_receipt_version(
        &self,
        record: &str,
        version: u64,
        person: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
        export: bool,
    ) -> Result<ActionOutcome, EngineError> {
        let uid = self.resolve(record).await?;
        self.refuse_unreadable(actor, std::slice::from_ref(&uid))
            .await?;
        let saved = store::transfer_receipts::get_version(&self.store.pool, &uid, version).await?;
        self.verify_transfer_receipt_document(&saved.document, now)
            .await?;
        let mut result = commands::result(&saved)?;
        let data = result.data.as_mut().unwrap();
        data["receipt"]["historical"] = json!(true);
        data["receipt"]["version_hash"] = json!(saved.document.hash().map_err(invalid)?);
        data["receipt"]["checked_at"] = json!(now.timestamp());
        if export {
            if saved.owner != person {
                return Err(EngineError::Forbidden(
                    "Import a readable receipt into your own review before exporting it".into(),
                ));
            }
            if saved.document.expires_at() <= now.timestamp() {
                return Err(invalid(
                    "This receipt's sharing term has expired; review new terms before exporting",
                ));
            }
            data["receipt_export"] = json!(serde_json::to_string(&saved.document)?);
        }
        Ok(result)
    }

    pub(super) async fn receipt_versions(
        &self,
        record: &str,
        before: Option<&str>,
        person: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        let uid = self.resolve(record).await?;
        self.refuse_unreadable(actor, std::slice::from_ref(&uid))
            .await?;
        let purpose = format!("transfer-receipt-versions:{uid}");
        let principal = principal(person, actor)?;
        let position = match before {
            Some(token) => Some(
                self.decode_read_cursor::<i64>(&purpose, &principal, token, now.timestamp())
                    .await?,
            ),
            None => None,
        };
        let mut versions =
            store::transfer_receipts::versions(&self.store.pool, &uid, position).await?;
        let more = versions.len() > 32;
        versions.truncate(32);
        let next = if more {
            Some(
                self.encode_read_cursor(
                    &purpose,
                    &principal,
                    &versions.last().unwrap().version,
                    now.timestamp(),
                )
                .await?,
            )
        } else {
            None
        };
        Ok(ActionOutcome {
            data: Some(
                json!({"receipt":{"record":uid,"state":"version_headers","document":{"kind":"receipt_versions","value":{"record":uid,"versions":versions,"next_before":next,"coverage":"Saved version headers only. Opening a version checks its original document and current signing authority."}}}}),
            ),
            ..Default::default()
        })
    }

    pub(super) async fn receipt_proof_page(
        &self,
        record: &str,
        version: u64,
        before: Option<&str>,
        person: &str,
        actor: Option<&str>,
        now: DateTime<Utc>,
    ) -> Result<ActionOutcome, EngineError> {
        let uid = self.resolve(record).await?;
        self.refuse_unreadable(actor, std::slice::from_ref(&uid))
            .await?;
        let saved = store::transfer_receipts::get_version(&self.store.pool, &uid, version).await?;
        let purpose = format!("transfer-receipt-proof:{uid}:{version}");
        let principal = principal(person, actor)?;
        let hash = saved.document.hash().map_err(invalid)?;
        let offset = if let Some(token) = before {
            let (expected, offset): (CanonicalHash, usize) = self
                .decode_read_cursor(&purpose, &principal, token, now.timestamp())
                .await?;
            if expected != hash {
                return Err(invalid(
                    "The original proof differs from this page; reopen the saved version",
                ));
            }
            offset
        } else {
            0
        };
        self.verify_transfer_receipt_document(&saved.document, now)
            .await?;
        let (header, events) = match &saved.document {
            Document::Publication(bundle) | Document::ProofFragment(bundle) => (
                json!({"scope":bundle.publication.scope,"approvals":bundle.publication.approvals,"person_approvals":bundle.publication.person_approvals,"authorities":bundle.authorities}),
                bundle.publication.events.as_slice(),
            ),
            Document::Review(review) => (serde_json::to_value(review)?, &[][..]),
            Document::Authority(authority) => (serde_json::to_value(authority)?, &[][..]),
        };
        if offset > events.len() {
            return Err(invalid("The proof page is outside this saved version"));
        }
        let end = (offset + 32).min(events.len());
        let next = if end < events.len() {
            Some(
                self.encode_read_cursor(&purpose, &principal, &(&hash, end), now.timestamp())
                    .await?,
            )
        } else {
            None
        };
        let state = commands::result(&saved)?.data.unwrap()["receipt"]["state"].clone();
        Ok(ActionOutcome {
            data: Some(
                json!({"receipt":{"record":uid,"version":version,"historical":true,"state":"verified_proof_view","document":{"kind":"receipt_proof","value":{"record":uid,"version":version,"hash":hash,"document_state":state,"checked_at":now.timestamp(),"header":header,"events":&events[offset..end],"from":offset,"total":events.len(),"next_before":next,"coverage":"Read-only page of an original saved document, verified with current identity and authority information. This page is not a portable receipt and cannot be imported as evidence. No quantities are added."}}}}),
            ),
            ..Default::default()
        })
    }
}
