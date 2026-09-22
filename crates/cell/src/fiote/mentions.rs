use super::*;
use std::collections::BTreeSet;

fn linked_reference(text: &str) -> Option<(&str, usize)> {
    if !text.starts_with('@') {
        return None;
    }
    let mut chars = text.char_indices();
    while let Some((index, ch)) = chars.next() {
        match ch {
            '\\' => {
                chars.next()?;
            }
            '[' | '\n' | '\r' => return None,
            ']' => {
                let target = text[index + 1..].strip_prefix("(record:")?;
                let end = target.find(')')?;
                let uid = &target[..end];
                if uid.is_empty()
                    || !uid
                        .chars()
                        .all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '-')
                {
                    return None;
                }
                return Some((uid, index + 1 + "(record:".len() + end + 1));
            }
            _ => {}
        }
    }
    None
}

fn references(body: &str) -> BTreeSet<String> {
    let mut references = BTreeSet::new();
    let mut code = 0;
    let mut rest = body;
    let mut previous = None;
    while let Some(ch) = rest.chars().next() {
        rest = &rest[ch.len_utf8()..];
        if ch == '['
            && code == 0
            && let Some((uid, length)) = linked_reference(rest)
        {
            references.insert(uid.into());
            rest = &rest[length..];
            previous = Some(')');
            continue;
        }
        if ch == '`' {
            let count = 1 + rest.chars().take_while(|ch| *ch == '`').count();
            rest = &rest[count - 1..];
            if code == 0 {
                code = count;
            } else if code == count {
                code = 0;
            }
        } else if ch == '@'
            && code == 0
            && previous.is_none_or(|ch: char| ch.is_whitespace() || "([{".contains(ch))
        {
            if let Some(quoted) = rest.strip_prefix('"') {
                let end = quoted.find(['"', '\n']).unwrap_or(quoted.len());
                if quoted[end..].starts_with('"') && end > 0 {
                    references.insert(quoted[..end].into());
                }
                rest = &quoted[(end + usize::from(end < quoted.len()))..];
            } else {
                let end = rest
                    .chars()
                    .take_while(|ch| ch.is_alphanumeric() || *ch == '-' || *ch == '_')
                    .map(char::len_utf8)
                    .sum::<usize>();
                if end > 0 {
                    references.insert(rest[..end].into());
                }
                rest = &rest[end..];
            }
        }
        previous = Some(ch);
    }
    references
}

impl Host {
    pub(super) async fn mentioned_fiote(
        &self,
        body: &str,
    ) -> Result<Option<(store::records::RecordRow, Configuration)>, String> {
        let mut selected = None;
        let references = references(body);
        if references.len() > 16 {
            return Err("Use at most 16 mentions in one message.".into());
        }
        for reference in references {
            let record = match store::records::resolve(&self.engine.store.pool, &reference)
                .await
                .map_err(|e| e.to_string())?
            {
                Some(record) => record,
                None => {
                    let ids: Vec<String> = store::sqlx::query_scalar("SELECT r.uid FROM record r JOIN record_extension e ON e.record_uid = r.uid WHERE e.namespace = 'lince.fiote' AND r.deleted_at IS NULL AND r.head = ? LIMIT 2")
                        .bind(&reference).fetch_all(&self.engine.store.pool).await.map_err(|e| e.to_string())?;
                    if ids.len() > 1 {
                        return Err(format!(
                            "More than one Fiote is named {reference}. Mention its @slug or @Record identifier."
                        ));
                    }
                    let Some(uid) = ids.first() else { continue };
                    self.record(uid).await?
                }
            };
            if store::records::get_extension(&self.engine.store.pool, &record.uid, "lince.fiote")
                .await
                .map_err(|e| e.to_string())?
                .is_none()
            {
                continue;
            }
            let config = self.load(&record.uid)?.ok_or_else(|| {
                format!(
                    "Configure {} in the Fiote Castle before mentioning it.",
                    record.head
                )
            })?;
            if !config.settings.enabled {
                return Err(format!(
                    "{} has a disabled connection. Enable it in the Fiote Castle.",
                    record.head
                ));
            }
            if let Some((previous, _)) = &selected {
                let previous: &store::records::RecordRow = previous;
                if previous.uid != record.uid {
                    return Err(
                        "Mention one Fiote at a time so its reply has a clear recipient.".into(),
                    );
                }
            }
            selected = Some((record, config));
        }
        Ok(selected)
    }

    pub(super) async fn prepare_mentioned_session(
        &self,
        record: &str,
        thread: &str,
        mentioned: bool,
    ) -> Result<(), String> {
        let changed = self
            .instruction_snapshot(thread)?
            .is_some_and(|snapshot| snapshot["fiote"] != record);
        if mentioned || changed {
            self.agents.close_thread(thread).await;
            let session = self.directory.join(format!("agent-session-{thread}.json"));
            if session.exists() {
                std::fs::remove_file(session).map_err(|e| e.to_string())?;
            }
        }
        if changed {
            std::fs::remove_file(self.instruction_path(thread)?).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mentions_ignore_email_and_code_and_accept_quoted_names() {
        assert_eq!(
            references(
                "@dev, @\"Development Fiote\" (@r_123) mail@dev.test `@inline` ```\n@fenced\n``` @dev"
            ),
            BTreeSet::from(["dev".into(), "Development Fiote".into(), "r_123".into()])
        );
        assert!(references("@\"unfinished\nhello @").is_empty());
    }

    #[test]
    fn selected_mentions_resolve_the_record_instead_of_the_label() {
        assert_eq!(
            references("Hi [@old-slug](record:r_123), [@person](record:r_456) and @dev"),
            BTreeSet::from(["r_123".into(), "r_456".into(), "dev".into()])
        );
        assert!(references("`[@dev](record:r_123)`\n```\n[@dev](record:r_456)\n```").is_empty());
        assert_eq!(
            references("[@Jane \\[work\\]](record:r_789) [@Fiote helper](record:r_123)"),
            BTreeSet::from(["r_789".into(), "r_123".into()])
        );
    }
}
