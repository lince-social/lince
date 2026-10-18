use super::*;
use nucleus::trust_analysis::discovery::{self as contract, Header, Page, Request};

impl Engine {
    pub(crate) async fn discover_served_evidence(
        &self,
        source: &str,
        node: &str,
        request: &Request,
        now: i64,
    ) -> Result<Value, EngineError> {
        request.validate(now).map_err(invalid)?;
        tokio::time::timeout(
            std::time::Duration::from_secs(3.min(request.deadline.saturating_sub(now)) as u64),
            self.discover_served_evidence_inner(source, node, request, now),
        )
        .await
        .map_err(|_| invalid("Evidence discovery work limit exceeded"))?
    }

    async fn discover_served_evidence_inner(
        &self,
        source: &str,
        node: &str,
        request: &Request,
        now: i64,
    ) -> Result<Value, EngineError> {
        let peer = self.social_ask_peer(source, false).await?;
        let _slot = self
            .evidence_serving_slots
            .try_acquire()
            .map_err(|_| invalid("Evidence serving is at its concurrent work limit"))?;
        self.serving_work_budget(&peer.organ).await?;
        let authority = self.serving_peer_authority(&peer.organ, source).await?;
        let epoch = *self.watch_query_changes().borrow();
        let question = request.question.hash().map_err(invalid)?;
        let principal = serde_json::to_string(&(
            source,
            &peer.organ,
            &request.id,
            &question,
            request.issued_at,
            request.deadline,
        ))?;
        let (upper,custody_upper,custody_stage,before): (i64,i64,bool,Option<i64>) = match &request.after {
            Some(token) => {
                self.decode_read_cursor("evidence-discovery", &principal, token, now)
                    .await?
            }
            None => {
                let (serving,custody):(i64,i64)=store::sqlx::query_as("SELECT (SELECT COALESCE(MAX(sequence),0) FROM record_evidence_serving),(SELECT COALESCE(MAX(sequence),0) FROM evidence_custody_commitment)").fetch_one(&self.store.pool).await?;
                (serving,custody,false,None)
            }
        };
        let rows: Vec<(i64,String)> = store::sqlx::query_as("SELECT a.sequence,a.record_uid FROM record_evidence_serving a JOIN record r ON r.uid=a.record_uid WHERE a.sequence<=? AND (? IS NULL OR a.sequence<?) AND a.expires_at>? AND r.deleted_at IS NULL AND NOT EXISTS(SELECT 1 FROM record_evidence_serving n WHERE n.owner_person_uid=a.owner_person_uid AND n.source_record_uid=a.source_record_uid AND n.source_kind=a.source_kind AND n.sequence>a.sequence) ORDER BY a.sequence DESC LIMIT 33")
            .bind(if custody_stage {0}else{upper}).bind(before).bind(before).bind(now).fetch_all(&self.store.pool).await?;
        let mut scanned = 0;
        let mut last = before;
        let mut accepted = Vec::new();
        let mut seen = BTreeSet::new();
        for (sequence, record) in rows.iter().take(contract::PAGE_WORK) {
            scanned += 1;
            last = Some(*sequence);
            let mut tx = self.store.pool.begin().await?;
            let saved = store::evidence_serving::get_on(&mut tx, record).await?;
            if !saved.active
                || !saved.current
                || saved.review.expires_at <= now
                || saved.review.request.audience.include.is_empty()
                || !saved.review.artifact.allows(&peer.organ)
            {
                continue;
            }
            let context = self
                .contact_identity_context_on(
                    &mut tx,
                    &Identity::Organ(peer.organ.clone()),
                    Some(&saved.owner),
                )
                .await?;
            if !context.readable
                || context.blocked
                || !saved
                    .review
                    .request
                    .audience
                    .allows(&Identity::Organ(peer.organ.clone()), &context)
                || self
                    .contact_record_readable_on(&mut tx, record, Some(&saved.owner))
                    .await
                    .is_err()
            {
                continue;
            }
            drop(tx);
            let snapshot = match self
                .serving_snapshot(
                    &saved.owner,
                    &saved.review.request,
                    Some(&saved.owner),
                    nucleus::execution::now(),
                )
                .await
            {
                Ok(snapshot) => snapshot,
                Err(_) => continue,
            };
            if !saved
                .review
                .allows_current(&snapshot.artifact, snapshot.version, snapshot.revision)
                || !request.question.matches(&snapshot.artifact)
            {
                continue;
            }
            let hash = snapshot.artifact.hash().map_err(invalid)?;
            if seen.insert(hash) {
                accepted.push((saved, snapshot));
            }
            if accepted.len() == contract::MAX_HEADERS {
                break;
            }
        }
        let serving_more = scanned < rows.len();
        let mut retained=None;
        let mut next_stage=custody_stage;
        let mut next_before=last;
        let mut more=serving_more;
        if !serving_more && custody_upper>0 {
            next_stage=true;
            let start=if custody_stage {before}else{None};
            let remaining_work=contract::PAGE_WORK.saturating_sub(scanned);
            let remaining_headers=contract::MAX_HEADERS.saturating_sub(accepted.len());
            if remaining_work>0 && remaining_headers>0 {
                let page=self.discover_custody_records(source,node,&request.question,custody_upper,start,remaining_work,remaining_headers,&seen,now).await?;
                more=page.more;next_before=page.last;retained=Some(page);
            } else {more=true;next_before=start;}
        }
        let next = if more {
            Some(
                self.encode_read_cursor("evidence-discovery", &principal, &(upper,custody_upper,next_stage,next_before), now)
                    .await?,
            )
        } else {
            None
        };
        let reader = ServingReader {
            endpoint: source,
            organ: &peer.organ,
            authority: &authority,
            epoch,
        };
        let mut tx = self.store.pool.begin().await?;
        self.social_ask_peer_on(&mut tx, source, &peer.organ, false)
            .await?;
        self.custody_peer_unchanged_on(&mut tx,source,&peer.organ,&authority).await?;
        if *self.watch_query_changes().borrow()!=epoch {return Err(invalid("Evidence discovery permissions changed during collection"));}
        let mut headers = Vec::new();
        for (saved, snapshot) in accepted {
            self.validate_served_evidence_on(&mut tx, &saved, &snapshot, &reader)
                .await?;
            headers.push(Header {
                hash: snapshot.artifact.hash().map_err(invalid)?,
                kind: snapshot.artifact.kind(),
                bytes: snapshot.artifact.bytes().map_err(invalid)?.len() as u32,
                expires_at: saved
                    .review
                    .expires_at
                    .min(snapshot.artifact.expires_at().map_err(invalid)?),
            });
        }
        if let Some(retained)=retained {
            for saved in retained.headers {
                self.validate_custody_header_on(&mut tx,&saved,&peer.organ,nucleus::execution::now().timestamp()).await?;
                if !headers.iter().any(|header|header.hash==saved.header.hash) {headers.push(saved.header);}
            }
        }
        let page = Page {
            id: request.id.clone(),
            question,
            headers,
            next,
            partial: more,
        };
        page.validate(request, nucleus::execution::now().timestamp())
            .map_err(invalid)?;
        Ok(json!({"service":node,"page":page}))
    }
}
