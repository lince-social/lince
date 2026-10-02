use super::*;
use nucleus::social::reports::{MAX_REPORT_BYTES, REPORT_LIFETIME, Report};
use store::sqlx::Row;

fn validate(report: &Report, now: i64) -> Result<String, EngineError> {
    if report.protocol != "lince.public-report.1"
        || !nucleus::valid_uid(&report.id, "report")
        || report.service.parse::<iroh::EndpointId>().is_err()
        || report.created_at <= 0
        || report.created_at > now.saturating_add(300)
        || report.expires_at <= now
        || report.expires_at != report.created_at.saturating_add(REPORT_LIFETIME)
        || report.explanation.len() > 8 * 1024
        || serde_json::to_vec(report)?.len() > MAX_REPORT_BYTES
    {
        return Err(invalid("Invalid or expired bounded public report"));
    }
    text(&report.explanation, 2000, true).map_err(invalid)?;
    validate_snippet(&report.document, report.document.issued_at)?;
    document_hash("public-report", report)
}

fn actor_key(actor: Option<&str>) -> &str {
    actor.unwrap_or("")
}

async fn prune_on(
    tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
    now: i64,
) -> Result<(), EngineError> {
    store::sqlx::query("DELETE FROM social_report_admission WHERE day<?")
        .bind(now.div_euclid(86400))
        .execute(&mut **tx)
        .await?;
    store::sqlx::query("DELETE FROM social_received_report WHERE expires_at<=?")
        .bind(now)
        .execute(&mut **tx)
        .await?;
    store::sqlx::query("DELETE FROM social_report_seen WHERE expires_at<=?")
        .bind(now)
        .execute(&mut **tx)
        .await?;
    store::sqlx::query("UPDATE social_report_work SET state='expired',error='The report expired before this operator accepted it' WHERE state='pending' AND expires_at<=?")
        .bind(now).execute(&mut **tx).await?;
    store::sqlx::query("DELETE FROM social_report_work WHERE expires_at<=?")
        .bind(now - 86400)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

impl Engine {
    pub(super) async fn social_preview_report(
        &self,
        post: &str,
        service: &str,
        explanation: &str,
    ) -> Result<Value, EngineError> {
        if !nucleus::valid_uid(post, "post") {
            return Err(invalid("Choose a public announcement"));
        }
        let mut tx = self.social_write_tx().await?;
        let body: String = store::sqlx::query_scalar(
            "SELECT body FROM social_document WHERE kind='snippet' AND id=?",
        )
        .bind(post)
        .fetch_one(&mut *tx)
        .await?;
        let document: Snippet = serde_json::from_str(&body)?;
        if document.id != post || !ask::allowed_on(&mut tx, &document).await? {
            return Err(invalid("Choose a currently valid cached announcement"));
        }
        tx.commit().await?;
        let now = nucleus::execution::now().timestamp();
        let report = Report {
            protocol: "lince.public-report.1".into(),
            id: nucleus::new_uid("report"),
            service: service.into(),
            document,
            explanation: explanation.into(),
            created_at: now,
            expires_at: now + REPORT_LIFETIME,
        };
        let hash = validate(&report, now)?;
        Ok(
            json!({"report_preview":report,"preview_hash":hash,"can_manage_services":self.social_require_local_write().await.is_ok(),"status":"Only this signed public announcement and your explanation will be sent to the selected operator. Its transport can observe your submitting endpoint; no Organ profile or private source is attached"}),
        )
    }

    pub(super) async fn social_queue_report(
        &self,
        report: &Report,
        expected: &str,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        let now = nucleus::execution::now().timestamp();
        let hash = validate(report, now)?;
        if hash != expected {
            return Err(invalid("The report changed after preview. Review it again"));
        }
        let mut tx = self.social_write_tx().await?;
        prune_on(&mut tx, now).await?;
        let existing: Option<(String, String)> =
            store::sqlx::query_as("SELECT actor,hash FROM social_report_work WHERE id=?")
                .bind(&report.id)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some((owner, held)) = existing {
            if owner != actor_key(actor) || held != hash {
                return Err(invalid("Conflicting report identity"));
            }
        } else {
            let cached: Option<String> = store::sqlx::query_scalar(
                "SELECT hash FROM social_document WHERE kind='snippet' AND id=?",
            )
            .bind(&report.document.id)
            .fetch_optional(&mut *tx)
            .await?;
            if cached.as_deref() != Some(document_hash("snippet", &report.document)?.as_str())
                || !ask::allowed_on(&mut tx, &report.document).await?
            {
                return Err(invalid(
                    "This public announcement changed. Preview the report again",
                ));
            }
            let count: i64 = store::sqlx::query_scalar("SELECT COUNT(*) FROM social_report_work")
                .fetch_one(&mut *tx)
                .await?;
            if count >= 32 {
                return Err(invalid(
                    "Clear retained report work before adding another; this device retains at most 32 reports",
                ));
            }
            store::sqlx::query("INSERT INTO social_report_work(id,actor,service,body,hash,created_at,expires_at) VALUES(?,?,?,?,?,?,?)")
                .bind(&report.id).bind(actor_key(actor)).bind(&report.service).bind(serde_json::to_string(report)?).bind(hash).bind(report.created_at).bind(report.expires_at).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        self.notify_query_changed();
        self.social_reports(actor).await
    }

    pub(super) async fn social_reports(&self, actor: Option<&str>) -> Result<Value, EngineError> {
        let now = nucleus::execution::now().timestamp();
        let mut tx = self.social_write_tx().await?;
        prune_on(&mut tx, now).await?;
        let rows=store::sqlx::query("SELECT id,service,body,state,expires_at,error FROM social_report_work WHERE actor=? ORDER BY created_at DESC,id LIMIT 32")
            .bind(actor_key(actor)).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        let mut reports = Vec::new();
        for row in rows {
            let report: Report = serde_json::from_str(&row.get::<String, _>("body"))?;
            reports.push(json!({"id":row.get::<String,_>("id"),"post":report.document.id,"title":report.document.title,"service":row.get::<String,_>("service"),"state":row.get::<String,_>("state"),"expires_at":row.get::<i64,_>("expires_at"),"error":row.get::<Option<String>,_>("error")}));
        }
        Ok(
            json!({"reports":reports,"can_manage_services":self.social_require_local_write().await.is_ok(),"status":"Report work is private to this actor on this device. Operator acceptance means intake, not removal. Clearing stops local pending retries and cannot recall an accepted report"}),
        )
    }

    pub(super) async fn social_clear_reports(
        &self,
        actor: Option<&str>,
    ) -> Result<Value, EngineError> {
        let mut tx = self.social_write_tx().await?;
        store::sqlx::query("DELETE FROM social_report_work WHERE actor=?")
            .bind(actor_key(actor))
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.notify_query_changed();
        self.social_reports(actor).await
    }

    pub(super) async fn social_receive_report(
        &self,
        source: &str,
        endpoint: &str,
        report: &Report,
        now: i64,
    ) -> Result<Value, EngineError> {
        let settings = self.social_settings().await?;
        if !settings.directory && !settings.townsquare {
            return Err(invalid(
                "This operator has not enabled public report intake",
            ));
        }
        if source.parse::<iroh::EndpointId>().is_err() || report.service != endpoint {
            return Err(invalid("The report was not addressed to this endpoint"));
        }
        let hash = validate(report, now)?;
        let mut tx = self.social_write_tx().await?;
        prune_on(&mut tx, now).await?;
        let held: Option<(String, String)> =
            store::sqlx::query_as("SELECT source,hash FROM social_report_seen WHERE id=?")
                .bind(&report.id)
                .fetch_optional(&mut *tx)
                .await?;
        if let Some((origin, held)) = held {
            if origin != source || held != hash {
                return Err(invalid("Conflicting report identity"));
            }
        } else {
            let day = now.div_euclid(86400);
            let source_used: Option<i64> = store::sqlx::query_scalar(
                "SELECT used FROM social_report_admission WHERE source=? AND day=?",
            )
            .bind(source)
            .bind(day)
            .fetch_optional(&mut *tx)
            .await?;
            let global:i64=store::sqlx::query_scalar("SELECT COALESCE((SELECT used FROM social_report_admission WHERE source='*' AND day=?),0)")
                .bind(day).fetch_one(&mut *tx).await?;
            let sources: i64 = store::sqlx::query_scalar(
                "SELECT COUNT(*) FROM social_report_admission WHERE source!='*'",
            )
            .fetch_one(&mut *tx)
            .await?;
            let (entries,bytes):(i64,i64)=store::sqlx::query_as("SELECT COUNT(*),COALESCE(SUM(length(CAST(body AS BLOB))),0) FROM social_received_report").fetch_one(&mut *tx).await?;
            let body = serde_json::to_string(report)?;
            if source_used.unwrap_or(0) >= 8
                || global >= 256
                || source_used.is_none() && sources >= 256
                || entries >= 256
                || bytes + body.len() as i64 > 4 * 1024 * 1024
            {
                tx.commit().await?;
                return Ok(
                    json!({"report":report.id,"hash":hash,"accepted":false,"error":"This operator's bounded report intake is full or throttled"}),
                );
            }
            for origin in [source, "*"] {
                store::sqlx::query("INSERT INTO social_report_admission(source,day,used) VALUES(?,?,1) ON CONFLICT(source) DO UPDATE SET used=used+1")
                    .bind(origin).bind(day).execute(&mut *tx).await?;
            }
            store::sqlx::query("INSERT INTO social_received_report(id,source,hash,body,received_at,expires_at) VALUES(?,?,?,?,?,?)")
                .bind(&report.id).bind(source).bind(&hash).bind(body).bind(now).bind(report.expires_at).execute(&mut *tx).await?;
            store::sqlx::query(
                "INSERT INTO social_report_seen(id,source,hash,expires_at) VALUES(?,?,?,?)",
            )
            .bind(&report.id)
            .bind(source)
            .bind(&hash)
            .bind(report.expires_at)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.notify_query_changed();
        Ok(json!({"report":report.id,"hash":hash,"accepted":true,"expires_at":report.expires_at}))
    }

    pub(super) async fn social_received_reports(
        &self,
        after: Option<&str>,
    ) -> Result<Value, EngineError> {
        if after.is_some_and(|id| !nucleus::valid_uid(id, "report")) {
            return Err(invalid("Invalid report page"));
        }
        let mut tx = self.social_write_tx().await?;
        prune_on(&mut tx, nucleus::execution::now().timestamp()).await?;
        let rows:Vec<(String,String,i64)>=store::sqlx::query_as("SELECT id,body,received_at FROM social_received_report WHERE id>? ORDER BY id LIMIT 12")
            .bind(after.unwrap_or("")).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        let entries:Vec<Value>=rows.into_iter().map(|(id,body,time)| Ok(json!({"id":id,"document":serde_json::from_str::<Report>(&body)?,"received_at":time}))).collect::<Result<_,EngineError>>()?;
        let next = entries.last().and_then(|row| row["id"].as_str());
        Ok(
            json!({"received_reports":entries,"next_after":next,"can_manage_services":self.social_require_local_write().await.is_ok(),"status":"Historical signed public evidence and reporter explanations. Reports do not automatically remove listings or prove a claim; transport origins remain local admission metadata"}),
        )
    }

    pub(super) async fn social_dismiss_report(&self, id: &str) -> Result<Value, EngineError> {
        if !nucleus::valid_uid(id, "report") {
            return Err(invalid("Choose a report"));
        }
        let mut tx = self.social_write_tx().await?;
        store::sqlx::query("DELETE FROM social_received_report WHERE id=?")
            .bind(id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.notify_query_changed();
        self.social_received_reports(None).await
    }

    pub async fn social_send_reports_once(&self) -> Result<usize, EngineError> {
        self.social_require_local_write().await?;
        let now = nucleus::execution::now().timestamp();
        let mut tx = self.social_write_tx().await?;
        prune_on(&mut tx, now).await?;
        let rows=store::sqlx::query("SELECT id,actor,service,body,hash,attempts FROM social_report_work WHERE state='pending' AND expires_at>? AND next_attempt<=? ORDER BY next_attempt,id LIMIT 3")
            .bind(now).bind(now).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        if rows.is_empty() {
            return Ok(0);
        }
        let network = self.social_network()?;
        let mut accepted = 0;
        for row in rows {
            let id: String = row.get("id");
            let actor: String = row.get("actor");
            let attempt: i64 = row.get("attempts");
            let report: Report = serde_json::from_str(&row.get::<String, _>("body"))?;
            let result=async {
                self.require_permission((!actor.is_empty()).then_some(actor.as_str()),"organ:update").await?;
                self.social_require_local_write().await?;
                if validate(&report,now)?!=row.get::<String,_>("hash") || report.service!=row.get::<String,_>("service") {return Err(invalid("Invalid retained report work"));}
                let still_pending:bool=store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM social_report_work WHERE id=? AND state='pending')").bind(&id).fetch_one(&self.store.pool).await?;
                if !still_pending {return Ok(None);}
                let settings=self.social_settings().await?;
                store::social::spend(&self.store.pool,&report.service,"out",serde_json::to_vec(&report)?.len()+128,settings.outgoing_bytes_per_minute,now).await?;
                let response=tokio::time::timeout(std::time::Duration::from_secs(5),network.request(&report.service,PublicRequest::SubmitReport {document:Box::new(report.clone())})).await
                    .map_err(|_|invalid("The selected report operator did not respond"))??;
                if serde_json::to_vec(&response)?.len()>1024 || response["report"]!=id || response["hash"]!=row.get::<String,_>("hash") || !response["accepted"].is_boolean() {return Err(invalid("Invalid report intake receipt"));}
                Ok(Some(response))
            }.await;
            match result {
                Ok(Some(receipt)) => {
                    let is_accepted = receipt["accepted"] == true;
                    store::sqlx::query("UPDATE social_report_work SET state=?,error=? WHERE id=? AND state='pending'")
                        .bind(if is_accepted {"accepted"} else {"refused"}).bind(if is_accepted {None} else {Some("This operator refused intake or reached its report limits")}).bind(&id).execute(&self.store.pool).await?;
                    accepted += usize::from(is_accepted);
                }
                Ok(None) => {}
                Err(_) => {
                    store::sqlx::query("UPDATE social_report_work SET attempts=attempts+1,next_attempt=?,error='Waiting for current permission and the selected operator; report bytes remain unchanged' WHERE id=? AND state='pending'")
                        .bind(now+5*(1i64<<attempt.clamp(0,8))).bind(&id).execute(&self.store.pool).await?;
                }
            }
        }
        self.notify_query_changed();
        Ok(accepted)
    }
}
