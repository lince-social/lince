use super::*;
use serde::{Deserialize, Serialize};

const MUTE_NAMESPACE: &str = "lince.social.mutes";
const MAX_MUTES: usize = 256;
const MAX_REMOVALS: i64 = 10_000;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Mute {
    kind: String,
    target: String,
    title: String,
}

fn author(document: &Snippet) -> Result<(String, String), EngineError> {
    match document.mode {
        AuthorMode::Anonymous => Ok((
            "anonymous-author".into(),
            document
                .anonymous
                .as_ref()
                .ok_or_else(|| invalid("Missing anonymous posting authority"))?
                .owner_key
                .clone(),
        )),
        AuthorMode::Identified => Ok((
            "public-organ".into(),
            document
                .profile
                .as_ref()
                .ok_or_else(|| invalid("Missing identified posting authority"))?
                .organ
                .clone(),
        )),
    }
}

fn mute_key(kind: &str, target: &str) -> String {
    format!(
        "mute_{}",
        nucleus::fact::sha256_hex(format!("{kind}\n{target}").as_bytes())
    )
}

fn valid_mute_key(key: &str) -> bool {
    key.len() == 69 && key.starts_with("mute_") && key[5..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn validate_map(map: &Value) -> Result<Vec<Mute>, EngineError> {
    let fields = map
        .as_object()
        .ok_or_else(|| invalid("Invalid private mute settings"))?;
    if fields.len() > MAX_MUTES || serde_json::to_vec(map)?.len() > 64 * 1024 {
        return Err(invalid(
            "Resolve private mute settings within 256 entries and 64 KiB",
        ));
    }
    let mut entries = Vec::new();
    for (key, value) in fields {
        let entry: Mute = serde_json::from_value(value.clone())?;
        let valid = match entry.kind.as_str() {
            "post" => nucleus::valid_uid(&entry.target, "post"),
            "anonymous-author" => request_auth::ed_key(&entry.target).is_ok(),
            "public-organ" => nucleus::valid_uid(&entry.target, "r"),
            _ => false,
        };
        if !valid
            || *key != mute_key(&entry.kind, &entry.target)
            || text(&entry.title, 160, true).is_err()
        {
            return Err(invalid("Invalid retained public mute target"));
        }
        entries.push(entry);
    }
    Ok(entries)
}

fn filter_rows(map: &Value, rows: &mut Vec<Value>) -> Result<(), EngineError> {
    let targets: std::collections::HashSet<(String, String)> = validate_map(map)?
        .into_iter()
        .map(|entry| (entry.kind, entry.target))
        .collect();
    let mut visible = Vec::new();
    for row in rows.drain(..) {
        let document: Snippet = serde_json::from_value(row["document"].clone())?;
        if !targets.contains(&("post".into(), document.id.clone()))
            && !targets.contains(&author(&document)?)
        {
            visible.push(row);
        }
    }
    *rows = visible;
    Ok(())
}

impl Engine {
    pub(super) async fn social_mutes(&self, after: Option<&str>) -> Result<Value, EngineError> {
        if after.is_some_and(|key| !valid_mute_key(key)) {
            return Err(invalid("Invalid hidden-target page"));
        }
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let map = store::records::get_extension(&self.store.pool, &organ.uid, MUTE_NAMESPACE)
            .await?
            .unwrap_or_else(|| json!({}));
        let fields = map
            .as_object()
            .ok_or_else(|| invalid("Invalid private mute settings"))?;
        let over_limit = fields.len() > MAX_MUTES || serde_json::to_vec(&map)?.len() > 64 * 1024;
        let page: serde_json::Map<String, Value> = fields
            .iter()
            .filter(|(key, _)| key.as_str() > after.unwrap_or(""))
            .take(32)
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        validate_map(&Value::Object(page.clone()))?;
        let next = page
            .keys()
            .last()
            .filter(|last| fields.keys().any(|key| key > *last));
        let status = if over_limit {
            "Merged own-device settings exceed 256 targets or 64 KiB. Show targets again until within the limit; signed cache evidence remains retained"
        } else {
            "Hidden announcements follow your owned devices. Anonymous author hiding follows its public pseudonym, without linking it to an Organ identity"
        };
        Ok(
            json!({"mutes":page,"total":fields.len(),"over_limit":over_limit,"next_after":next,"can_manage_services":self.social_require_local_write().await.is_ok(),"status":status}),
        )
    }

    pub(super) async fn social_mute_post(
        &self,
        post: &str,
        whole_author: bool,
    ) -> Result<Value, EngineError> {
        if !nucleus::valid_uid(post, "post") {
            return Err(invalid("Choose a public announcement"));
        }
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
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
        let (kind, target) = if whole_author {
            author(&document)?
        } else {
            ("post".into(), post.to_owned())
        };
        let mut map = owner::extension_on(&mut tx, &organ.uid, MUTE_NAMESPACE).await?;
        validate_map(&map)?;
        let key = mute_key(&kind, &target);
        map[key] = serde_json::to_value(Mute {
            kind,
            target,
            title: document.title,
        })?;
        validate_map(&map)?;
        store::records::set_extension_on(&mut tx, &organ.uid, MUTE_NAMESPACE, &map).await?;
        tx.commit().await?;
        self.notify_query_changed();
        self.social_mutes(None).await
    }

    pub(super) async fn social_unmute(&self, key: &str) -> Result<Value, EngineError> {
        if !valid_mute_key(key) {
            return Err(invalid("Choose a retained hidden target"));
        }
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let mut tx = self.social_write_tx().await?;
        let mut map = owner::extension_on(&mut tx, &organ.uid, MUTE_NAMESPACE).await?;
        map.as_object_mut()
            .ok_or_else(|| invalid("Invalid private mute map"))?
            .remove(key)
            .ok_or_else(|| invalid("This target is no longer hidden"))?;
        store::records::set_extension_on(&mut tx, &organ.uid, MUTE_NAMESPACE, &map).await?;
        tx.commit().await?;
        self.notify_query_changed();
        self.social_mutes(None).await
    }

    pub(super) async fn social_filter_local_results(
        &self,
        rows: &mut Vec<Value>,
    ) -> Result<(), EngineError> {
        let organ = store::organs::local(&self.store.pool)
            .await?
            .ok_or_else(|| invalid("No local Organ"))?;
        let map = store::records::get_extension(&self.store.pool, &organ.uid, MUTE_NAMESPACE)
            .await?
            .unwrap_or_else(|| json!({}));
        filter_rows(&map, rows)
    }

    pub(super) async fn social_filter_local_results_on(
        &self,
        tx: &mut store::sqlx::Transaction<'_, store::sqlx::Sqlite>,
        rows: &mut Vec<Value>,
    ) -> Result<(), EngineError> {
        let organ: String = store::sqlx::query_scalar(
            "SELECT uid FROM record WHERE slug=? AND kind=? AND deleted_at IS NULL LIMIT 1",
        )
        .bind(store::organs::LOCAL_ORGAN_SLUG)
        .bind(nucleus::RecordKind::Organ.as_str())
        .fetch_one(&mut **tx)
        .await?;
        let map = owner::extension_on(tx, &organ, MUTE_NAMESPACE).await?;
        filter_rows(&map, rows)
    }

    pub(super) async fn social_remove_listing(
        &self,
        post: &str,
        reason: &str,
    ) -> Result<Value, EngineError> {
        if !nucleus::valid_uid(post, "post") {
            return Err(invalid("Choose a public announcement"));
        }
        text(reason, 500, true).map_err(invalid)?;
        let mut tx = self.social_write_tx().await?;
        let exists: bool = store::sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM social_document WHERE kind='snippet' AND id=?)",
        )
        .bind(post)
        .fetch_one(&mut *tx)
        .await?;
        if !exists {
            return Err(invalid("Choose a retained public listing to review"));
        }
        let count: i64 =
            store::sqlx::query_scalar("SELECT COUNT(*) FROM social_listing_removal WHERE post<>?")
                .bind(post)
                .fetch_one(&mut *tx)
                .await?;
        if count >= MAX_REMOVALS {
            return Err(invalid("The host's 10,000-entry removal list is full"));
        }
        store::sqlx::query("INSERT INTO social_listing_removal(post,reason,removed_at) VALUES(?,?,?) ON CONFLICT(post) DO UPDATE SET reason=excluded.reason,removed_at=excluded.removed_at")
            .bind(post).bind(reason).bind(nucleus::execution::now().timestamp()).execute(&mut *tx).await?;
        store::sqlx::query("DELETE FROM social_search WHERE id=?")
            .bind(post)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.notify_query_changed();
        Ok(
            json!({"status":"Listing removed on this host. Signed bytes and ending evidence remain; future revisions stay removed until deliberate restoration"}),
        )
    }

    pub(super) async fn social_restore_listing(&self, post: &str) -> Result<Value, EngineError> {
        if !nucleus::valid_uid(post, "post") {
            return Err(invalid("Choose a removed public listing"));
        }
        let mut tx = self.social_write_tx().await?;
        let removed = store::sqlx::query("DELETE FROM social_listing_removal WHERE post=?")
            .bind(post)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if removed == 0 {
            return Err(invalid("This listing is not removed"));
        }
        let body: String = store::sqlx::query_scalar(
            "SELECT body FROM social_document WHERE kind='snippet' AND id=? AND state='active'",
        )
        .bind(post)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| invalid("Only a retained active listing can be restored"))?;
        let document: Snippet = serde_json::from_str(&body)?;
        if document.id != post || !ask::allowed_on(&mut tx, &document).await? {
            return Err(invalid(
                "Only a currently valid active listing can be restored",
            ));
        }
        store::sqlx::query("DELETE FROM social_search WHERE id=?")
            .bind(post)
            .execute(&mut *tx)
            .await?;
        store::sqlx::query("INSERT INTO social_search(id,title,text) VALUES(?,?,?)")
            .bind(post)
            .bind(&document.title)
            .bind(&document.text)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        self.notify_query_changed();
        Ok(
            json!({"status":"This host's listing was restored after checking its current signature, expiry and authority floors"}),
        )
    }

    pub(super) async fn social_removed_listings(
        &self,
        after: Option<&str>,
    ) -> Result<Value, EngineError> {
        if after.is_some_and(|id| !nucleus::valid_uid(id, "post")) {
            return Err(invalid("Invalid removal page"));
        }
        let rows: Vec<(String,String,i64,Option<String>)> = store::sqlx::query_as("SELECT m.post,m.reason,m.removed_at,d.body FROM social_listing_removal m LEFT JOIN social_document d ON d.kind='snippet' AND d.id=m.post WHERE m.post>? ORDER BY m.post LIMIT 32")
            .bind(after.unwrap_or("")).fetch_all(&self.store.pool).await?;
        let mut results = Vec::new();
        let mut bytes = 0;
        for (post, reason, time, body) in rows {
            let document: Option<Value> = body.as_deref().map(serde_json::from_str).transpose()?;
            let row = json!({"post":post,"reason":reason,"removed_at":time,"document":document});
            let size = serde_json::to_vec(&row)?.len();
            if bytes + size > MAX_FRAME_BYTES - 1024 {
                break;
            }
            bytes += size;
            results.push(row);
        }
        let next = results.last().and_then(|row| row["post"].as_str());
        Ok(
            json!({"removed_listings":results,"next_after":next,"can_manage_services":self.social_require_local_write().await.is_ok(),"status":"Host-local removals preserve signed evidence. Other operators make independent decisions"}),
        )
    }
}
