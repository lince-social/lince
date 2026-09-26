use crate::{
    provider::{DiscardText, Message, Provider, TextOutput},
    tools::Registry,
};
use std::{collections::HashSet, time::Duration};
use tokio::sync::watch;

pub const MAX_CONTEXT_BYTES: usize = 512 * 1024;

pub fn validate_context(system: &str, messages: &[Message]) -> Result<(), String> {
    let mut text = system.len();
    if text > MAX_CONTEXT_BYTES {
        return Err("This thread exceeds the context limit. Start another thread.".into());
    }
    let mut binary = 0;
    for message in messages {
        if let Message::RichUser {
            text: body,
            content,
        }
        | Message::RichAssistant {
            text: body,
            content,
        } = message
        {
            text += body.len();
            for part in content {
                match part {
                    nucleus::message::MessagePart::Question { question } => {
                        text += question.text().len()
                    }
                    nucleus::message::MessagePart::Steps { steps } => {
                        text += nucleus::operation::steps_text(steps).len()
                    }
                    nucleus::message::MessagePart::Text { text: body } => text += body.len(),
                    nucleus::message::MessagePart::Reference { name, uri } => {
                        text += name.len() + uri.len()
                    }
                    nucleus::message::MessagePart::Attachment { data, .. } => binary += data.len(),
                }
            }
        } else {
            text += serde_json::to_vec(message)
                .map_err(|error| error.to_string())?
                .len();
        }
        if text > MAX_CONTEXT_BYTES || text + binary > 8 * 1024 * 1024 {
            return Err("This thread exceeds the context limit. Start another thread.".into());
        }
    }
    Ok(())
}
pub const MAX_MODEL_REQUESTS: usize = 32;
pub const MAX_TOOL_CALLS: usize = 64;

pub async fn run(
    provider: &dyn Provider,
    system: &str,
    messages: Vec<Message>,
    tools: &Registry,
    stop: watch::Receiver<bool>,
) -> Result<String, String> {
    run_streamed(provider, system, messages, tools, stop, &DiscardText).await
}

pub async fn run_streamed(
    provider: &dyn Provider,
    system: &str,
    messages: Vec<Message>,
    tools: &Registry,
    stop: watch::Receiver<bool>,
    output: &dyn TextOutput,
) -> Result<String, String> {
    let mut receipts = Vec::new();
    let result = turn(
        provider,
        system,
        messages,
        tools,
        stop,
        &mut receipts,
        output,
    )
    .await;
    result.map_err(|error| {
        if receipts.is_empty() {
            error
        } else {
            format!(
                "{error}\n\nTool results before stopping:\n{}",
                receipts.join("\n")
            )
        }
    })
}

struct Prefix<'a> {
    text: &'a str,
    output: &'a dyn TextOutput,
}

#[async_trait::async_trait]
impl TextOutput for Prefix<'_> {
    async fn usage(&self, usage: nucleus::operation::Usage) -> Result<(), String> {
        self.output.usage(usage).await
    }
    async fn update(&self, text: &str) -> Result<(), String> {
        let combined = if self.text.is_empty() {
            text.into()
        } else if text.is_empty() {
            self.text.into()
        } else {
            format!("{}\n\n{text}", self.text)
        };
        if combined.len() > MAX_CONTEXT_BYTES {
            return Err("The reply exceeds the text limit.".into());
        }
        self.output.update(&combined).await
    }
}

async fn turn(
    provider: &dyn Provider,
    system: &str,
    mut messages: Vec<Message>,
    tools: &Registry,
    mut stop: watch::Receiver<bool>,
    receipts: &mut Vec<String>,
    output: &dyn TextOutput,
) -> Result<String, String> {
    let definitions = tools.definitions();
    let mut call_ids = HashSet::new();
    let mut text = String::new();
    for _ in 0..MAX_MODEL_REQUESTS {
        if *stop.borrow() {
            return Err("Stopped by you.".into());
        }
        validate_context(system, &messages)?;
        let progress = Prefix {
            text: &text,
            output,
        };
        let reply = tokio::select! {
            _ = stop.changed() => return Err("Stopped by you.".into()),
            result = tokio::time::timeout(Duration::from_secs(120), provider.stream(system, &messages, &definitions, &progress)) => {
                result.map_err(|_| "The provider did not reply within two minutes.".to_string())??
            }
        };
        progress.update(&reply.text).await?;
        if !reply.text.is_empty() {
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(&reply.text);
        }
        if reply.calls.is_empty() {
            return if reply.text.trim().is_empty() {
                Err("The provider returned an empty reply.".into())
            } else {
                Ok(text)
            };
        }
        if call_ids.len() + reply.calls.len() > MAX_TOOL_CALLS {
            return Err(format!(
                "Stopped at the limit of {MAX_TOOL_CALLS} tool calls."
            ));
        }
        for call in &reply.calls {
            if call.id.is_empty() || !call_ids.insert(call.id.clone()) {
                return Err("The provider repeated a tool call identifier.".into());
            }
        }
        messages.push(Message::Assistant {
            text: reply.text,
            calls: reply.calls.clone(),
        });
        for call in reply.calls {
            if *stop.borrow() {
                return Err(
                    "Stopped by you. Completed Record and file operations remain saved.".into(),
                );
            }
            let result = tools.run(&call.name, call.arguments).await;
            receipts.push(format!("{}: {}", call.name, result));
            messages.push(Message::Tool {
                id: call.id,
                name: call.name,
                result,
            });
        }
    }
    Err(format!(
        "Stopped at the limit of {MAX_MODEL_REQUESTS} model requests. Completed Record and file operations remain saved."
    ))
}
