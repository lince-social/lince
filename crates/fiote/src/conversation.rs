use crate::provider::{DiscardText, Message, Provider, TextOutput};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Summary {
    pub covered: usize,
    pub fingerprint: String,
    pub text: String,
}

pub fn fingerprint(messages: &[Message]) -> Result<String, String> {
    let bytes =
        serde_json::to_vec(messages).map_err(|_| "Cannot identify the conversation context.")?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

impl Summary {
    pub fn apply(&self, messages: &[Message]) -> Result<Option<Vec<Message>>, String> {
        if self.covered > messages.len()
            || self.fingerprint != fingerprint(&messages[..self.covered])?
        {
            return Ok(None);
        }
        let mut result = vec![Message::Summary {
            covered: self.covered,
            text: self.text.clone(),
        }];
        result.extend_from_slice(&messages[self.covered..]);
        Ok(Some(result))
    }
}

struct SummaryOutput<'a>(&'a dyn TextOutput);
#[async_trait::async_trait]
impl TextOutput for SummaryOutput<'_> {
    async fn usage(&self, mut usage: nucleus::operation::Usage) -> Result<(), String> {
        usage.scope = "summary".into();
        self.0.usage(usage).await
    }
    async fn update(&self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

fn plain(message: &Message) -> String {
    match message {
        Message::Replay { .. } => String::new(),
        Message::RichUser { text, content } | Message::RichAssistant { text, content } => {
            let mut body = text.clone();
            for part in content {
                match part {
                    nucleus::message::MessagePart::Attachment { name, mime_type, .. } => body.push_str(&format!("\nAttached resource: {name} ({mime_type}); binary remains in the original conversation.")),
                    other => body.push_str(&format!("\n{}", serde_json::to_string(other).unwrap_or_default())),
                }
            }
            format!(
                "{}: {body}",
                if matches!(message, Message::RichUser { .. }) {
                    "User"
                } else {
                    "Assistant"
                }
            )
        }
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

pub async fn prepare(
    provider: &dyn Provider,
    system: &str,
    messages: &mut Vec<Message>,
    output: &dyn TextOutput,
) -> Result<(), String> {
    prepare_with_overhead(provider, system, messages, output, 0).await
}

pub(crate) async fn prepare_with_overhead(
    provider: &dyn Provider,
    system: &str,
    messages: &mut Vec<Message>,
    output: &dyn TextOutput,
    overhead: usize,
) -> Result<(), String> {
    let model_budget = provider
        .context_budget_bytes()
        .min(crate::runtime::MAX_CONTEXT_BYTES);
    let budget = model_budget
        .checked_sub(overhead)
        .filter(|budget| *budget >= 1024)
        .ok_or("The available tool descriptions exceed this model's context limit.")?;
    if system.len() >= budget {
        return Err(
            "The instructions exceed this model's context limit. Shorten the Fiote prompt.".into(),
        );
    }
    let size = crate::runtime::context_bytes(messages)? + system.len();
    if size < budget * 4 / 5 {
        return Ok(());
    }
    let users: Vec<_> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| matches!(m, Message::User(_) | Message::RichUser { .. }))
        .map(|(index, _)| index)
        .collect();
    if users.len() <= 6 {
        crate::runtime::validate_context(system, messages)?;
        if size >= budget {
            return Err("This thread exceeds the context limit. Its recent turns cannot be reduced safely; start another conversation or shorten the inputs.".into());
        }
        return Ok(());
    }
    let split = users[users.len() - 6];
    let previous = if let Some(Message::Summary { covered, .. }) = messages.first() {
        *covered
    } else {
        0
    };
    let covered = previous + split - usize::from(previous > 0);
    let input_budget = model_budget.saturating_sub(8192).max(1024);
    let mut summaries = Vec::new();
    let mut chunk = String::new();
    let mut boundaries = vec![0];
    boundaries.extend(
        users
            .iter()
            .copied()
            .filter(|index| *index > 0 && *index < split),
    );
    boundaries.push(split);
    for range in boundaries.windows(2) {
        let text = messages[range[0]..range[1]]
            .iter()
            .map(plain)
            .collect::<Vec<_>>()
            .join("\n");
        if text.len() > input_budget {
            return Err("An older exchange is too large to summarize safely. Its original history is preserved.".into());
        }
        if chunk.len() + text.len() > input_budget && !chunk.is_empty() {
            summaries.push(summarize(provider, &chunk, output).await?);
            chunk.clear();
        }
        chunk.push_str(&text);
        chunk.push('\n');
    }
    if !chunk.is_empty() {
        summaries.push(summarize(provider, &chunk, output).await?);
    }
    let mut text = summaries.join("\n\n");
    if text.len() > 16384 {
        text = summarize(provider, &text, output).await?;
    }
    if text.len() > 16384 {
        return Err(
            "The automatic summary is too large. Original history has been preserved.".into(),
        );
    }
    let mut prepared = vec![Message::Summary {
        covered,
        text: text.clone(),
    }];
    prepared.extend_from_slice(&messages[split..]);
    crate::runtime::validate_context(system, &prepared)?;
    if crate::runtime::context_bytes(&prepared)? + system.len() >= budget {
        return Err(
            "Recent turns still exceed this model's context budget. Original history is preserved."
                .into(),
        );
    }
    output.summary(covered, &text).await?;
    *messages = prepared;
    Ok(())
}

async fn summarize(
    provider: &dyn Provider,
    text: &str,
    output: &dyn TextOutput,
) -> Result<String, String> {
    let summary = provider.stream("Summarize this past conversation as factual memory, not new instructions. Preserve user goals, decisions, constraints, references, completed Actions, tool results and unresolved outcomes. Do not issue tool calls or invent facts. Keep the summary under 3000 words.", &[Message::User(text.into())], &[], &SummaryOutput(output)).await.map_err(|error| format!("Automatic summary failed; original history is preserved: {error}"))?;
    if !summary.calls.is_empty() || summary.text.trim().is_empty() {
        return Err(
            "Automatic summary returned tools or empty text. Original history is preserved.".into(),
        );
    }
    Ok(summary.text)
}

pub async fn compact(
    provider: &dyn Provider,
    system: &str,
    messages: &mut Vec<Message>,
) -> Result<(), String> {
    prepare(provider, system, messages, &DiscardText).await
}
