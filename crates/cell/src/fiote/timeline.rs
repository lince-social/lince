use super::*;
use serde_json::{Value, json};

pub(super) struct Timeline<'a> {
    pub engine: Arc<Engine>,
    pub tools: &'a Registry,
    pub author: String,
    pub thread: String,
    pub pending: PathBuf,
    pub state: Mutex<State>,
}

pub(super) struct State {
    pub message: String,
    pub text: String,
    pub offset: usize,
    pub closed: bool,
    pub message_id: Option<String>,
    pub calls: HashMap<String, Call>,
}

pub(super) struct Call {
    message: String,
    thread: String,
    bytes: usize,
    previous: String,
}

impl Timeline<'_> {
    async fn write(&self, uid: &str, operation: &str, text: &str) -> Result<Value, String> {
        let result = self.tools.run("lince_message", json!({
            "operation":operation,"request_id":nucleus::new_uid("stream"),"message_uid":uid,"text":text
        })).await;
        if result["ok"] == true {
            Ok(result["result"].clone())
        } else {
            Err(result["error"]
                .as_str()
                .unwrap_or("Could not save reply.")
                .into())
        }
    }

    async fn close(&self, state: &mut State) -> Result<(), String> {
        if !state.closed {
            self.write(&state.message, "finish", &state.text[state.offset..])
                .await?;
            state.closed = true;
        }
        Ok(())
    }

    pub async fn update(&self, text: &str) -> Result<(), String> {
        let mut state = self.state.lock().await;
        if text == state.text {
            return Ok(());
        }
        if state.closed {
            let result = self
                .tools
                .run(
                    "lince_message",
                    json!({"operation":"start","request_id":nucleus::new_uid("stream"),"text":""}),
                )
                .await;
            let uid = result["result"]["message_uid"]
                .as_str()
                .ok_or("Could not start the next reply.")?
                .to_string();
            save(
                &self.pending,
                &Pending {
                    message: uid.clone(),
                },
            )?;
            state.message = uid;
            state.offset = state.text.len();
            state.closed = false;
        }
        let body = text
            .get(state.offset..)
            .ok_or("The agent changed a completed reply.")?;
        self.write(&state.message, "update", body).await?;
        state.text = text.into();
        Ok(())
    }

    pub async fn finish(&self, text: &str, interrupted: bool) -> Result<(), String> {
        self.update(text).await?;
        let mut state = self.state.lock().await;
        if !state.closed {
            self.write(
                &state.message,
                if interrupted { "interrupt" } else { "finish" },
                &state.text[state.offset..],
            )
            .await?;
            state.closed = true;
        }
        Ok(())
    }

    pub async fn activity(&self, value: &Value) -> Result<(), String> {
        let mut state = self.state.lock().await;
        if value["sessionUpdate"] == "message_boundary" {
            if let Some(id) = value["messageId"].as_str() {
                if state
                    .message_id
                    .as_deref()
                    .is_some_and(|previous| previous != id)
                {
                    self.close(&mut state).await?;
                }
                state.message_id = Some(id.into());
            }
            return Ok(());
        }
        if !matches!(
            value["sessionUpdate"].as_str(),
            Some("tool_call" | "tool_call_update")
        ) {
            return Ok(());
        }
        let id = value["toolCallId"]
            .as_str()
            .ok_or("Tool update has no identifier.")?;
        if !state.calls.contains_key(id) {
            if state.calls.len() >= 256 {
                return Err("The turn reached its saved tool-call limit.".into());
            }
            self.close(&mut state).await?;
            let title = value["title"].as_str().unwrap_or("Agent tool");
            let message = self
                .engine
                .act(
                    Action::CreateMessage {
                        thread: self.thread.clone(),
                        body: title.chars().take(512).collect(),
                        author: Some(self.author.clone()),
                        state: MessageState::Finished,
                        parent: Some(state.message.clone()),
                        references: vec![],
                    },
                    None,
                )
                .await
                .map_err(|error| error.to_string())?
                .created
                .ok_or("Could not save tool summary.")?;
            let thread = self
                .engine
                .act(
                    Action::CreateThread {
                        target: message.clone(),
                        head: "Tool transcript".into(),
                    },
                    None,
                )
                .await
                .map_err(|error| error.to_string())?
                .created
                .ok_or("Could not save tool transcript.")?;
            state.calls.insert(
                id.into(),
                Call {
                    message,
                    thread,
                    bytes: 0,
                    previous: String::new(),
                },
            );
        }
        let call = state.calls.get_mut(id).unwrap();
        let mut parts = Vec::new();
        if let Some(input) = value.get("rawInput").filter(|v| !v.is_null()) {
            parts.push(format!(
                "Input\n{}",
                serde_json::to_string_pretty(input).map_err(|error| error.to_string())?
            ));
        }
        for content in value["content"].as_array().into_iter().flatten() {
            if let Some(text) = content["content"]["text"].as_str() {
                parts.push(text.to_string());
            } else {
                parts.push(
                    serde_json::to_string_pretty(content).map_err(|error| error.to_string())?,
                );
            }
        }
        if let Some(output) = value.get("rawOutput").filter(|v| !v.is_null()) {
            parts.push(format!(
                "Output\n{}",
                serde_json::to_string_pretty(output).map_err(|error| error.to_string())?
            ));
        }
        let detail = parts.join("\n\n");
        let retained = self.engine.store.pool.clone();
        if store::records::get(&retained, &call.thread)
            .await
            .map_err(|error| error.to_string())?
            .is_none()
        {
            return Ok(());
        }
        if !detail.is_empty() && detail != call.previous && call.bytes < 256 * 1024 {
            let remaining = (256 * 1024 - call.bytes).min(60_000);
            let mut end = remaining.min(detail.len());
            while !detail.is_char_boundary(end) {
                end -= 1;
            }
            let mut text = detail[..end].to_string();
            if text.len() < detail.len() {
                text.push_str("\n[Output truncated]");
            }
            call.bytes += text.len();
            self.engine
                .act(
                    Action::CreateMessage {
                        thread: call.thread.clone(),
                        body: text,
                        author: Some(self.author.clone()),
                        state: MessageState::Finished,
                        parent: None,
                        references: vec![],
                    },
                    None,
                )
                .await
                .map_err(|error| error.to_string())?;
            call.previous = detail.clone();
        }
        let lines: Vec<_> = detail.lines().collect();
        let mut preview = lines.iter().take(3).copied().collect::<Vec<_>>().join("\n");
        if lines.len() > 3 {
            preview.push_str(&format!("\n… +{} lines", lines.len() - 3));
        }
        let old = store::records::get_extension(
            &self.engine.store.pool,
            &call.message,
            "lince.tool-call",
        )
        .await
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
        let status = value
            .get("status")
            .cloned()
            .unwrap_or_else(|| old["status"].clone());
        if detail.is_empty() {
            preview = old["preview"].as_str().unwrap_or("").into();
        }
        self.engine.act(Action::SetExtension {
            target: call.message.clone(), namespace: "lince.tool-call".into(),
            fds: json!({"id":id,"thread":call.thread,"status":status,"preview":preview.chars().take(800).collect::<String>()}),
        }, None).await.map_err(|error| error.to_string())?;
        Ok(())
    }
}
