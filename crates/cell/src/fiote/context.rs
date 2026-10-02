use super::*;

fn messages(messages: &[Message]) -> Vec<serde_json::Value> {
    messages.iter().map(|message| match message {
        Message::RichUser { text, content } | Message::RichAssistant { text, content } => {
            let attachments: Vec<_> = content.iter().map(|part| match part {
                nucleus::message::MessagePart::Attachment { name, mime_type, data } => serde_json::json!({"kind":"attachment","name":name,"mime_type":mime_type,"encoded_bytes":data.len(),"binary_content":"Inspect the source Message attachment"}),
                part => serde_json::to_value(part).unwrap_or_default(),
            }).collect();
            serde_json::json!({"role":if matches!(message, Message::RichUser { .. }) { "user" } else { "assistant" },"text":text,"attachments":attachments})
        }
        message => serde_json::to_value(message).unwrap_or_default(),
    }).collect()
}

pub(super) fn supplied(path: &Path, input: &[Message], source: Option<&str>) -> Result<(), String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() > 1024 * 1024 {
        return Err("Context inspection exceeds 1 MiB".into());
    }
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    value["scope"] = serde_json::json!(
        "Latest model request supplied by Lince, including tool calls/results; provider internal memory is not exposed"
    );
    value["messages"] = serde_json::json!(messages(input));
    value["source_message"] = serde_json::json!(source);
    value["supplied_at"] = serde_json::json!(nucleus::execution::now().to_rfc3339());
    if serde_json::to_vec(&value).map_err(|e| e.to_string())?.len() > 1024 * 1024 {
        return Err("Context inspection exceeds 1 MiB".into());
    }
    save(path, &value)
}

impl Host {
    pub(super) async fn save_context(
        &self,
        thread: &str,
        record: &str,
        messages: &[Message],
        tools: &Registry,
    ) -> Result<(), String> {
        if !nucleus::valid_uid(thread, "r") {
            return Err("Invalid context thread".into());
        }
        let task =
            store::records::get_extension(&self.engine.store.pool, thread, "lince.fiote-task")
                .await
                .map_err(|e| e.to_string())?;
        let activation = store::records::get_extension(
            &self.engine.store.pool,
            thread,
            "lince.fiote-activation",
        )
        .await
        .map_err(|e| e.to_string())?;
        let messages = self::messages(messages);
        let value = serde_json::json!({"thread":thread,"fiote":record,"scope":"Starting input supplied by Lince, not the provider's full internal memory or subsequent tool results","messages":messages,"tools":tools.definitions().into_iter().map(|tool| tool.name).collect::<Vec<_>>(),"task":task,"activation":activation,"history_window":12,"saved_at":nucleus::execution::now().to_rfc3339(),"external_memory":"not exposed"});
        if serde_json::to_vec(&value).map_err(|e| e.to_string())?.len() > 1024 * 1024 {
            return Err("Context inspection exceeds 1 MiB".into());
        }
        save(
            &self.directory.join(format!("context-{thread}.json")),
            &value,
        )
    }

    pub(super) async fn inspect_context(
        &self,
        thread: &str,
    ) -> Result<Option<serde_json::Value>, String> {
        if !nucleus::valid_uid(thread, "r") {
            return Err("Invalid context thread".into());
        }
        let mut context = match std::fs::read(self.directory.join(format!("context-{thread}.json")))
        {
            Ok(bytes) if bytes.len() <= 1024 * 1024 => {
                serde_json::from_slice::<serde_json::Value>(&bytes).map_err(|e| e.to_string())?
            }
            Ok(_) => return Err("Context inspection exceeds 1 MiB".into()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                serde_json::json!({"scope":"No starting-context snapshot is available for this session; external internal memory is not exposed"})
            }
            Err(error) => return Err(error.to_string()),
        };
        let rows: Vec<(String, String)> = store::sqlx::query_as("SELECT r.uid, e.fds FROM record_extension e JOIN record r ON r.uid = e.record_uid WHERE e.namespace = 'lince.fiote-child' AND json_extract(e.fds, '$.parent') = ? AND r.deleted_at IS NULL ORDER BY r.uid LIMIT 128").bind(thread).fetch_all(&self.engine.store.pool).await.map_err(|e| e.to_string())?;
        let running = self.running.lock().await;
        let mut children = Vec::new();
        for (uid, value) in rows {
            let mut child: serde_json::Value =
                serde_json::from_str(&value).map_err(|e| e.to_string())?;
            child["thread"] = serde_json::json!(uid);
            child["source"] = serde_json::json!("reported child session");
            child["context_available"] =
                serde_json::json!(self.directory.join(format!("context-{uid}.json")).is_file());
            if running.contains_key(&uid) {
                child["state"] = serde_json::json!("working");
                child["source"] = serde_json::json!("Lince runtime");
            }
            children.push(child);
        }
        context["children"] = serde_json::json!(children);
        Ok(Some(context))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inspecting_large_attachments_preserves_metadata_without_copying_binary_content() {
        let snapshot = messages(&[Message::RichUser {
            text: "Inspect this receipt".into(),
            content: vec![nucleus::message::MessagePart::Attachment {
                name: "receipt.jpg".into(),
                mime_type: "image/jpeg".into(),
                data: "AQID".repeat(500_000),
            }],
        }]);
        assert_eq!(snapshot[0]["attachments"][0]["name"], "receipt.jpg");
        assert_eq!(snapshot[0]["attachments"][0]["encoded_bytes"], 2_000_000);
        assert!(snapshot[0]["attachments"][0].get("data").is_none());
        assert!(serde_json::to_vec(&snapshot).unwrap().len() < 1024);
    }
}
