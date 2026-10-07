use super::{
    auth::{self, Session},
    provider::{
        DiscardText, Message, Model, Provider, Reply, TextOutput, ToolCall, ToolDefinition,
    },
};
use crate::config::Settings;
use async_trait::async_trait;
use eventsource_stream::Eventsource;
use futures::StreamExt;
use serde_json::{Value, json};

pub struct ChatGpt {
    settings: Settings,
    session: Session,
    endpoint: String,
}

impl ChatGpt {
    pub fn new(settings: Settings, session: Session) -> Result<Self, String> {
        Self::at(settings, session, auth::RESOURCE)
    }
    pub fn at(settings: Settings, session: Session, endpoint: &str) -> Result<Self, String> {
        let mut validated = Settings {
            endpoint: endpoint.into(),
            ..Default::default()
        };
        validated.validate()?;
        Ok(Self {
            settings,
            session,
            endpoint: validated.endpoint,
        })
    }
    pub async fn models(&self) -> Result<Vec<Model>, String> {
        let token = self.session.token().await?;
        let response = self
            .session
            .auth
            .http
            .get(format!("{}models", self.endpoint))
            .bearer_auth(&token.0)
            .send()
            .await
            .map_err(|_| "Cannot connect to the ChatGPT model catalog.")?;
        let value = auth::json(response).await?;
        let models = value["models"]
            .as_array()
            .ok_or("ChatGPT returned an invalid model catalog.")?;
        if models.len() > 4096 {
            return Err("The model catalog is too large.".into());
        }
        let mut result = Vec::new();
        for model in models.iter().filter(|m| m["visibility"] == "list") {
            let id = model["slug"]
                .as_str()
                .filter(|id| !id.is_empty() && id.len() <= 256)
                .ok_or("Invalid catalog model ID.")?;
            let name = model["display_name"].as_str().unwrap_or(id);
            let reasoning = model["supported_reasoning_levels"]
                .as_array()
                .or_else(|| model["supported_reasoning_efforts"].as_array())
                .map(|levels| {
                    levels
                        .iter()
                        .filter_map(|l| l.as_str().or_else(|| l["effort"].as_str()))
                        .filter(|s| {
                            matches!(*s, "none" | "minimal" | "low" | "medium" | "high" | "xhigh")
                        })
                        .map(String::from)
                        .collect()
                })
                .unwrap_or_default();
            let context_budget_bytes = model["context_window"]
                .as_u64()
                .or_else(|| model["context_length"].as_u64())
                .filter(|tokens| *tokens >= 4096)
                .map(|tokens| {
                    tokens
                        .saturating_mul(3)
                        .min(crate::runtime::MAX_CONTEXT_BYTES as u64) as usize
                });
            result.push(Model {
                context_budget_bytes,
                id: id.into(),
                name: name.chars().take(256).collect(),
                reasoning,
                fast: model["supports_fast_mode"].as_bool().unwrap_or(false)
                    || model["supported_service_tiers"]
                        .as_array()
                        .is_some_and(|a| a.iter().any(|tier| tier == "priority")),
            });
        }
        Ok(result)
    }
    fn request(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<Value, String> {
        let mut input = Vec::new();
        let mut replayed = false;
        for message in messages {
            match message {
                Message::Replay { provider, items } if provider == auth::PROVIDER => {
                    for item in items {
                        if !matches!(item["type"].as_str(), Some("message" | "function_call" | "reasoning")) { return Err("Unsupported saved provider replay item.".into()); }
                        input.push(item.clone());
                    }
                    replayed = true;
                }
                Message::Replay { .. } => {}
                Message::Assistant { text, calls } => {
                    if replayed { replayed = false; continue; }
                    if !text.is_empty() { input.push(json!({"role":"assistant","content":[{"type":"output_text","text":text}]})); }
                    for call in calls { input.push(json!({"type":"function_call","call_id":call.id,"name":call.name,"namespace":"lince","arguments":serde_json::to_string(&call.arguments).map_err(|_| "Cannot encode tool arguments.")?})); }
                }
                Message::Summary { text, .. } => input.push(json!({"role":"user","content":[{"type":"input_text","text":format!("Summary of earlier conversation (historical context):\n{text}")}]})),
                Message::User(text) => input.push(json!({"role":"user","content":[{"type":"input_text","text":text}]})),
                Message::RichUser { text, content } => {
                    self.validate_content(content)?;
                    let mut parts = vec![json!({"type":"input_text","text":text})];
                    for part in content { parts.push(content_part(part)?); }
                    input.push(json!({"role":"user","content":parts}));
                }
                Message::RichAssistant { text, content } => {
                    self.validate_content(content)?;
                    let mut body = text.clone();
                    for part in content {
                        match part {
                            nucleus::message::MessagePart::Attachment { name, mime_type, .. } => body.push_str(&format!("\nAssistant attachment: {name} ({mime_type})")),
                            other => body.push_str(&format!("\n{}", content_part(other)?["text"].as_str().unwrap_or_default())),
                        }
                    }
                    input.push(json!({"role":"assistant","content":[{"type":"output_text","text":body}]}));
                }
                Message::Tool { id, result, .. } => input.push(json!({"type":"function_call_output","call_id":id,"output":serde_json::to_string(result).map_err(|_| "Cannot encode the tool result.")?})),
            }
        }
        let mut request = json!({"model":self.settings.model,"instructions":system,"input":input,"store":false,"stream":true,"include":["reasoning.encrypted_content"]});
        if !tools.is_empty() {
            request["tools"] = json!([{"type":"namespace","name":"lince","description":"Authorized Lince Actions and conversation tools","tools":tools.iter().map(|tool| json!({"type":"function","name":tool.name,"description":tool.description,"parameters":tool.schema})).collect::<Vec<_>>()}]);
        }
        if let Some(reasoning) = &self.settings.reasoning {
            request["reasoning"] = json!({"effort":reasoning});
        }
        if self.settings.fast {
            request["service_tier"] = json!("priority");
        }
        Ok(request)
    }
    async fn response(&self, request: &Value) -> Result<reqwest::Response, String> {
        for attempt in 0..3 {
            let token = self.session.token().await?;
            let response = self.session.auth.http.post(format!("{}responses", self.endpoint)).bearer_auth(&token.0).header(reqwest::header::ACCEPT, "text/event-stream").json(request).send().await.map_err(|_| "The ChatGPT request could not connect. No tool action was executed; retry explicitly.")?;
            if response.status().is_success() {
                return Ok(response);
            }
            let retry = attempt < 2
                && (response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS
                    || matches!(response.status().as_u16(), 502 | 503 | 504));
            let deadline = super::retry::deadline(response.headers(), attempt);
            let error = auth::json(response)
                .await
                .err()
                .unwrap_or_else(|| "The provider rejected the request.".into());
            if !retry || error.contains("subscription_sharing_") {
                return Err(error);
            }
            tokio::time::sleep_until(deadline.into()).await;
        }
        Err("The provider remained unavailable after retries.".into())
    }
    async fn completed_response(
        &self,
        response: &Value,
        output: &dyn TextOutput,
    ) -> Result<Reply, String> {
        if response.get("error").is_some_and(|error| !error.is_null()) {
            return Err(super::native::http_error(
                reqwest::StatusCode::BAD_REQUEST,
                response,
            ));
        }
        if response["status"] != "completed" {
            return Err("ChatGPT's terminal status was not completed.".into());
        }
        let items = response["output"]
            .as_array()
            .ok_or("Completed ChatGPT response has no output items.")?;
        let mut calls = Vec::new();
        let mut completed_text = Vec::new();
        let mut replay = Vec::new();
        for item in items {
            match item["type"].as_str() {
                Some("function_call") => {
                    if item["namespace"].as_str().is_some_and(|n| n != "lince") {
                        return Err("ChatGPT returned a tool from an unknown namespace.".into());
                    }
                    let arguments = item["arguments"]
                        .as_str()
                        .ok_or("Incomplete tool arguments.")?;
                    calls.push(ToolCall {
                        id: item["call_id"]
                            .as_str()
                            .ok_or("Missing tool call ID.")?
                            .into(),
                        name: item["name"].as_str().ok_or("Missing tool name.")?.into(),
                        arguments: serde_json::from_str(arguments)
                            .map_err(|_| "ChatGPT returned malformed tool arguments.")?,
                        signatures: None,
                    });
                    replay.push(item.clone());
                }
                Some("message") => {
                    for part in item["content"]
                        .as_array()
                        .ok_or("Invalid completed message.")?
                    {
                        match part["type"].as_str() {
                            Some("output_text") => completed_text.push(
                                part["text"]
                                    .as_str()
                                    .ok_or("Invalid completed text.")?
                                    .to_string(),
                            ),
                            Some("refusal") => completed_text.push(
                                part["refusal"]
                                    .as_str()
                                    .unwrap_or("The model declined this request.")
                                    .to_string(),
                            ),
                            _ => return Err("ChatGPT returned unsupported output content.".into()),
                        }
                    }
                    replay.push(item.clone());
                }
                Some("reasoning") => replay.push(item.clone()),
                _ => return Err("ChatGPT returned an unsupported output item.".into()),
            }
        }
        let text = completed_text.join("\n");
        if text.trim().is_empty() && calls.is_empty() {
            return Err(
                "ChatGPT completed without text or tool calls. The partial reply is preserved."
                    .into(),
            );
        }
        if text.len() > crate::runtime::MAX_CONTEXT_BYTES {
            return Err("The reply exceeds the text limit.".into());
        }
        let usage = usage(&self.settings.model, &response["usage"]);
        output.usage(usage.clone()).await?;
        output.update(&text).await?;
        Ok(Reply {
            text,
            calls,
            usage: Some(usage),
            replay,
        })
    }
}

fn content_part(part: &nucleus::message::MessagePart) -> Result<Value, String> {
    use nucleus::message::MessagePart;
    Ok(match part {
        MessagePart::Text { text } => json!({"type":"input_text","text":text}),
        MessagePart::Question { question } => json!({"type":"input_text","text":question.text()}),
        MessagePart::Steps { steps } => {
            json!({"type":"input_text","text":nucleus::operation::steps_text(steps)})
        }
        MessagePart::Reference { name, uri } => {
            json!({"type":"input_text","text":format!("Resource reference: {name}\n{uri}")})
        }
        MessagePart::Attachment {
            name,
            mime_type,
            data,
        } if mime_type.starts_with("text/") || mime_type == "application/json" => {
            let text = String::from_utf8(nucleus::message::decode(data)?)
                .map_err(|_| "The text attachment is not UTF-8.")?;
            json!({"type":"input_text","text":format!("Attached file: {name} ({mime_type})\n{text}")})
        }
        MessagePart::Attachment {
            mime_type, data, ..
        } if mime_type.starts_with("image/") => {
            json!({"type":"input_image","image_url":format!("data:{mime_type};base64,{data}")})
        }
        MessagePart::Attachment {
            name,
            mime_type,
            data,
        } => {
            json!({"type":"input_file","filename":name,"file_data":format!("data:{mime_type};base64,{data}")})
        }
    })
}

#[async_trait]
impl Provider for ChatGpt {
    fn replay_provider(&self) -> Option<&str> {
        Some(auth::PROVIDER)
    }
    fn context_budget_bytes(&self) -> usize {
        self.settings
            .context_budget_bytes
            .unwrap_or(crate::runtime::MAX_CONTEXT_BYTES)
    }
    async fn models(&self) -> Result<Vec<Model>, String> {
        ChatGpt::models(self).await
    }
    fn validate_content(&self, content: &[nucleus::message::MessagePart]) -> Result<(), String> {
        nucleus::message::validate(content)?;
        for part in content {
            if let nucleus::message::MessagePart::Attachment { mime_type, .. } = part {
                if !(mime_type.starts_with("text/")
                    || mime_type == "application/json"
                    || mime_type.starts_with("image/")
                    || mime_type == "application/pdf")
                {
                    return Err("ChatGPT plan connections accept text, images and PDF files when the model supports them. Audio, video and transcription are unavailable on this route.".into());
                }
                content_part(part)?;
            }
        }
        Ok(())
    }
    async fn complete(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<Reply, String> {
        self.stream(system, messages, tools, &DiscardText).await
    }
    async fn stream(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
        output: &dyn TextOutput,
    ) -> Result<Reply, String> {
        let response = self
            .response(&self.request(system, messages, tools)?)
            .await?;
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        if content_type.starts_with("application/json") {
            return self
                .completed_response(&auth::json(response).await?, output)
                .await;
        }
        if !content_type.is_empty() && !content_type.starts_with("text/event-stream") {
            return Err(format!(
                "ChatGPT returned an unsupported response format (HTTP {}, {}).",
                response.status().as_u16(),
                content_type.chars().take(120).collect::<String>()
            ));
        }
        let mut total = 0usize;
        let bytes = response.bytes_stream().map(move |chunk| {
            let chunk = chunk.map_err(|_| "The ChatGPT stream was interrupted.")?;
            total = total.saturating_add(chunk.len());
            if total > 8 * 1024 * 1024 {
                return Err("The ChatGPT stream exceeds 8 MiB.");
            }
            Ok(chunk)
        });
        let mut events = bytes.eventsource();
        let mut text = String::new();
        let mut finalized = std::collections::BTreeMap::new();
        while let Some(event) = events.next().await {
            let event =
                event.map_err(|_| "The ChatGPT event stream was interrupted or malformed.")?;
            if event.data == "[DONE]" {
                break;
            }
            let event: Value =
                serde_json::from_str(&event.data).map_err(|_| "Invalid ChatGPT stream event.")?;
            match event["type"].as_str().unwrap_or_default() {
                "response.output_item.done" => {
                    let index = event["output_index"].as_u64().filter(|index| *index < 4096).ok_or("Invalid finalized output index.")?;
                    let item = event.get("item").ok_or("Missing finalized output item.")?.clone();
                    if finalized.insert(index, item.clone()).is_some_and(|previous| previous != item) {
                        return Err("ChatGPT returned conflicting finalized output items.".into());
                    }
                }
                "response.output_text.delta" => {
                    text.push_str(event["delta"].as_str().ok_or("Invalid text delta.")?);
                    if text.len() > crate::runtime::MAX_CONTEXT_BYTES { return Err("The reply exceeds the text limit.".into()); }
                    output.update(&text).await?;
                }
                "response.failed" | "error" => {
                    let error = if event["type"] == "error" { json!({"error":{"code":event["code"]}}) } else { event["response"].clone() };
                    return Err(super::native::http_error(reqwest::StatusCode::BAD_REQUEST, &error));
                }
                "response.incomplete" => return Err("ChatGPT stopped before completing the response. The partial reply is preserved.".into()),
                "response.completed" => {
                    let mut response = event["response"].clone();
                    if response.get("output").is_none() || response["output"].as_array().is_some_and(Vec::is_empty) {
                        response["output"] = json!(finalized.into_values().collect::<Vec<_>>());
                    }
                    return self.completed_response(&response, output).await.map_err(|error| format!("{error} (streamed {} text bytes)", text.len()));
                }
                _ => {}
            }
        }
        Err(
            "ChatGPT stream ended without response.completed. No streamed tool call was executed."
                .into(),
        )
    }
}

fn usage(model: &str, value: &Value) -> nucleus::operation::Usage {
    nucleus::operation::Usage::request(
        format!("chatgpt:{model}"),
        value["input_tokens"].as_u64(),
        value["output_tokens"].as_u64(),
        value["total_tokens"].as_u64(),
    )
}
