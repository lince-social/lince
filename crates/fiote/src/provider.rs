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
    RichUser {
        text: String,
        content: Vec<nucleus::message::MessagePart>,
    },
    RichAssistant {
        text: String,
        content: Vec<nucleus::message::MessagePart>,
    },
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
    #[serde(default)]
    pub usage: Option<nucleus::operation::Usage>,
    pub text: String,
    pub calls: Vec<ToolCall>,
}

#[async_trait]
pub trait TextOutput: Send + Sync {
    async fn context(&self, _messages: &[Message]) -> Result<(), String> {
        Ok(())
    }
    async fn usage(&self, _usage: nucleus::operation::Usage) -> Result<(), String> {
        Ok(())
    }
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
    fn validate_content(&self, content: &[nucleus::message::MessagePart]) -> Result<(), String> {
        if content.is_empty() {
            Ok(())
        } else {
            Err("This provider adapter does not advertise message attachments. Use a capable ACP agent or direct provider.".into())
        }
    }
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
        if let Some(usage) = &reply.usage {
            output.usage(usage.clone()).await?;
        }
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

fn message(value: &Message) -> Result<ChatMessage, String> {
    Ok(match value {
        Message::User(text) => ChatMessage::user(text.clone()),
        Message::RichUser { text, content } | Message::RichAssistant { text, content } => {
            let mut parts = vec![ContentPart::Text(text.clone())];
            for part in content {
                parts.push(match part {
                    nucleus::message::MessagePart::Question { question } => {
                        ContentPart::Text(question.text())
                    }
                    nucleus::message::MessagePart::Steps { steps } => {
                        ContentPart::Text(nucleus::operation::steps_text(steps))
                    }
                    nucleus::message::MessagePart::Text { text } => ContentPart::Text(text.clone()),
                    nucleus::message::MessagePart::Reference { name, uri } => {
                        ContentPart::Text(format!("Resource reference: {name}\n{uri}"))
                    }
                    nucleus::message::MessagePart::Attachment {
                        name,
                        mime_type,
                        data,
                    } if mime_type.starts_with("text/") || mime_type == "application/json" => {
                        let text = String::from_utf8(nucleus::message::decode(data)?)
                            .map_err(|_| "The text attachment is not UTF-8.")?;
                        ContentPart::Text(format!("Attached file: {name} ({mime_type})\n{text}"))
                    }
                    nucleus::message::MessagePart::Attachment {
                        name,
                        mime_type,
                        data,
                    } => ContentPart::Binary(genai::chat::Binary::from_base64(
                        mime_type.clone(),
                        data.clone(),
                        Some(name.clone()),
                    )),
                });
            }
            if matches!(value, Message::RichAssistant { .. }) {
                ChatMessage::assistant(parts)
            } else {
                ChatMessage::user(parts)
            }
        }
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
    })
}

#[async_trait]
impl Provider for GenaiProvider {
    fn validate_content(&self, content: &[nucleus::message::MessagePart]) -> Result<(), String> {
        nucleus::message::validate(content)?;
        for part in content {
            if let nucleus::message::MessagePart::Attachment {
                mime_type, data, ..
            } = part
            {
                let text = mime_type.starts_with("text/") || mime_type == "application/json";
                if text {
                    String::from_utf8(nucleus::message::decode(data)?)
                        .map_err(|_| "The text attachment is not UTF-8.")?;
                }
                let media = mime_type.starts_with("image/") || mime_type == "application/pdf";
                let audio = mime_type.starts_with("audio/")
                    && self.target.model.adapter_kind == genai::adapter::AdapterKind::Gemini;
                if !(text || media || audio) {
                    return Err("This direct provider does not support this file type. Use a capable ACP agent or a resource reference.".into());
                }
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
        for value in messages {
            if let Message::RichUser { content, .. } | Message::RichAssistant { content, .. } =
                value
            {
                self.validate_content(content)?;
            }
        }
        let request = ChatRequest::new(
            messages
                .iter()
                .map(message)
                .collect::<Result<Vec<_>, _>>()?,
        )
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
        let usage = Some(usage_report(&self.target, &response.usage));
        if response
            .content
            .iter()
            .any(|part| matches!(part, ContentPart::Binary(_)))
        {
            return Err("This direct adapter returned media output that it cannot save. Use a capable ACP agent for rich replies.".into());
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
        Ok(Reply { text, calls, usage })
    }

    async fn stream(
        &self,
        system: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
        output: &dyn TextOutput,
    ) -> Result<Reply, String> {
        for value in messages {
            if let Message::RichUser { content, .. } | Message::RichAssistant { content, .. } =
                value
            {
                self.validate_content(content)?;
            }
        }
        let request = ChatRequest::new(
            messages
                .iter()
                .map(message)
                .collect::<Result<Vec<_>, _>>()?,
        )
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
            .with_capture_usage(true)
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
                    if end.captured_content.as_ref().is_some_and(|content| {
                        content
                            .iter()
                            .any(|part| matches!(part, ContentPart::Binary(_)))
                    }) {
                        return Err("This direct adapter returned media output that it cannot save. Use a capable ACP agent for rich replies.".into());
                    }
                    let usage = end
                        .captured_usage
                        .as_ref()
                        .map(|usage| usage_report(&self.target, usage));
                    if let Some(usage) = &usage {
                        output.usage(usage.clone()).await?;
                    }
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
                    return Ok(Reply { text, calls, usage });
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

fn usage_report(
    target: &genai::ServiceTarget,
    usage: &genai::chat::Usage,
) -> nucleus::operation::Usage {
    nucleus::operation::Usage::request(
        format!("genai · {}", target.model.model_name),
        usage.prompt_tokens.and_then(|value| value.try_into().ok()),
        usage
            .completion_tokens
            .and_then(|value| value.try_into().ok()),
        usage.total_tokens.and_then(|value| value.try_into().ok()),
    )
}

#[cfg(test)]
mod attachment_tests {
    use super::*;
    use base64::{Engine as _, engine::general_purpose::STANDARD as B64};
    use nucleus::message::MessagePart;

    fn file(name: &str, mime: &str, bytes: &[u8]) -> MessagePart {
        MessagePart::Attachment {
            name: name.into(),
            mime_type: mime.into(),
            data: B64.encode(bytes),
        }
    }

    fn provider() -> GenaiProvider {
        GenaiProvider::new(
            &Settings {
                provider: crate::config::ProviderKind("openai".into()),
                model: "fixture".into(),
                endpoint: "https://api.openai.com/v1/".into(),
                ..Default::default()
            },
            &Secret("fixture-key".into()),
        )
        .unwrap()
    }

    #[test]
    fn direct_text_and_csv_input_preserves_names_contents_and_accompanying_message() {
        let files = vec![
            file("note.txt", "text/plain", b"Marker 617"),
            file("table.csv", "text/csv", b"item,value\na,2\nb,3\n"),
        ];
        provider().validate_content(&files).unwrap();
        let supplied = message(&Message::RichUser {
            text: "Analyze both files".into(),
            content: files,
        })
        .unwrap();
        assert_eq!(
            supplied.content.texts(),
            vec![
                "Analyze both files",
                "Attached file: note.txt (text/plain)\nMarker 617",
                "Attached file: table.csv (text/csv)\nitem,value\na,2\nb,3\n",
            ]
        );
    }

    #[test]
    fn direct_unsupported_media_and_invalid_text_refuse_the_whole_request() {
        let provider = provider();
        for (name, mime) in [("clip.mp4", "video/mp4"), ("voice.wav", "audio/wav")] {
            assert!(
                provider
                    .validate_content(&[
                        file("note.txt", "text/plain", b"Keep this input"),
                        file(name, mime, b"bytes"),
                    ])
                    .unwrap_err()
                    .contains("does not support")
            );
        }
        let invalid = file("invalid.txt", "text/plain", &[0xff]);
        assert!(
            provider
                .validate_content(std::slice::from_ref(&invalid))
                .unwrap_err()
                .contains("UTF-8")
        );
        assert!(
            message(&Message::RichUser {
                text: "Inspect".into(),
                content: vec![invalid]
            })
            .is_err()
        );
    }
}
