use super::*;
use nucleus::social::requests::*;
use store::sqlx::Row;

impl Engine {
    pub(super) async fn social_retain_receive_failure(
        &self,
        context: &str,
        service: &str,
        document: &PrivateDelivery,
        accepted_at: i64,
        error: &str,
    ) -> Result<(), EngineError> {
        let now = nucleus::execution::now().timestamp();
        let hash = request_auth::validate_collected_delivery(document, accepted_at, now)?;
        let envelope = &document.envelope;
        let reference = json!({"envelope":envelope.id,"envelope_hash":hash,"message":envelope.message,"content_hash":envelope.content_hash,"mailbox":envelope.route});
        let mut tx = self.social_write_tx().await?;
        store::sqlx::query("DELETE FROM social_receive_failure WHERE expires_at<=? OR context IN (SELECT uid FROM record WHERE deleted_at IS NOT NULL)").bind(now).execute(&mut *tx).await?;
        let existing: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM social_receive_failure WHERE context=? AND service=? AND envelope=?)").bind(context).bind(service).bind(&envelope.id).fetch_one(&mut *tx).await?;
        let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_receive_failure")
            .fetch_one(&mut *tx)
            .await?;
        if !existing && count >= 256 {
            return Err(invalid(
                "Resolve or discard pending ciphertext errors before retaining more than 256 references",
            ));
        }
        store::sqlx::query("INSERT INTO social_receive_failure(context,service,envelope,reference,error,expires_at) VALUES(?,?,?,?,?,?) ON CONFLICT(context,service,envelope) DO UPDATE SET error=excluded.error WHERE social_receive_failure.reference=excluded.reference")
            .bind(context).bind(service).bind(&envelope.id).bind(serde_json::to_string(&reference)?).bind(error.chars().take(500).collect::<String>()).bind(envelope.expires_at).execute(&mut *tx).await?;
        tx.commit().await?;
        self.notify_query_changed();
        Ok(())
    }

    pub(super) async fn social_discard_private(
        &self,
        context: &str,
        service: &str,
        envelope: &str,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        self.social_own_record(context, actor).await?;
        let changed = store::sqlx::query("UPDATE social_receive_failure SET discard=1,next_attempt=0 WHERE context=? AND service=? AND envelope=? AND expires_at>?")
            .bind(context).bind(service).bind(envelope).bind(nucleus::execution::now().timestamp()).execute(&self.store.pool).await?.rows_affected();
        if changed == 0 {
            return Err(invalid(
                "This failed ciphertext reference is no longer pending on this device",
            ));
        }
        store::sqlx::query(
            "UPDATE social_pickup_work SET next_attempt=0 WHERE context=? AND service=?",
        )
        .bind(context)
        .bind(service)
        .execute(&self.store.pool)
        .await?;
        Ok(
            json!({"status":"Discard saved on this device. Waiting for current authorization and the selected mailbox; retained conversation history is unchanged"}),
        )
    }

    pub(super) async fn social_discard_failed_from(
        &self,
        network: &std::sync::Arc<dyn Network>,
        context: &str,
        service: &str,
    ) -> Result<(), EngineError> {
        let now = nucleus::execution::now().timestamp();
        store::sqlx::query("DELETE FROM social_receive_failure WHERE expires_at<=?")
            .bind(now)
            .execute(&self.store.pool)
            .await?;
        let row = store::sqlx::query("SELECT envelope,reference FROM social_receive_failure WHERE context=? AND service=? AND discard=1 AND next_attempt<=? ORDER BY next_attempt,envelope LIMIT 1").bind(context).bind(service).bind(now).fetch_optional(&self.store.pool).await?;
        let Some(row) = row else {
            return Ok(());
        };
        let envelope: String = row.get("envelope");
        let result = async {
        let reference: Value = serde_json::from_str(&row.get::<String,_>("reference"))?;
        let access = self.social_private_access(context, vec![envelope.clone()]).await?;
        if reference["mailbox"] != access.mailbox {
            return Err(invalid("Discard waits for the original device's mailbox authority; a fresh account cannot sign for its old mailbox"));
        }
        let id = format!("account:{context}");
        let key = self.social_storage_key().await?;
        let (body, _) = store::social::device_state(&self.store.pool, &id).await?.ok_or_else(|| invalid("This device's live reply account is unavailable"))?;
        let account: session::AccountState = session::open_local(&id, &body, &key)?;
        if account.route.signing_key != access.certificate.signing_key {
            return Err(invalid("Reply keys changed before signing the refusal"));
        }
        let mut receipt = RecipientReceipt {
            envelope: envelope.clone(),
            envelope_hash: reference["envelope_hash"].as_str().ok_or_else(||invalid("Missing failed envelope hash"))?.into(),
            message: reference["message"].as_str().ok_or_else(||invalid("Missing failed message identity"))?.into(),
            content_hash: reference["content_hash"].as_str().ok_or_else(||invalid("Missing failed content hash"))?.into(),
            stage: ReceiptStage::RecipientRefused,
            at: now.max(access.certificate.issued_at),
            certificate: access.certificate.clone(),
            signature: String::new(),
        };
        receipt.signature = account.signing_key()?.sign_bytes(&signing_bytes("recipient-receipt", &receipt)?);
        let response = network.request(service, PublicRequest::DiscardPrivate { access:access.clone(), receipts:vec![receipt] }).await?;
        if response["service"] != service || response["mailbox"] != access.mailbox || response["stage"] != "recipient-refused" || response["envelopes"] != json!([envelope]) {
            return Err(invalid("The mailbox did not confirm the exact recipient refusal"));
        }
        store::sqlx::query("DELETE FROM social_receive_failure WHERE context=? AND service=? AND envelope=? AND reference=? AND discard=1").bind(context).bind(service).bind(&envelope).bind(serde_json::to_string(&reference)?).execute(&self.store.pool).await?;
        self.notify_query_changed();
        Ok::<(), EngineError>(())
        }.await;
        if let Err(error) = result {
            store::sqlx::query("UPDATE social_receive_failure SET error=?,next_attempt=?+30 WHERE context=? AND service=? AND envelope=? AND discard=1").bind(error.to_string().chars().take(500).collect::<String>()).bind(now).bind(context).bind(service).bind(envelope).execute(&self.store.pool).await?;
        }
        Ok(())
    }

    pub(super) async fn social_receive_failures(
        &self,
        actor: Option<&str>,
    ) -> Result<Vec<Value>, EngineError> {
        let rows = store::sqlx::query("SELECT context,service,envelope,error,discard FROM social_receive_failure WHERE expires_at>? ORDER BY context,service,envelope LIMIT 256").bind(nucleus::execution::now().timestamp()).fetch_all(&self.store.pool).await?;
        let mut failures = Vec::new();
        for row in rows {
            let context: String = row.get("context");
            if self.social_own_record(&context, actor).await.is_ok() {
                failures.push(json!({"context":context,"service":row.get::<String,_>("service"),"envelope":row.get::<String,_>("envelope"),"error":row.get::<String,_>("error"),"discard":row.get::<bool,_>("discard")}));
            }
        }
        Ok(failures)
    }
}
