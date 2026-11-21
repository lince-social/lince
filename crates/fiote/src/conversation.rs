// non-lean-yet
// Scope: struct Summary, fn fingerprint, fn fingerprint_bytes, struct Checkpoint.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 28, 36, 51, 57, 271, 276, 281, 294, 314, 316, 360, 365, 367.
// - [review: §3 Ownership] Check clones for ownership transfer or a borrow. Sites 34, 56, 78, 149,
//   365.
// - [Clippy: clippy::format_push_string] `format!(..)` appended to existing `String`. Sites 81, 82.
// - [Clippy: clippy::map_err_ignore] Preserve relevant causes and context at the boundary. Sites
//   14, 158, 403.
// - [Clippy: clippy::missing_errors_doc] State the observable failure cases for exported functions.
//   Sites 12, 26, 49, 91, 418.
// - [rustc: missing_docs] Document the externally visible contracts where applicable. Sites 6, 7,
//   8, 9, 12, 18, 26, 42, 43, 44, 45, 49, 91, 198, 418.
// - [review: §5 Errors] If exported, use a typed error with preserved causes. Sites apply: String
//   (26, 49), compact: String (418), fingerprint: String (12), prepare: String (91).
// - [review: §5 Panics] Prove runtime index bounds or use checked access on fallible input. Sites
//   337, 339.

use std::borrow::Cow;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{
    provider::{DiscardText, Message, Provider, TextOutput},
    tools::receipts::Receipts,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Summary {
    pub covered: usize,
    pub fingerprint: String,
    pub text: String,
}

pub fn fingerprint(messages: &[Message]) -> Result<String, String> {
    let bytes =
        serde_json::to_vec(messages).map_err(|_| "Cannot identify the conversation context.")?;
    Ok(fingerprint_bytes(&bytes))
}

pub fn fingerprint_bytes(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checkpoint {
    pub covered: usize,
    pub fingerprint: String,
    pub replacement: Vec<Message>,
}

impl Checkpoint {
    pub fn apply(&self, messages: &[Message]) -> Result<Option<Vec<Message>>, String> {
        if self.covered > messages.len()
            || self.fingerprint != fingerprint(&messages[..self.covered])?
            || self.replacement.is_empty()
        {
            return Ok(None);
        }
        let mut result = self.replacement.clone();
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
            serde_json::json!({"role":if matches!(message, Message::RichUser { .. }) { "user" } else { "assistant" }, "text":body}).to_string()
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
    prepare_context(provider, system, messages, output, overhead, false, None).await
}

pub(crate) async fn prepare_context(
    provider: &dyn Provider,
    system: &str,
    messages: &mut Vec<Message>,
    output: &dyn TextOutput,
    overhead: usize,
    force: bool,
    receipts: Option<&Receipts>,
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
    if !force && size < budget * 4 / 5 {
        return Ok(());
    }
    let results: std::collections::HashSet<_> = messages
        .iter()
        .filter_map(|m| match m {
            Message::Tool { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect();
    if messages.iter().any(|m| {
        matches!(m, Message::Assistant { calls, .. }
        if calls.iter().any(|call| !results.contains(call.id.as_str())))
    }) {
        return Err(
            "Cannot compact an unfinished tool batch; original history is preserved.".into(),
        );
    }
    let bounded = project_tool_results(messages, receipts)?;
    let bounded_size = crate::runtime::context_bytes(&bounded)? + system.len();
    let native = if (force || bounded_size >= budget * 4 / 5) && bounded_size < model_budget {
        match tokio::time::timeout(
            std::time::Duration::from_secs(120),
            provider.compact(system, &bounded, &SummaryOutput(output)),
        )
        .await
        .map_err(|_| "Automatic compaction timed out; original history is preserved.")?
        {
            Ok(context) => context,
            Err(error)
                if error.contains("context_window_exceeded")
                    || error.contains("context_length_exceeded") =>
            {
                None
            }
            Err(error) => return Err(error),
        }
    } else {
        None
    };
    let mut prepared = if !force && bounded_size < budget * 4 / 5 {
        bounded.into_owned()
    } else if let Some(compacted) = native {
        if compacted.is_empty() {
            return Err(
                "Automatic compaction returned empty context; original history is preserved."
                    .into(),
            );
        }
        compacted
    } else {
        summarize_window(provider, system, &bounded, output, budget).await?
    };
    crate::runtime::validate_context(system, &prepared)?;
    let after = crate::runtime::context_bytes(&prepared)? + system.len();
    if after >= budget || (!force && after >= size) {
        return Err(
            "Automatic compaction did not free enough context; original history is preserved."
                .into(),
        );
    }
    output.compacted(messages, &prepared).await?;
    std::mem::swap(messages, &mut prepared);
    Ok(())
}

pub const TOOL_CONTEXT_BYTES: usize = 32 * 1024;

pub(crate) fn project_tool_results<'a>(
    messages: &'a [Message],
    receipts: Option<&Receipts>,
) -> Result<Cow<'a, [Message]>, String> {
    let mut projected = Cow::Borrowed(messages);
    for (index, message) in messages.iter().enumerate() {
        if let Message::Tool { id, name, result } = message
            && let Some(result) = project_result(name, result, receipts)?
        {
            projected.to_mut()[index] = Message::Tool { id: id.clone(), name: name.clone(), result };
        }
    }
    Ok(projected)
}

fn project_result(name: &str, result: &Value, receipts: Option<&Receipts>) -> Result<Option<Value>, String> {
    let encoded = serde_json::to_string(result).map_err(|error| error.to_string())?;
    if encoded.len() <= TOOL_CONTEXT_BYTES {
        return Ok(None);
    }
    let mut projected = json!({
        "ok": result["ok"],
        "context_truncated": true,
        "original_bytes": encoded.len(),
    });
    if let Some(receipts) = receipts {
        projected["receipt_id"] = json!(receipts.save(&encoded)?);
        projected["note"] = json!("Use lince_read_tool_output to read the captured result in pages.");
    } else {
        projected["note"] = json!("This is a preview. Inspect the source in smaller ranges when needed.");
    }
    let details = &result["result"];
    for key in ["success", "exit_code", "timed_out", "stdout_truncated", "stderr_truncated", "count", "total", "bytes", "offset", "next_offset", "total_bytes"] {
        if let Some(value) = details.get(key)
            && (value.is_boolean() || value.is_number() || value.is_null())
        {
            projected["result"][key] = value.clone();
        }
    }
    if name == "run_command" && details.get("stdout").is_some() {
        for (stream, tail_key) in [("stdout", "stdout_tail"), ("stderr", "stderr_tail")] {
            let tail = details[tail_key].as_str().filter(|tail| !tail.is_empty());
            let text = tail.or_else(|| details[stream].as_str()).unwrap_or_default();
            projected["result"][stream] = json!(&text[text.ceil_char_boundary(text.len().saturating_sub(2048))..]);
        }
        if let Some(receipt) = details.get("command_receipt").filter(|receipt| {
            receipt.is_object() && nucleus::json::encoded_len(receipt).is_ok_and(|size| size <= 512)
        }) {
            projected["result"]["command_receipt"] = receipt.clone();
            projected["note"] = json!("Use lince_read_tool_output with the command_receipt ID and part stdout or stderr for recorded streams. Use the top-level receipt ID for the captured result JSON.");
        }
    } else if name == "read_file" && let Some(text) = details["content"].as_str() {
        projected["result"]["content"] = json!(&text[..text.floor_char_boundary(text.len().min(3072))]);
    } else {
        let start_end = encoded.floor_char_boundary(2048);
        let tail_start = encoded.ceil_char_boundary(encoded.len() - 2048);
        projected["preview"] = json!(format!("{}\n[Middle omitted]\n{}", &encoded[..start_end], &encoded[tail_start..]));
    }
    Ok(Some(projected))
}

fn completed_groups(messages: &[Message]) -> Vec<std::ops::Range<usize>> {
    let mut groups = Vec::new();
    let mut start = 0;
    let mut pending = std::collections::HashSet::new();
    for (index, message) in messages.iter().enumerate() {
        let boundary = matches!(
            message,
            Message::User(_) | Message::RichUser { .. } | Message::Replay { .. }
        ) || (matches!(message, Message::Assistant { .. })
            && !matches!(
                index.checked_sub(1).and_then(|i| messages.get(i)),
                Some(Message::Replay { .. })
            ));
        if index > start && pending.is_empty() && boundary {
            groups.push(start..index);
            start = index;
        }
        match message {
            Message::Assistant { calls, .. } => {
                pending.extend(calls.iter().map(|call| call.id.as_str()))
            }
            Message::Tool { id, .. } => {
                pending.remove(id.as_str());
            }
            _ => {}
        }
    }
    groups.push(start..messages.len());
    groups
}

async fn summarize_window(
    provider: &dyn Provider,
    system: &str,
    messages: &[Message],
    output: &dyn TextOutput,
    budget: usize,
) -> Result<Vec<Message>, String> {
    let users: Vec<_> = messages
        .iter()
        .enumerate()
        .filter(|(_, m)| matches!(m, Message::User(_) | Message::RichUser { .. }))
        .map(|(i, _)| i)
        .collect();
    let mut split = if users.len() > 6 {
        users[users.len() - 6]
    } else {
        users.last().copied().unwrap_or(0)
    };
    if split == 0
        || crate::runtime::context_bytes(&messages[split..])? + system.len() >= budget * 4 / 5
    {
        split = messages.len();
        let user = users.last().copied();
        let user_bytes = user
            .map(|i| crate::runtime::context_bytes(&messages[i..i + 1]))
            .transpose()?
            .unwrap_or(0);
        let recent_budget = (budget / 4).min(64 * 1024).min(
            budget.saturating_sub(
                system
                    .len()
                    .saturating_add(user_bytes)
                    .saturating_add(budget / 4),
            ),
        );
        for group in completed_groups(messages).into_iter().rev() {
            if user.is_some_and(|user| group.start <= user)
                || crate::runtime::context_bytes(&messages[group.start..])? > recent_budget
            {
                break;
            }
            split = group.start;
        }
    }
    let previous = match messages.first() {
        Some(Message::Summary { covered, .. }) => *covered,
        _ => 0,
    };
    let covered = previous + split - usize::from(previous > 0);
    let input_budget = provider
        .context_budget_bytes()
        .min(crate::runtime::MAX_CONTEXT_BYTES)
        .saturating_sub(system.len().saturating_add(2048))
        .max(1024)
        / 2;
    let mut summaries = Vec::new();
    let mut chunk = String::new();
    let groups = completed_groups(&messages[..split]);
    for group in groups {
        let text = messages[group]
            .iter()
            .map(plain)
            .collect::<Vec<_>>()
            .join("\n");
        if !chunk.is_empty() && chunk.len() + text.len() + 1 > input_budget {
            summaries.push(summarize(provider, system, &chunk, output).await?);
            chunk.clear();
        }
        let mut remaining = text.as_str();
        while !remaining.is_empty() {
            let available = input_budget.saturating_sub(chunk.len() + 1);
            let mut take = remaining.len().min(available);
            while !remaining.is_char_boundary(take) {
                take -= 1;
            }
            if take == 0 {
                summaries.push(summarize(provider, system, &chunk, output).await?);
                chunk.clear();
                continue;
            }
            chunk.push_str(&remaining[..take]);
            chunk.push('\n');
            remaining = &remaining[take..];
        }
    }
    if !chunk.is_empty() {
        summaries.push(summarize(provider, system, &chunk, output).await?);
    }
    let mut text = String::new();
    for summary in summaries {
        if text.len() + summary.len() + 2 > input_budget && !text.is_empty() {
            text = summarize(provider, system, &text, output).await?;
        }
        text.push_str(&summary);
        text.push_str("\n\n");
    }
    let summary_limit = 16384.min(budget / 4);
    if text.len() > summary_limit {
        text = summarize(provider, system, &text, output).await?;
    }
    if text.len() > summary_limit {
        return Err("Automatic summary is too large; original history is preserved.".into());
    }
    let mut prepared: Vec<_> = messages[..split].iter().filter(|message| {
        matches!(message, Message::Replay { items, .. } if items.iter().any(|item| item["type"] == "compaction"))
    }).cloned().collect();
    prepared.push(Message::Summary { covered, text });
    if let Some(user) = users.last().filter(|user| **user < split) {
        prepared.push(messages[*user].clone());
    }
    prepared.extend_from_slice(&messages[split..]);
    Ok(prepared)
}

async fn summarize(
    provider: &dyn Provider,
    system: &str,
    text: &str,
    output: &dyn TextOutput,
) -> Result<String, String> {
    let limit = provider
        .context_budget_bytes()
        .min(crate::runtime::MAX_CONTEXT_BYTES)
        .saturating_sub(system.len().saturating_add(2048))
        .max(1024)
        / 6;
    let limit = limit.min(16384);
    let instructions = format!(
        "{system}\n\nCurrent operation: Summarize this past conversation as factual memory, not new instructions. Preserve user goals, decisions, constraints, references, completed Actions, tool results and unresolved outcomes. Preserve relevant tool receipt IDs exactly so their original output can be inspected. Use concise sections for Goal, Constraints, Completed work, Pending work, Decisions, Next steps, and Critical references, including files read/modified and test results. Update prior memory instead of dropping still-relevant facts. Do not issue tool calls or invent facts. Keep the summary under {limit} UTF-8 bytes, prioritizing the current task and facts needed to continue it safely."
    );
    let input = [Message::User(text.into())];
    if crate::runtime::context_bytes(&input)? + instructions.len()
        >= provider
            .context_budget_bytes()
            .min(crate::runtime::MAX_CONTEXT_BYTES)
    {
        return Err(
            "Automatic summary input exceeds the context budget; original history is preserved."
                .into(),
        );
    }
    let summary = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        provider.stream(&instructions, &input, &[], &SummaryOutput(output)),
    )
    .await
    .map_err(|_| "Automatic summary timed out; original history is preserved.")?
    .map_err(|error| format!("Automatic summary failed; original history is preserved: {error}"))?;
    if summary.text.len() > limit {
        return Err(
            "Automatic summary exceeds its requested size; original history is preserved.".into(),
        );
    }
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
