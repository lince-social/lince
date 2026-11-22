// non-lean-yet
// Scope: const MAX_CONTEXT_BYTES, fn context_bytes, fn validate_context, fn run.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 282.
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 165, 234, 253,
//   260.
// - [Clippy: clippy::as_conversions] Use From or TryFrom when it preserves the conversion contract.
//   Sites 218.
// - [Clippy: clippy::map_err_ignore] Preserve relevant causes and context at the boundary. Sites
//   160, 193.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 10, 41, 83, 93.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 8, 10,
//   41, 83, 93.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites context_bytes:
//   String (10), run: String (83), run_streamed: String (93), validate_context: String (41).

use std::{collections::HashSet, time::Duration};

use serde_json::{Value, json};
use tokio::sync::watch;

use crate::{
    provider::{DiscardText, Message, Provider, TextOutput},
    tools::{
        Registry,
        receipts::{self, Receipts},
    },
};

pub const MAX_CONTEXT_BYTES: usize = 512 * 1024;

pub fn context_bytes(messages: &[Message]) -> Result<usize, String> {
    let mut size = 0usize;
    for message in messages {
        size = size.saturating_add(match message {
            Message::RichUser { text, content } | Message::RichAssistant { text, content } => {
                let mut size = text.len();
                for part in content {
                    size = size.saturating_add(match part {
                        nucleus::message::MessagePart::Attachment {
                            mime_type, data, ..
                        } if mime_type.starts_with("text/") || mime_type == "application/json" => {
                            nucleus::message::decode(data)?.len()
                        }
                        nucleus::message::MessagePart::Attachment { mime_type, .. } => {
                            if mime_type.starts_with("image/") {
                                4096
                            } else {
                                16384
                            }
                        }
                        other => nucleus::json::encoded_len(other).map_err(|e| e.to_string())?,
                    });
                }
                size
            }
            other => nucleus::json::encoded_len(other).map_err(|e| e.to_string())?,
        });
    }
    Ok(size)
}

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
            text += nucleus::json::encoded_len(message).map_err(|error| error.to_string())?;
        }
        if text > MAX_CONTEXT_BYTES || text + binary > 8 * 1024 * 1024 {
            return Err("This thread exceeds the context limit. Start another thread.".into());
        }
    }
    Ok(())
}

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
                "{error}\n\nRecent tool call outcomes before stopping (complete receipts remain in the journal):\n{}",
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
    let mut definitions = tools.definitions();
    definitions.retain(|definition| definition.name != receipts::READ_TOOL_OUTPUT);
    let tool_outputs = if definitions.is_empty() {
        None
    } else {
        definitions.push(receipts::definition());
        Some(Receipts::new(output.tool_output_directory())?)
    };
    let tool_bytes =
        nucleus::json::encoded_len(&definitions).map_err(|_| "Cannot encode tool descriptions.")?;
    let mut previous_calls: HashSet<String> = messages
        .iter()
        .filter_map(|m| {
            if let Message::Tool { id, .. } = m {
                Some(id.clone())
            } else {
                None
            }
        })
        .collect();
    previous_calls.extend(output.executed_calls().await?);
    let mut call_ids = HashSet::new();
    let mut text = String::new();
    let mut force_compact = false;
    let mut overflow_retried = false;
    loop {
        if *stop.borrow() {
            return Err("Stopped by you.".into());
        }
        tokio::select! {
            _ = stop.changed() => return Err("Stopped by you.".into()),
            result = crate::conversation::prepare_context(provider, system, &mut messages, output, tool_bytes, force_compact, tool_outputs.as_ref()) => { result?; }
        }
        output.context(&messages).await?;
        let projected =
            crate::conversation::project_tool_results(&messages, tool_outputs.as_ref())?;
        validate_context(system, &projected)?;
        output.model_context(&projected, &definitions).await?;
        let progress = Prefix {
            text: &text,
            output,
        };
        let reply = tokio::select! {
            _ = stop.changed() => return Err("Stopped by you.".into()),
            result = tokio::time::timeout(Duration::from_secs(120), provider.stream(system, &projected, &definitions, &progress)) => {
                result.map_err(|_| "The provider did not reply within two minutes.".to_string())?
            }
        };
        drop(projected);
        let reply = match reply {
            Ok(reply) => reply,
            Err(error)
                if !overflow_retried
                    && (error.contains("context_window_exceeded")
                        || error.contains("context_length_exceeded")) =>
            {
                overflow_retried = true;
                force_compact = true;
                continue;
            }
            Err(error) => return Err(error),
        };
        overflow_retried = false;
        force_compact = reply.usage.as_ref().is_some_and(|usage| {
            let used = usage.context_used.or(usage.total_tokens).or_else(|| {
                usage
                    .input_tokens
                    .map(|input| input.saturating_add(usage.output_tokens.unwrap_or(0)))
            });
            let capacity = usage
                .context_capacity
                .unwrap_or((provider.context_budget_bytes().min(MAX_CONTEXT_BYTES) / 3) as u64);
            used.is_some_and(|used| used >= capacity.saturating_mul(4) / 5)
        });
        progress.update(&reply.text).await?;
        if !reply.text.is_empty() {
            if !text.is_empty() {
                text.push_str("\n\n");
            }
            text.push_str(&reply.text);
        }
        if !reply.replay.is_empty() {
            messages.push(Message::Replay {
                provider: provider
                    .replay_provider()
                    .ok_or("The provider returned replay data without its format identifier.")?
                    .into(),
                items: reply.replay.clone(),
            });
        }
        if reply.calls.is_empty() {
            let empty = reply.text.trim().is_empty();
            messages.push(Message::Assistant {
                text: reply.text,
                calls: Vec::new(),
            });
            output.context(&messages).await?;
            return if empty {
                Err("The provider returned an empty reply.".into())
            } else {
                Ok(text)
            };
        }
        for call in &reply.calls {
            if call.id.is_empty()
                || previous_calls.contains(&call.id)
                || !call_ids.insert(call.id.clone())
            {
                return Err("The provider repeated a tool call identifier.".into());
            }
        }
        messages.push(Message::Assistant {
            text: reply.text,
            calls: reply.calls.clone(),
        });
        output.context(&messages).await?;
        for call in reply.calls {
            if *stop.borrow() {
                return Err(
                    "Stopped by you. Completed Record and file operations remain saved.".into(),
                );
            }
            output.tool_pending(&call).await?;
            let mut result = if call.name == receipts::READ_TOOL_OUTPUT
                && let Some(tool_outputs) = &tool_outputs
            {
                match tool_outputs.read(call.arguments) {
                    Ok(page) => json!({"ok":true,"result":page}),
                    Err(error) => json!({"ok":false,"error":error}),
                }
            } else {
                tokio::select! {
                    biased;
                    _ = stop.changed() => return Err("Stopped by you. Completed Record, file and command operations remain saved.".into()),
                    result = tools.run_with_receipts(&call.name, call.arguments, tool_outputs.as_ref()) => result,
                }
            };
            let diff = if matches!(
                call.name.as_str(),
                "create_file" | "write_file" | "edit_file" | "run_command"
            ) {
                result
                    .get_mut("result")
                    .and_then(Value::as_object_mut)
                    .and_then(|result| result.remove("diff"))
            } else {
                None
            };
            if receipts.len() == 64 {
                receipts.remove(0);
            }
            receipts.push(format!(
                "{} [{}]: {}",
                call.name.chars().take(64).collect::<String>(),
                call.id.chars().take(64).collect::<String>(),
                if result["ok"] == true {
                    "completed"
                } else {
                    "failed"
                },
            ));
            messages.push(Message::Tool {
                id: call.id,
                name: call.name,
                result,
            });
            output.context(&messages).await?;
            if let Some(Message::Tool { id, result, .. }) = messages.last() {
                output
                    .tool_finished(id, result, diff.as_ref().and_then(Value::as_str))
                    .await;
            }
        }
    }
}
