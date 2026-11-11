use super::*;
use fiote::{conversation::Summary, provider::ToolCall};

#[derive(Default, Serialize, Deserialize)]
pub(super) struct Journal {
    pub source: Option<String>,
    pub source_fingerprint: String,
    pub body: Option<String>,
    pub frames: Vec<Message>,
    pub pending: Option<ToolCall>,
    #[serde(default)]
    pub context: Vec<Message>,
    #[serde(default)]
    pub archives: Vec<String>,
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

fn archive_path(path: &Path, fingerprint: &str) -> Result<PathBuf, String> {
    if fingerprint.len() != 64 || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Invalid native journal archive identity.".into());
    }
    let stem = path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or("Invalid journal path.")?;
    Ok(path.with_file_name(format!("{stem}-archive-{fingerprint}.json")))
}

pub(super) fn frames(path: &Path, journal: &Journal) -> Result<Vec<Message>, String> {
    let mut frames = Vec::new();
    for identity in &journal.archives {
        let bytes = std::fs::read(archive_path(path, identity)?)
            .map_err(|_| "A native journal archive is missing.")?;
        if bytes.len() > 16 * 1024 * 1024 {
            return Err("A native journal archive exceeds its limit.".into());
        }
        let archived: Vec<Message> =
            serde_json::from_slice(&bytes).map_err(|_| "A native journal archive is damaged.")?;
        if fiote::conversation::fingerprint_bytes(&bytes) != *identity {
            return Err("A native journal archive changed.".into());
        }
        frames.extend(archived);
    }
    frames.extend_from_slice(&journal.frames);
    Ok(frames)
}

pub(super) fn checkpoint(path: &Path, messages: &[Message]) -> Result<(), String> {
    let mut journal = load(path)?.unwrap_or_default();
    if journal.context.is_empty() {
        let user = messages
            .iter()
            .rposition(|message| matches!(message, Message::User(_) | Message::RichUser { .. }))
            .ok_or("The native turn has no source message.")?;
        journal.source_fingerprint = fiote::conversation::fingerprint(&messages[user..user + 1])?;
        journal.frames = messages[user + 1..].to_vec();
    } else {
        let previous = journal.context.len();
        if messages.get(..previous) != Some(journal.context.as_slice()) {
            return Err("The native context changed without a compaction checkpoint; original receipts are preserved.".into());
        }
        journal.frames.extend_from_slice(&messages[previous..]);
    }
    journal.context = messages.to_vec();
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

pub(super) fn replace_context(
    path: &Path,
    before: &[Message],
    after: &[Message],
) -> Result<(), String> {
    let mut journal = load(path)?.ok_or("The native turn journal is missing.")?;
    if journal.context != before || journal.pending.is_some() {
        return Err(
            "The native journal changed during compaction; original receipts are preserved.".into(),
        );
    }
    if !journal.frames.is_empty() {
        let identity = fiote::conversation::fingerprint(&journal.frames)?;
        save(&archive_path(path, &identity)?, &journal.frames)?;
        journal.archives.push(identity);
        journal.frames.clear();
    }
    journal.context = after.to_vec();
    save(path, &journal)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compaction_keeps_archived_receipts_and_appends_only_new_frames() {
        let root = tempfile::tempdir().unwrap();
        let path = path(root.path(), "fixture");
        let first = vec![
            Message::User("Never reboot".into()),
            Message::Assistant {
                text: String::new(),
                calls: vec![ToolCall {
                    id: "one".into(),
                    name: "write_file".into(),
                    arguments: serde_json::json!({}),
                    signatures: None,
                }],
            },
            Message::Tool {
                id: "one".into(),
                name: "write_file".into(),
                result: serde_json::json!({"ok":true,"receipt":"complete"}),
            },
        ];
        checkpoint(&path, &first).unwrap();
        let mut compacted = vec![Message::Replay {
            provider: "chatgpt".into(),
            items: vec![serde_json::json!({"type":"compaction","encrypted_content":"opaque"})],
        }];
        replace_context(&path, &first, &compacted).unwrap();
        checkpoint(&path, &compacted).unwrap();
        let previous = fiote::conversation::fingerprint(&first[1..]).unwrap();
        assert_eq!(
            fiote::conversation::fingerprint(
                &frames(&path, &load(&path).unwrap().unwrap()).unwrap()
            )
            .unwrap(),
            previous
        );
        compacted.push(Message::Assistant {
            text: "Done".into(),
            calls: vec![],
        });
        checkpoint(&path, &compacted).unwrap();
        checkpoint(&path, &compacted).unwrap();
        let journal = load(&path).unwrap().unwrap();
        let restored = frames(&path, &journal).unwrap();
        assert_eq!(restored.len(), 3);
        assert_eq!(
            fiote::conversation::fingerprint(&restored[..2]).unwrap(),
            previous
        );
        let stale = vec![Message::User("Wrong context".into())];
        assert!(replace_context(&path, &stale, &[]).is_err());
        assert!(checkpoint(&path, &stale).is_err());
        assert_eq!(
            frames(&path, &load(&path).unwrap().unwrap()).unwrap().len(),
            3
        );
    }
    #[test]
    fn segmented_receipts_outlive_the_old_total_limit_and_detect_damage() {
        let root = tempfile::tempdir().unwrap();
        let path = path(root.path(), "fixture");
        let mut context = vec![Message::User("Long task".into())];
        checkpoint(&path, &context).unwrap();
        for index in 0..20 {
            context.push(Message::Tool {
                id: index.to_string(),
                name: "read_file".into(),
                result: serde_json::json!({"receipt":index,"text":"x".repeat(1024 * 1024)}),
            });
            checkpoint(&path, &context).unwrap();
            let after = vec![Message::Summary {
                covered: index + 1,
                text: "Continue work".into(),
            }];
            replace_context(&path, &context, &after).unwrap();
            context = after;
        }
        let journal = load(&path).unwrap().unwrap();
        assert_eq!(journal.archives.len(), 20);
        assert!(std::fs::metadata(&path).unwrap().len() < 8192);
        let receipts = frames(&path, &journal).unwrap();
        assert_eq!(receipts.len(), 20);
        assert!(serde_json::to_vec(&receipts).unwrap().len() > 16 * 1024 * 1024);
        let archive = archive_path(&path, &journal.archives[0]).unwrap();
        std::fs::write(archive, b"[]").unwrap();
        assert!(
            frames(&path, &journal)
                .unwrap_err()
                .contains("archive changed")
        );
        assert!(archive_path(&path, "../outside").is_err());
    }

    #[test]
    fn unresolved_tool_execution_survives_compaction_failure() {
        let root = tempfile::tempdir().unwrap();
        let path = path(root.path(), "fixture");
        let call = ToolCall {
            id: "pending".into(),
            name: "write_file".into(),
            arguments: serde_json::json!({}),
            signatures: None,
        };
        let before = vec![
            Message::User("Work".into()),
            Message::Assistant {
                text: String::new(),
                calls: vec![call.clone()],
            },
        ];
        checkpoint(&path, &before).unwrap();
        let mut journal = load(&path).unwrap().unwrap();
        journal.pending = Some(call);
        save(&path, &journal).unwrap();
        assert!(replace_context(&path, &before, &[]).is_err());
        assert_eq!(load(&path).unwrap().unwrap().pending.unwrap().id, "pending");
    }
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
        let Some(source) = &journal.source else {
            return Ok(None);
        };
        if !seen.contains(source) {
            return Ok(None);
        }
        let Some(user) = store::records::get(&self.engine.store.pool, source)
            .await
            .map_err(|e| e.to_string())?
        else {
            return Ok(None);
        };
        let content = store::message_content::load(&self.engine.store.pool, source)
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
        let mut frames = frames(&path(&self.directory, message), &journal)?;
        if frames.is_empty() {
            return Ok(None);
        }
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
        provider: Option<&str>,
    ) -> Result<Vec<Message>, String> {
        let path = self.directory.join(format!("summary-{thread}.json"));
        match std::fs::read(path) {
            Ok(bytes) if bytes.len() <= 16 * 1024 * 1024 => {
                let value: serde_json::Value = serde_json::from_slice(&bytes)
                    .map_err(|_| "The saved conversation checkpoint is damaged.")?;
                let prepared = if value.get("replacement").is_some() {
                    serde_json::from_value::<fiote::conversation::Checkpoint>(value)
                        .map_err(|_| "The saved conversation checkpoint is damaged.")?
                        .apply(messages)?
                } else {
                    serde_json::from_value::<Summary>(value)
                        .map_err(|_| "The saved conversation summary is damaged.")?
                        .apply(messages)?
                };
                Ok(prepared.filter(|replacement| !replacement.iter().any(|message| {
                    matches!(message, Message::Replay { provider: owner, items }
                        if items.iter().any(|item| item["type"] == "compaction") && Some(owner.as_str()) != provider)
                })).unwrap_or_else(|| messages.to_vec()))
            }
            Ok(_) => Err("The saved conversation checkpoint exceeds its limit.".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(messages.to_vec()),
            Err(e) => Err(e.to_string()),
        }
    }
}
