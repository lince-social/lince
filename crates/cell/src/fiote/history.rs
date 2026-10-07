use super::*;
use fiote::{conversation::Summary, provider::ToolCall};

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Journal {
    pub source: Option<String>,
    pub source_fingerprint: String,
    pub body: Option<String>,
    pub frames: Vec<Message>,
    pub pending: Option<ToolCall>,
}

pub(super) fn path(directory: &Path, message: &str) -> PathBuf {
    directory.join(format!("native-{message}.json"))
}

pub(super) fn load(path: &Path) -> Result<Option<Journal>, String> {
    match std::fs::read(path) {
        Ok(bytes) if bytes.len() <= 16 * 1024 * 1024 => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| "The native conversation journal is damaged.".into()),
        Ok(_) => Err("The native conversation journal exceeds its limit.".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn checkpoint(path: &Path, messages: &[Message]) -> Result<(), String> {
    let mut journal = load(path)?.unwrap_or_default();
    let user = messages
        .iter()
        .rposition(|message| matches!(message, Message::User(_) | Message::RichUser { .. }))
        .ok_or("The native turn has no source message.")?;
    journal.source_fingerprint = fiote::conversation::fingerprint(&messages[user..user + 1])?;
    journal.frames = messages[user + 1..].to_vec();
    if journal.pending.as_ref().is_some_and(|call| {
        journal
            .frames
            .iter()
            .any(|message| matches!(message, Message::Tool { id, .. } if id == &call.id))
    }) {
        journal.pending = None;
    }
    if serde_json::to_vec(&journal)
        .map_err(|e| e.to_string())?
        .len()
        > 16 * 1024 * 1024
    {
        return Err("The native turn exceeds its journal limit.".into());
    }
    save(path, &journal)
}

impl Host {
    pub(super) async fn restore_frames(
        &self,
        message: &str,
        body: &str,
        messages: &[Message],
        seen: &std::collections::HashSet<String>,
    ) -> Result<Option<Vec<Message>>, String> {
        let Some(journal) = load(&path(&self.directory, message))? else {
            return Ok(None);
        };
        if journal.body.as_deref().is_some_and(|saved| saved != body) {
            return Ok(None);
        }
        let Some(source) = journal.source else {
            return Ok(None);
        };
        if !seen.contains(&source) {
            return Ok(None);
        }
        let Some(user) = store::records::get(&self.engine.store.pool, &source)
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let content = store::message_content::load(&self.engine.store.pool, &source)
            .await
            .map_err(|e| e.to_string())?;
        let user = if content.is_empty() {
            Message::User(user.body)
        } else {
            Message::RichUser {
                text: user.body,
                content,
            }
        };
        if fiote::conversation::fingerprint(&[user])? != journal.source_fingerprint {
            return Ok(None);
        }
        if journal.frames.is_empty() {
            return Ok(None);
        }
        let mut frames = journal.frames;
        let results: std::collections::HashSet<_> = frames
            .iter()
            .filter_map(|m| {
                if let Message::Tool { id, .. } = m {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();
        let mut unresolved = Vec::new();
        for frame in &frames {
            if let Message::Assistant { calls, .. } = frame {
                for call in calls.iter().filter(|c| !results.contains(&c.id)) {
                    unresolved.push(Message::Tool { id: call.id.clone(), name: call.name.clone(), result: serde_json::json!({"ok":false,"outcome":"unknown","error":"This turn was interrupted. Inspect the current Record/canvas/file state and report uncertainty. Do not repeat the effect unless the user explicitly requests it."}) });
                }
            }
        }
        frames.extend(unresolved);
        if !matches!(frames.last(), Some(Message::Assistant { calls, .. }) if calls.is_empty()) {
            frames.push(Message::Assistant {
                text: format!(
                    "[Interrupted turn; completed tool receipts remain authoritative]\n{body}"
                ),
                calls: Vec::new(),
            });
        }
        let preceding = messages
            .iter()
            .rfind(|m| matches!(m, Message::User(_) | Message::RichUser { .. }));
        if preceding.is_none_or(|m| {
            fiote::conversation::fingerprint(std::slice::from_ref(m))
                .ok()
                .as_deref()
                != Some(&journal.source_fingerprint)
        }) {
            return Ok(None);
        }
        Ok(Some(frames))
    }
    pub(super) fn summarized_history(
        &self,
        thread: &str,
        messages: &[Message],
    ) -> Result<Vec<Message>, String> {
        let path = self.directory.join(format!("summary-{thread}.json"));
        match std::fs::read(path) {
            Ok(bytes) if bytes.len() <= 65536 => {
                let summary: Summary = serde_json::from_slice(&bytes)
                    .map_err(|_| "The saved conversation summary is damaged.")?;
                Ok(summary
                    .apply(messages)?
                    .unwrap_or_else(|| messages.to_vec()))
            }
            Ok(_) => Err("The saved conversation summary exceeds its limit.".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(messages.to_vec()),
            Err(e) => Err(e.to_string()),
        }
    }
}
