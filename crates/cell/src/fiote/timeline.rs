use super::*;
use serde_json::{Value, json};

pub(super) struct Timeline<'a> {
    pub engine: Arc<Engine>,
    pub root: String,
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

    pub async fn progress(&self, steps: Option<Value>, status: &str) -> Result<(), String> {
        let mut value = store::records::get_extension(
            &self.engine.store.pool,
            &self.root,
            "lince.message-progress",
        )
        .await
        .map_err(|error| error.to_string())?
        .unwrap_or_else(|| json!({"source":"agent","steps":[],"history":[]}));
        if steps.is_none()
            && value["steps"]
                .as_array()
                .is_none_or(|steps| steps.is_empty())
        {
            return Ok(());
        }
        if let Some(steps) = steps {
            let parsed: Vec<nucleus::operation::Step> =
                serde_json::from_value(steps.clone()).map_err(|error| error.to_string())?;
            if !parsed.is_empty() {
                nucleus::operation::validate_steps(&parsed)?;
            }
            value["steps"] = steps;
        }
        if value["state"] == status
            && value["history"]
                .as_array()
                .and_then(|history| history.last())
                .is_some_and(|last| last["steps"] == value["steps"])
        {
            return Ok(());
        }
        value["state"] = status.into();
        value["updated_ms"] = nucleus::operation::now_ms().into();
        let snapshot =
            json!({"state":value["state"],"steps":value["steps"],"updated_ms":value["updated_ms"]});
        let history = value["history"]
            .as_array_mut()
            .ok_or("Invalid progress history.")?;
        history.push(snapshot);
        if history.len() > 16 {
            history.remove(0);
        }
        self.engine
            .act(
                Action::SetExtension {
                    target: self.root.clone(),
                    namespace: "lince.message-progress".into(),
                    fds: value,
                },
                None,
            )
            .await
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub async fn open_content(&self) -> Result<String, String> {
        let mut state = self.state.lock().await;
        if state.closed {
            self.reopen(&mut state).await?;
        }
        Ok(state.message.clone())
    }

    async fn reopen(&self, state: &mut State) -> Result<(), String> {
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
                root: Some(self.root.clone()),
                message: uid.clone(),
            },
        )?;
        state.message = uid;
        state.offset = state.text.len();
        state.closed = false;
        Ok(())
    }

    pub async fn update(&self, text: &str) -> Result<(), String> {
        let mut state = self.state.lock().await;
        if text == state.text {
            return Ok(());
        }
        if state.closed {
            self.reopen(&mut state).await?;
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
        if value["sessionUpdate"] == "plan" {
            return self
                .progress(Some(value["entries"].clone()), "running")
                .await;
        }
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
                        content: Vec::new(),
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
                        content: Vec::new(),
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
