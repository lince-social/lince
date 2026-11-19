use nucleus::message::{self, MessagePart, StoredPart};
use serde_json::json;
use sqlx::{Error, Sqlite, SqliteConnection, SqlitePool, Transaction};

fn invalid(error: impl ToString) -> Error {
    Error::Protocol(error.to_string())
}

pub async fn save(pool: &SqlitePool, message: &str, parts: &[MessagePart]) -> Result<(), Error> {
    let mut transaction = crate::write_tx(pool).await?;
    save_on(&mut transaction, message, parts).await?;
    transaction.commit().await
}

pub async fn save_on(
    transaction: &mut Transaction<'_, Sqlite>,
    message: &str,
    parts: &[MessagePart],
) -> Result<(), Error> {
    let descriptors = message::describe(parts).map_err(invalid)?;
    for (index, part) in parts.iter().enumerate() {
        if let MessagePart::Attachment { data, .. } = part {
            if data.is_empty() {
                crate::records::set_extension_on(
                    transaction,
                    message,
                    &message::chunk_namespace(index, 0),
                    &json!({"data":""}),
                )
                .await?;
            }
            for (chunk, bytes) in data.as_bytes().chunks(message::CHUNK_BYTES).enumerate() {
                let data = std::str::from_utf8(bytes).map_err(invalid)?;
                crate::records::set_extension_on(
                    transaction,
                    message,
                    &message::chunk_namespace(index, chunk),
                    &json!({"data":data}),
                )
                .await?;
            }
        }
    }
    crate::records::set_extension_on(
        transaction,
        message,
        "lince.message-content",
        &json!({"parts":descriptors}),
    )
    .await?;
    Ok(())
}

pub async fn load(pool: &SqlitePool, uid: &str) -> Result<Vec<MessagePart>, Error> {
    let mut connection = pool.acquire().await?;
    load_on(&mut connection, uid).await
}

async fn extension_on(
    connection: &mut SqliteConnection,
    uid: &str,
    namespace: &str,
) -> Result<Option<serde_json::Value>, Error> {
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT substr(fds, 1, ?) FROM record_extension WHERE record_uid=? AND namespace=?",
    )
    .bind((crate::records::MAX_EXTENSION_BYTES + 1) as i64)
    .bind(uid)
    .bind(namespace)
    .fetch_optional(connection)
    .await?;
    raw.map(extension_value).transpose()
}

fn extension_value(raw: String) -> Result<serde_json::Value, Error> {
    if raw.len() > crate::records::MAX_EXTENSION_BYTES {
        return Err(invalid("Record extension exceeds its byte limit."));
    }
    serde_json::from_str(&raw).map_err(invalid)
}

async fn attachment_on(
    connection: &mut SqliteConnection,
    uid: &str,
    part: usize,
    chunks: usize,
) -> Result<String, Error> {
    if chunks == 0 {
        return Ok(String::new());
    }
    let _read = nucleus::performance::Span::new("store-message-attachment-chunks-await");
    let requested: Vec<_> = (0..chunks)
        .map(|chunk| message::chunk_namespace(part, chunk))
        .collect();
    let requested = serde_json::to_string(&requested).map_err(invalid)?;
    let mut rows = sqlx::query_scalar::<_, Option<String>>(
        "SELECT substr(chunk.fds, 1, ?)
         FROM json_each(?) requested
         LEFT JOIN record_extension chunk
           ON chunk.record_uid = ? AND chunk.namespace = requested.value
         ORDER BY CAST(requested.key AS INTEGER)",
    )
    .bind((crate::records::MAX_EXTENSION_BYTES + 1) as i64)
    .bind(requested)
    .bind(uid)
    .fetch(connection);
    let mut data = String::new();
    while let Some(raw) = std::future::poll_fn(|context| rows.as_mut().poll_next(context)).await {
        let value = extension_value(raw?.ok_or_else(|| {
            invalid("This attachment has not arrived or is no longer available.")
        })?)?;
        let piece = value["data"]
            .as_str()
            .ok_or_else(|| invalid("Invalid attachment chunk."))?;
        if piece.len() > message::CHUNK_BYTES {
            return Err(invalid("Attachment chunk exceeds its size limit."));
        }
        data.push_str(piece);
        nucleus::performance::add("message-attachment-chunk-rows", 1);
        nucleus::performance::add("message-attachment-encoded-bytes", piece.len() as u64);
    }
    Ok(data)
}

pub async fn load_on(
    connection: &mut SqliteConnection,
    uid: &str,
) -> Result<Vec<MessagePart>, Error> {
    let Some(mut value) = extension_on(connection, uid, "lince.message-content").await? else {
        return Ok(Vec::new());
    };
    let descriptors: Vec<StoredPart> = serde_json::from_value(
        value
            .get_mut("parts")
            .map(serde_json::Value::take)
            .unwrap_or_default(),
    )
    .map_err(invalid)?;
    if descriptors.len() > message::MAX_PARTS {
        return Err(invalid("Too many message parts."));
    }
    let mut restored = message::Restoration::default();
    for (index, part) in descriptors.into_iter().enumerate() {
        let part = restored.part(part).map_err(invalid)?;
        let data = if let Some(chunks) = part.chunks() {
            Some(attachment_on(connection, uid, index, chunks).await?)
        } else {
            None
        };
        part.finish(data).map_err(invalid)?;
    }
    restored.finish().map_err(invalid)
}

pub async fn replace(
    pool: &SqlitePool,
    message: &str,
    expected: &serde_json::Value,
    value: &serde_json::Value,
) -> Result<(), Error> {
    let mut transaction = crate::write_tx(pool).await?;
    let current: Option<String> = sqlx::query_scalar("SELECT fds FROM record_extension WHERE record_uid = ? AND namespace = 'lince.message-content'").bind(message).fetch_optional(&mut *transaction).await?;
    let current = current
        .map(|value| serde_json::from_str::<serde_json::Value>(&value))
        .transpose()
        .map_err(invalid)?
        .unwrap_or_default();
    if &current != expected {
        return Err(invalid(
            "The message changed while this response was being saved. Reload it before answering.",
        ));
    }
    crate::records::set_extension_on(&mut transaction, message, "lince.message-content", value)
        .await?;
    transaction.commit().await
}
