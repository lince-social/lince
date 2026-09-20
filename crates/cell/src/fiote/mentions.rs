use super::*;
use std::collections::BTreeSet;

fn references(body: &str) -> BTreeSet<String> {
    let mut references = BTreeSet::new();
    let mut code = 0;
    let mut chars = body.chars().peekable();
    let mut previous = None;
    while let Some(ch) = chars.next() {
        if ch == '`' {
            let mut count = 1;
            while chars.peek() == Some(&'`') {
                chars.next();
                count += 1;
            }
            if code == 0 {
                code = count;
            } else if code == count {
                code = 0;
            }
        } else if ch == '@'
            && code == 0
            && previous.is_none_or(|ch: char| ch.is_whitespace() || "([{".contains(ch))
        {
            let mut reference = String::new();
            if chars.peek() == Some(&'"') {
                chars.next();
                let mut closed = false;
                for ch in chars.by_ref() {
                    if ch == '"' {
                        closed = true;
                        break;
                    }
                    if ch == '\n' {
                        break;
                    }
                    reference.push(ch);
                }
                if !closed {
                    reference.clear();
                }
            } else {
                while let Some(ch) = chars.peek().copied() {
                    if !ch.is_alphanumeric() && ch != '-' && ch != '_' {
                        break;
                    }
                    reference.push(ch);
                    chars.next();
                }
            }
            if !reference.is_empty() {
                references.insert(reference);
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
}
