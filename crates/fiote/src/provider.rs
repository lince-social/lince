use crate::config::{ProviderKind, Secret, Settings};
use async_trait::async_trait;
use genai::chat::{
    ChatMessage, ChatOptions, ChatRequest, ContentPart, StopReason, Tool, ToolResponse,
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

#[derive(Debug, Clone)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub schema: Value,
}

#[derive(Debug, Clone, Default)]
pub struct Reply {
    pub text: String,
    pub calls: Vec<ToolCall>,
}

#[async_trait]
pub trait Provider: Send + Sync {
    async fn complete(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
    ) -> Result<Reply, String>;
}

pub struct GenaiProvider {
    client: genai::Client,
    target: genai::ServiceTarget,
}

impl GenaiProvider {
    pub fn new(settings: &Settings, key: &Secret) -> Result<Self, String> {
        let adapter = match settings.provider {
            ProviderKind::OpenAi => genai::adapter::AdapterKind::OpenAI,
            ProviderKind::Anthropic => genai::adapter::AdapterKind::Anthropic,
            ProviderKind::Gemini => genai::adapter::AdapterKind::Gemini,
            ProviderKind::Ollama => genai::adapter::AdapterKind::Ollama,
        };
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
}
