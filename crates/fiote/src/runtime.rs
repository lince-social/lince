use crate::{
    provider::{Message, Provider},
    tools::Registry,
};
use std::{collections::HashSet, time::Duration};
use tokio::sync::watch;

pub const MAX_CONTEXT_BYTES: usize = 512 * 1024;

pub async fn run(
    provider: &dyn Provider,
    system: &str,
    messages: Vec<Message>,
    tools: &Registry,
    stop: watch::Receiver<bool>,
) -> Result<String, String> {
    let mut receipts = Vec::new();
    let result = turn(provider, system, messages, tools, stop, &mut receipts).await;
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

async fn turn(
    provider: &dyn Provider,
    system: &str,
    mut messages: Vec<Message>,
    tools: &Registry,
    mut stop: watch::Receiver<bool>,
    receipts: &mut Vec<String>,
) -> Result<String, String> {
    let definitions = tools.definitions();
    let mut call_ids = HashSet::new();
    for _ in 0..8 {
        if *stop.borrow() {
            return Err("Stopped by you.".into());
        }
        if system.len()
            + serde_json::to_vec(&messages)
                .map_err(|e| e.to_string())?
                .len()
            > MAX_CONTEXT_BYTES
        {
            return Err("This thread exceeds the context limit. Start another thread.".into());
        }
        let reply = tokio::select! {
            _ = stop.changed() => return Err("Stopped by you.".into()),
            result = tokio::time::timeout(Duration::from_secs(120), provider.complete(system, &messages, &definitions)) => {
                result.map_err(|_| "The provider did not reply within two minutes.".to_string())??
            }
        };
        if reply.calls.is_empty() {
            return if reply.text.trim().is_empty() {
                Err("The provider returned an empty reply.".into())
            } else {
                Ok(reply.text)
            };
        }
        if call_ids.len() + reply.calls.len() > 16 {
            return Err("Stopped at the limit of 16 tool calls.".into());
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
                return Err("Stopped by you. Completed file operations remain on disk.".into());
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
    Err(
        "Stopped at the limit of eight model requests. Completed file operations remain on disk."
            .into(),
    )
}
