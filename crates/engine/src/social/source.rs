use super::*;
use store::sqlx::Row;

pub(super) const MAX_OWN_POSTS: i64 = 256;

pub(super) fn trim_publication_state(state: &mut Value) -> Result<(), EngineError> {
    let current_draft = nucleus::fact::sha256_hex(&serde_json::to_vec(&state["draft"])?);
    let object = state
        .as_object_mut()
        .ok_or_else(|| invalid("Invalid private publication state"))?;
    let mut drafts: Vec<_> = object
        .keys()
        .filter(|key| key.starts_with("draft_"))
        .cloned()
        .collect();
    drafts.sort_by_key(|key| (key != &format!("draft_{current_draft}"), key.clone()));
    for key in drafts.into_iter().skip(32) {
        object.remove(&key);
    }
    let mut ancestors = std::collections::HashSet::new();
    let mut revisions = Vec::new();
    for (key, doc) in object
        .iter()
        .filter(|(key, _)| key.starts_with("revision_"))
    {
        if let Some(parent) = doc["parent"].as_str() {
            ancestors.insert(parent.to_owned());
        }
        ancestors.extend(
            doc["resolves"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(str::to_owned),
        );
        revisions.push((
            key.clone(),
            doc["revision"]
                .as_str()
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(0),
        ));
    }
    revisions.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let mut retained = 0;
    for (key, _) in revisions {
        let hash = key.strip_prefix("revision_").unwrap();
        if !ancestors.contains(hash) {
            continue;
        }
        retained += 1;
        if retained > 32 {
            object.remove(&key);
        }
    }
    if serde_json::to_vec(state)?.len() > 512 * 1024 {
        return Err(invalid(
            "The retained publication state is full. Resolve its concurrent branches or archive this ended post",
        ));
    }
    Ok(())
}

impl Engine {
    pub(crate) async fn social_require_safe_generic_deletion(
        &self,
        uid: &str,
    ) -> Result<(), EngineError> {
        let retained: bool = store::sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record_extension WHERE record_uid=? AND namespace IN ('lince.social.publication','lince.social.session-authority','lince.social.participants','lince.social.request-draft'))").bind(uid).fetch_one(&self.store.pool).await?;
        if retained {
            return Err(invalid(
                "Use My posts or Requests to close and archive this social item. Ordinary deletion would strand publication or private reply authority",
            ));
        }
        Ok(())
    }

    pub(super) async fn social_publications(&self) -> Result<Vec<(String, Value)>, EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let rows = store::sqlx::query("SELECT e.record_uid,e.fds FROM record_extension e JOIN record r ON r.uid=e.record_uid WHERE e.namespace=? AND r.organ_uid=? AND r.deleted_at IS NULL AND COALESCE(json_extract(e.fds,'$.archived'),0)=0 ORDER BY e.record_uid LIMIT ?")
            .bind(PUBLICATION_NAMESPACE).bind(&organ.uid).bind(MAX_OWN_POSTS + 1)
            .fetch_all(&self.store.pool).await?;
        if rows.len() > MAX_OWN_POSTS as usize {
            return Err(invalid(
                "The device supports 256 retained social posts. Archive an ended post before adding another",
            ));
        }
        rows.into_iter()
            .map(|row| {
                let body: String = row.get("fds");
                if body.len() > 512 * 1024 {
                    return Err(invalid(
                        "A retained social publication exceeds the private state bound",
                    ));
                }
                Ok((row.get("record_uid"), serde_json::from_str(&body)?))
            })
            .collect()
    }

    pub(super) async fn social_source_draft(
        &self,
        source: &str,
        actor: Option<&str>,
    ) -> Result<(String, PostDraft), EngineError> {
        let promise = store::misc::get_promise(&self.store.pool, source).await?;
        if promise
            .as_ref()
            .is_some_and(|p| p.state != nucleus::PromiseState::Open)
        {
            return Err(invalid(
                "Only an OPEN promise can prepare a public announcement",
            ));
        }
        let uid = match &promise {
            Some(p) => p
                .record_uid
                .clone()
                .ok_or_else(|| invalid("Choose an OPEN promise attached to a readable Record"))?,
            None => self.resolve(source).await?,
        };
        self.social_own_record(&uid, actor).await?;
        let record = store::records::get(&self.store.pool, &uid)
            .await?
            .ok_or_else(|| invalid("Missing source Record"))?;
        if record.kind != "plain" {
            return Err(invalid(
                "Choose a plain Need/Contribution Record; conversations, people and executable Records cannot be projected",
            ));
        }
        let quantity = promise
            .as_ref()
            .map(|p| store::exact::from_f64(p.delta))
            .unwrap_or(record.quantity);
        let unit_uid = promise
            .as_ref()
            .and_then(|p| p.unit_uid.as_deref())
            .or(record.unit_uid.as_deref());
        let unit = match unit_uid {
            Some(unit) => store::concepts::canonical_name(&self.store.pool, unit).await?,
            None => None,
        };
        let mut draft = PostDraft {
            title: record
                .head
                .chars()
                .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
                .take(160)
                .collect(),
            text: record
                .body
                .chars()
                .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
                .take(1200)
                .collect(),
            direction: if quantity.is_negative() {
                Direction::Need
            } else {
                Direction::Contribution
            },
            unit,
            ..Default::default()
        };
        if draft.unit.is_some() && !quantity.is_zero() {
            let amount = if quantity.is_negative() {
                quantity.checked_neg().ok_or_else(|| {
                    invalid("The source quantity cannot be represented as a positive public amount")
                })?
            } else {
                quantity
            };
            draft.quantity = Some(amount.to_string());
        }
        Ok((
            promise.as_ref().map(|p| p.uid.clone()).unwrap_or(uid),
            draft,
        ))
    }

    pub(super) async fn social_refresh_sources(&self) -> Result<(), EngineError> {
        if self.social_require_local_write().await.is_err() {
            return Ok(());
        }
        for (uid, state) in self.social_publications().await? {
            let Some(source) = state["source"].as_str() else {
                continue;
            };
            let projected = self.social_source_draft(source, None).await;
            let (fingerprint, candidate, error) = match projected {
                Ok((_, projection)) => {
                    let fingerprint = nucleus::fact::sha256_hex(&serde_json::to_vec(&projection)?);
                    if state["source_fingerprint"] == fingerprint && state["source_error"].is_null()
                    {
                        continue;
                    }
                    let mut candidate: PostDraft = serde_json::from_value(state["draft"].clone())?;
                    candidate.title = projection.title;
                    candidate.text = projection.text;
                    candidate.direction = projection.direction;
                    candidate.quantity = projection.quantity;
                    candidate.unit = projection.unit;
                    (
                        json!(fingerprint),
                        serde_json::to_value(candidate)?,
                        Value::Null,
                    )
                }
                Err(_) => {
                    let error = json!(
                        "Source unavailable, closed or no longer eligible. Your saved public draft is unchanged; review, pause or withdraw it yourself"
                    );
                    if state["source_error"] == error {
                        continue;
                    }
                    (state["source_fingerprint"].clone(), Value::Null, error)
                }
            };
            let mut tx = self.social_write_tx().await?;
            let body: Option<String> = store::sqlx::query_scalar(
                "SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?",
            )
            .bind(&uid)
            .bind(PUBLICATION_NAMESPACE)
            .fetch_optional(&mut *tx)
            .await?;
            let Some(body) = body else { continue };
            let mut current: Value = serde_json::from_str(&body)?;
            if current["source"] != state["source"] || current["draft"] != state["draft"] {
                continue;
            }
            current["source_fingerprint"] = fingerprint;
            current["source_draft"] = candidate;
            current["source_error"] = error;
            store::records::set_extension_on(&mut tx, &uid, PUBLICATION_NAMESPACE, &current)
                .await?;
            tx.commit().await?;
        }
        Ok(())
    }
}
