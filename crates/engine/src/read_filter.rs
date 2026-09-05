use std::collections::HashSet;

use crate::Engine;
use crate::error::EngineError;

pub struct PersonFilter {
    pub predicate: protein::Predicate,
}

impl Engine {
    pub async fn read_filter_of(
        &self,
        person_uid: &str,
    ) -> Result<Option<PersonFilter>, EngineError> {
        let Some(raw) = store::read_filter::get(&self.store.pool, person_uid).await? else {
            return Ok(None);
        };
        let predicate: protein::Predicate = serde_json::from_str(&raw).map_err(|error| {
            EngineError::Consequence(format!(
                "the read filter saved for {person_uid} cannot be read: {error}"
            ))
        })?;
        Ok(Some(PersonFilter { predicate }))
    }

    pub async fn set_read_filter(
        &self,
        person_uid: &str,
        predicate: Option<&protein::Predicate>,
    ) -> Result<(), EngineError> {
        let raw = match predicate {
            Some(predicate) => Some(
                serde_json::to_string(predicate)
                    .map_err(|error| EngineError::Consequence(error.to_string()))?,
            ),
            None => None,
        };
        store::read_filter::set(&self.store.pool, person_uid, raw.as_deref()).await?;
        Ok(())
    }

    pub async fn readable_by(
        &self,
        person_uid: &str,
    ) -> Result<Option<HashSet<String>>, EngineError> {
        let Some(filter) = self.read_filter_of(person_uid).await? else {
            return Ok(None);
        };
        let protein = protein::Protein {
            source: protein::Source::Record,
            filter: vec![filter.predicate],
            fields: None,
            include: Default::default(),
            aggregate: None,
            order: vec![],
            limit: None,
        };
        let matched = protein::matching_records(&self.store, &protein, None).await?;
        Ok(Some(matched.into_iter().map(|row| row.uid).collect()))
    }

    pub(crate) async fn record_targets_of(
        &self,
        action: &crate::actions::Action,
    ) -> Result<Vec<String>, EngineError> {
        use crate::actions::Action;
        let named: Vec<&String> = match action {
            Action::CreateMessage { thread, .. } => vec![thread],
            Action::CreateMessageDraft {
                conversation,
                thread,
                ..
            } => vec![conversation, thread],
            Action::SetQuantity { target, .. }
            | Action::SetQuantityExact { target, .. }
            | Action::EditRecordText { target, .. }
            | Action::ReviseMessage {
                message: target, ..
            }
            | Action::ReviseMessageDraft { draft: target, .. }
            | Action::DeleteMessageDraft { draft: target }
            | Action::SendMessageDraft { draft: target }
            | Action::DeleteRecord { target }
            | Action::MoveRecordTo { record: target, .. }
            | Action::CancelRecordMove { record: target } => vec![target],
            _ => Vec::new(),
        };
        let mut out = Vec::new();
        for name in named {
            if let Ok(uid) = self.resolve(name).await {
                out.push(uid);
            }
        }
        Ok(out)
    }

    pub async fn refuse_unreadable(
        &self,
        actor: Option<&str>,
        targets: &[String],
    ) -> Result<(), EngineError> {
        if targets.is_empty() {
            return Ok(());
        }
        let Some(actor) = actor else {
            return Ok(());
        };
        let Some(readable) = self.readable_by(actor).await? else {
            return Ok(());
        };
        for target in targets {
            if !readable.contains(target) {
                return Err(EngineError::Consequence(format!(
                    "{target} is outside what this login may see, so it may not be changed either"
                )));
            }
        }
        Ok(())
    }
}
