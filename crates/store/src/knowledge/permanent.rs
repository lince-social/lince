use super::{StoreError, invalid};
use nucleus::knowledge::Item;
use serde_json::{Value, json};
use sha2::{Digest, Sha384};
use sqlx::{Row, Sqlite, Transaction};

pub const KNOWLEDGE_NAMESPACE: &str = "lince.knowledge";
pub const REFERENCE_NAMESPACE: &str = "lince.knowledge.reference";
pub const IMPORT_NAMESPACE: &str = "lince.knowledge.import";
pub const BUNDLE_LIMIT: usize = 16 * 1024 * 1024;

pub fn fingerprint(bytes: &[u8]) -> String {
    Sha384::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub async fn imported_copy_on(
    tx: &mut Transaction<'_, Sqlite>,
    organ: &str,
    origin: &str,
    record: &str,
) -> Result<Option<String>, StoreError> {
    let rows:Vec<(String,Option<String>)>=sqlx::query_as("SELECT r.uid,r.deleted_at FROM record r JOIN record_extension e ON e.record_uid=r.uid WHERE r.organ_uid=? AND e.namespace=? AND json_extract(e.fds,'$.origin_organ')=? AND json_extract(e.fds,'$.record_uid')=? ORDER BY r.uid LIMIT 2").bind(organ).bind(IMPORT_NAMESPACE).bind(origin).bind(record).fetch_all(&mut **tx).await?;
    if rows.len() > 1 {
        return Err(invalid(
            "Choose one imported local copy before importing this knowledge again",
        ));
    }
    match rows.into_iter().next() {
        Some((_, Some(_))) => Err(invalid(
            "Restore the deleted local copy before importing this knowledge again",
        )),
        Some((uid, None)) => Ok(Some(uid)),
        None => Ok(None),
    }
}

pub async fn reference_uid_on(
    tx: &mut Transaction<'_, Sqlite>,
    organ: &str,
    source: &str,
    item: &str,
) -> Result<Option<String>, StoreError> {
    let synced:Option<String>=sqlx::query_scalar("SELECT r.uid FROM record r JOIN record_extension e ON e.record_uid=r.uid WHERE r.organ_uid=? AND e.namespace=? AND json_extract(e.fds,'$.source')=? AND json_extract(e.fds,'$.item_id')=? AND r.deleted_at IS NULL ORDER BY r.uid LIMIT 1").bind(organ).bind(REFERENCE_NAMESPACE).bind(source).bind(item).fetch_optional(&mut **tx).await?;
    if let Some(uid) = synced {
        sqlx::query("INSERT OR IGNORE INTO knowledge_reference(organ_uid,source_uid,item_id,record_uid) VALUES(?,?,?,?)").bind(organ).bind(source).bind(item).bind(uid).execute(&mut **tx).await?;
    }
    let row:Option<(String,Option<String>)>=sqlx::query_as("SELECT k.record_uid,r.deleted_at FROM knowledge_reference k JOIN record r ON r.uid=k.record_uid WHERE k.organ_uid=? AND k.source_uid=? AND k.item_id=?")
        .bind(organ).bind(source).bind(item).fetch_optional(&mut **tx).await?;
    match row {
        Some((_, Some(_))) => Err(invalid(
            "This saved reference was deleted; restore it before saving the source again",
        )),
        Some((uid, None)) => Ok(Some(uid)),
        None => Ok(None),
    }
}

pub async fn create_reference_on(
    tx: &mut Transaction<'_, Sqlite>,
    uid: &str,
    organ: &str,
    item: &Item,
) -> Result<String, StoreError> {
    if let Some(uid) = reference_uid_on(tx, organ, &item.source, &item.id).await? {
        return Ok(uid);
    }
    crate::records::create_with_uid_on(
        tx,
        uid,
        crate::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Plain,
            head: &item.title,
            body: "",
            quantity: crate::exact::zero(),
        },
        organ,
        None,
    )
    .await?;
    sqlx::query(
        "INSERT INTO knowledge_reference(organ_uid,source_uid,item_id,record_uid) VALUES(?,?,?,?)",
    )
    .bind(organ)
    .bind(&item.source)
    .bind(&item.id)
    .bind(uid)
    .execute(&mut **tx)
    .await?;
    crate::records::set_extension_on(tx,uid,REFERENCE_NAMESPACE,&json!({"source":item.source,"item_id":item.id,"uri":item.uri,"title":item.title,"first_revision":item.revision})).await?;
    Ok(uid.into())
}

pub async fn save_payload_on(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
    item: &Item,
    payload: &Value,
) -> Result<(), StoreError> {
    super::validate_item(item)?;
    let text = serde_json::to_string(payload).map_err(|error| invalid(&error.to_string()))?;
    if text.len() > BUNDLE_LIMIT {
        return Err(invalid(
            "A saved knowledge dependency bundle can contain at most 16 MiB",
        ));
    }
    let mut boundaries = vec![0];
    let mut last = 0;
    for (index, _) in text.char_indices() {
        if index - last >= 256 * 1024 {
            boundaries.push(index);
            last = index;
        }
    }
    boundaries.push(text.len());
    let count = boundaries.len() - 1;
    let previous: Option<String> =
        sqlx::query_scalar("SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?")
            .bind(record)
            .bind(KNOWLEDGE_NAMESPACE)
            .fetch_optional(&mut **tx)
            .await?;
    let old_count = previous
        .as_deref()
        .and_then(|raw| serde_json::from_str::<Value>(raw).ok())
        .and_then(|value| value["parts"].as_u64())
        .unwrap_or(0) as usize;
    for (index, span) in boundaries.windows(2).enumerate() {
        crate::records::set_extension_on(
            tx,
            record,
            &format!("lince.knowledge.part.{index:04}"),
            &json!({"text":&text[span[0]..span[1]]}),
        )
        .await?;
    }
    for index in count..old_count.min(64) {
        crate::records::set_extension_on(
            tx,
            record,
            &format!("lince.knowledge.part.{index:04}"),
            &json!({}),
        )
        .await?;
    }
    crate::records::set_extension_on(tx,record,KNOWLEDGE_NAMESPACE,&json!({"format":1,"parts":count,"sha384":fingerprint(text.as_bytes()),"source":item.source,"item_id":item.id,"revision":item.revision,"title":item.title,"uri":item.uri})).await?;
    Ok(())
}

pub async fn payload_on(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
) -> Result<Option<Value>, StoreError> {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?")
            .bind(record)
            .bind(KNOWLEDGE_NAMESPACE)
            .fetch_optional(&mut **tx)
            .await?;
    let Some(raw) = raw else {
        return Ok(None);
    };
    let manifest: Value =
        serde_json::from_str(&raw).map_err(|error| invalid(&error.to_string()))?;
    let count = manifest["parts"]
        .as_u64()
        .filter(|count| *count > 0 && *count <= 64)
        .ok_or_else(|| invalid("Saved knowledge has an invalid content manifest"))?;
    if manifest["format"] != 1 {
        return Err(invalid("Unsupported saved knowledge format"));
    }
    let mut text = String::new();
    for index in 0..count {
        let raw: String = sqlx::query_scalar(
            "SELECT fds FROM record_extension WHERE record_uid=? AND namespace=?",
        )
        .bind(record)
        .bind(format!("lince.knowledge.part.{index:04}"))
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| invalid("Some saved knowledge content has not arrived yet"))?;
        let part: Value =
            serde_json::from_str(&raw).map_err(|error| invalid(&error.to_string()))?;
        let part = part["text"]
            .as_str()
            .ok_or_else(|| invalid("Saved knowledge has an invalid content part"))?;
        if text.len() + part.len() > BUNDLE_LIMIT {
            return Err(invalid("Saved knowledge exceeds its dependency limits"));
        }
        text.push_str(part);
    }
    if manifest["sha384"].as_str() != Some(fingerprint(text.as_bytes()).as_str()) {
        return Err(invalid(
            "Saved knowledge content has not finished syncing or failed its integrity check",
        ));
    }
    Ok(Some(
        serde_json::from_str(&text).map_err(|error| invalid(&error.to_string()))?,
    ))
}

pub async fn record_metadata_on(
    tx: &mut Transaction<'_, Sqlite>,
    record: &str,
) -> Result<Value, StoreError> {
    let row = sqlx::query(
        "SELECT uid,head,body,organ_uid FROM record WHERE uid=? AND deleted_at IS NULL",
    )
    .bind(record)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| invalid("Saved knowledge Record is unavailable"))?;
    Ok(
        json!({"uid":row.get::<String,_>("uid"),"head":row.get::<String,_>("head"),"body":row.get::<String,_>("body"),"organ_uid":row.get::<Option<String>,_>("organ_uid")}),
    )
}
