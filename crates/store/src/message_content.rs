use nucleus::message::{self, MessagePart, StoredPart};
use serde_json::json;
use sqlx::{Error, SqlitePool};

fn invalid(error: impl ToString) -> Error {
    Error::Protocol(error.to_string())
}

pub async fn save(pool: &SqlitePool, message: &str, parts: &[MessagePart]) -> Result<(), Error> {
    let descriptors = message::describe(parts).map_err(invalid)?;
    for (index, part) in parts.iter().enumerate() {
        if let MessagePart::Attachment { data, .. } = part {
            if data.is_empty() {
                crate::records::set_extension(
                    pool,
                    message,
                    &message::chunk_namespace(index, 0),
                    &json!({"data":""}),
                )
                .await?;
            }
            for (chunk, bytes) in data.as_bytes().chunks(message::CHUNK_BYTES).enumerate() {
                let data = std::str::from_utf8(bytes).map_err(invalid)?;
                crate::records::set_extension(
                    pool,
                    message,
                    &message::chunk_namespace(index, chunk),
                    &json!({"data":data}),
                )
                .await?;
            }
        }
    }
    crate::records::set_extension(
        pool,
        message,
        "lince.message-content",
        &json!({"parts":descriptors}),
    )
    .await
}

pub async fn load(pool: &SqlitePool, uid: &str) -> Result<Vec<MessagePart>, Error> {
    let Some(value) = crate::records::get_extension(pool, uid, "lince.message-content").await?
    else {
        return Ok(Vec::new());
    };
    let descriptors: Vec<StoredPart> =
        serde_json::from_value(value["parts"].clone()).map_err(invalid)?;
    if descriptors.len() > message::MAX_PARTS {
        return Err(invalid("Too many message parts."));
    }
    let mut parts = Vec::new();
    let mut total = 0;
    for (index, part) in descriptors.into_iter().enumerate() {
        parts.push(match part {
            StoredPart::Question { question } => MessagePart::Question { question },
            StoredPart::Steps { steps } => MessagePart::Steps { steps },
            StoredPart::Text { text } => MessagePart::Text { text },
            StoredPart::Reference { name, uri } => MessagePart::Reference { name, uri },
            StoredPart::Attachment {
                name,
                mime_type,
                bytes,
                sha256,
                chunks,
            } => {
                total += bytes;
                if total > message::MAX_CONTENT_BYTES || chunks > 24 {
                    return Err(invalid("Message attachments exceed their size limit."));
                }
                let mut data = String::new();
                for chunk in 0..chunks {
                    let value = crate::records::get_extension(
                        pool,
                        uid,
                        &message::chunk_namespace(index, chunk),
                    )
                    .await?
                    .ok_or_else(|| {
                        invalid("This attachment has not arrived or is no longer available.")
                    })?;
                    let piece = value["data"]
                        .as_str()
                        .ok_or_else(|| invalid("Invalid attachment chunk."))?;
                    if piece.len() > message::CHUNK_BYTES {
                        return Err(invalid("Attachment chunk exceeds its size limit."));
                    }
                    data.push_str(piece);
                }
                let decoded = message::decode(&data).map_err(invalid)?;
                if decoded.len() != bytes || message::digest(&decoded) != sha256 {
                    return Err(invalid(
                        "The attachment is incomplete or its contents changed.",
                    ));
                }
                MessagePart::Attachment {
                    name,
                    mime_type,
                    data,
                }
            }
        });
    }
    message::validate(&parts).map_err(invalid)?;
    Ok(parts)
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
