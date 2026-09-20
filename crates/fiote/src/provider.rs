use crate::config::{Secret, Settings};
use async_trait::async_trait;
use futures::StreamExt;
use genai::chat::{
    ChatMessage, ChatOptions, ChatRequest, ChatStreamEvent, ContentPart, StopReason, Tool,
    ToolResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
    pub signatures: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    User(String),
    Assistant {
        text: String,
        calls: Vec<ToolCall>,
    },
    Tool {
        id: String,
        name: String,
        result: Value,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Reply {
    pub text: String,
    pub calls: Vec<ToolCall>,
}

#[async_trait]
pub trait TextOutput: Send + Sync {
    async fn update(&self, text: &str) -> Result<(), String>;
}

pub struct DiscardText;

#[async_trait]
impl TextOutput for DiscardText {
    async fn update(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

#[async_trait]
pub trait Provider: Send + Sync {
    async fn complete(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<Reply, String>;

    async fn stream(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
        output: &dyn TextOutput,
    ) -> Result<Reply, String> {
        let reply = self.complete(system, messages, tools).await?;
        output.update(&reply.text).await?;
        Ok(reply)
    }
}

pub struct GenaiProvider {
    client: genai::Client,
    target: genai::ServiceTarget,
}

impl GenaiProvider {
    pub fn new(settings: &Settings, key: &Secret) -> Result<Self, String> {
        let adapter = genai::adapter::AdapterKind::from_lower_str(&settings.provider.0)
            .ok_or("This provider adapter is unavailable.")?;
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(std::time::Duration::from_secs(15))
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .map_err(|_| "Cannot initialize the provider connection.")?;
        Ok(Self {
            client: genai::Client::builder().with_reqwest(http).build(),
            target: genai::ServiceTarget {
                endpoint: genai::resolver::Endpoint::from_owned(settings.endpoint.clone()),
                auth: genai::resolver::AuthData::from_single(key.0.clone()),
                model: genai::ModelIden::new(adapter, settings.model.clone()),
            },
        })
    }
}

fn message(value: &Message) -> ChatMessage {
    match value {
        Message::User(text) => ChatMessage::user(text.clone()),
        Message::Assistant { text, calls } => {
            let mut parts = Vec::new();
            if !text.is_empty() {
                parts.push(ContentPart::Text(text.clone()));
            }
            for call in calls {
                if let Some(signatures) = &call.signatures {
                    parts.extend(
                        signatures
                            .iter()
                            .cloned()
                            .map(ContentPart::ThoughtSignature),
                    );
                }
                parts.push(ContentPart::ToolCall(genai::chat::ToolCall {
                    call_id: call.id.clone(),
                    fn_name: call.name.clone(),
                    fn_arguments: call.arguments.clone(),
                    thought_signatures: None,
                }));
            }
            ChatMessage::assistant(parts)
        }
        Message::Tool { id, name, result } => {
            ChatMessage::tool(ToolResponse::new(id, result.to_string()).with_fn_name(name))
        }
    }
}

#[async_trait]
impl Provider for GenaiProvider {
    async fn complete(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<Reply, String> {
        let request = ChatRequest::new(messages.iter().map(message).collect())
            .with_system(system)
            .with_tools(
                tools
                    .iter()
                    .map(|tool| {
                        Tool::new(&tool.name)
                            .with_description(&tool.description)
                            .with_schema(tool.schema.clone())
                    })
                    .collect::<Vec<_>>(),
            );
        let response = self.client.exec_chat(self.target.clone(), request, Some(&ChatOptions::default().with_max_tokens(4096))).await
            .map_err(|_| "Provider request failed. Check the endpoint, model, API key and provider availability.".to_string())?;
        if matches!(
            response.stop_reason,
            Some(StopReason::MaxTokens(_) | StopReason::ContentFilter(_) | StopReason::Other(_))
        ) {
            return Err("The provider stopped before completing its reply. Try a shorter request or another model.".into());
        }
        let text = response.content.texts().join("\n");
        let calls = response
            .into_tool_calls()
            .into_iter()
            .map(|call| ToolCall {
                id: call.call_id,
                name: call.fn_name,
                arguments: call.fn_arguments,
                signatures: call.thought_signatures,
            })
            .collect();
        Ok(Reply { text, calls })
    }

    async fn stream(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
        output: &dyn TextOutput,
    ) -> Result<Reply, String> {
        let request = ChatRequest::new(messages.iter().map(message).collect())
            .with_system(system)
            .with_tools(
                tools
                    .iter()
                    .map(|tool| {
                        Tool::new(&tool.name)
                            .with_description(&tool.description)
                            .with_schema(tool.schema.clone())
                    })
                    .collect::<Vec<_>>(),
            );
        let options = ChatOptions::default()
            .with_max_tokens(4096)
            .with_capture_content(true)
            .with_capture_tool_calls(true);
        let mut stream = self
            .client
            .exec_chat_stream(self.target.clone(), request, Some(&options))
            .await
            .map_err(|_| "Provider request failed. Check the endpoint, model and credentials.")?
            .stream;
        let mut text = String::new();
        let mut dirty = false;
        let mut first = true;
        let mut flush = tokio::time::interval(std::time::Duration::from_millis(60));
        flush.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            let event = tokio::select! {
                event = stream.next() => event,
                _ = flush.tick(), if dirty => {
                    output.update(&text).await?;
                    dirty = false;
                    continue;
                }
            };
            match event {
                Some(Ok(ChatStreamEvent::Chunk(chunk))) => {
                    if text.len() + chunk.content.len() > crate::runtime::MAX_CONTEXT_BYTES {
                        output.update(&text).await?;
                        return Err("The provider reply exceeded the text limit.".into());
                    }
                    text.push_str(&chunk.content);
                    dirty = true;
                    if first && !text.is_empty() {
                        output.update(&text).await?;
                        dirty = false;
                        first = false;
                    }
                }
                Some(Ok(ChatStreamEvent::End(end))) => {
                    output.update(&text).await?;
                    if !matches!(
                        end.captured_stop_reason,
                        Some(
                            StopReason::Completed(_)
                                | StopReason::ToolCall(_)
                                | StopReason::StopSequence(_)
                        )
                    ) {
                        return Err("The provider stopped before completing its reply.".into());
                    }
                    let calls = end
                        .captured_into_tool_calls()
                        .unwrap_or_default()
                        .into_iter()
                        .map(|call| ToolCall {
                            id: call.call_id,
                            name: call.fn_name,
                            arguments: call.fn_arguments,
                            signatures: call.thought_signatures,
                        })
                        .collect();
                    return Ok(Reply { text, calls });
                }
                Some(Ok(_)) => {}
                Some(Err(_)) | None => {
                    output.update(&text).await?;
                    return Err("The provider stream was interrupted before completion.".into());
                }
            }
        }
    }
}
