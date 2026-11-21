use std::collections::BTreeSet;

use nucleus::{RecordKind, valid_uid};
use serde_json::Value;
use sqlx::{Row, SqliteConnection};

use super::{
    ConceptIds, RecordKinds,
    queries::{DRAFTS, LINGUAS, MEMBERSHIPS, VISIBILITY},
};
use crate::{
    StoreError,
    access_snapshot::{
        LinguaConceptMetadata, LinguaMetadata, MessageDraftMetadata, VisibilityRuleMetadata,
        VisibilitySubject, VisibilityTarget, protocol,
        sql::checked_rows,
        validate::{
            lingua_visibility, object, parse_role_identity, require_lingua_uid,
            require_record_kind, require_uid, visibility_grant,
        },
    },
};

pub(super) async fn visibility_on(
    connection: &mut SqliteConnection,
    max_bytes: i64,
    count: usize,
    record_kinds: &RecordKinds<'_>,
    concept_ids: &ConceptIds<'_>,
    places: &BTreeSet<String>,
    role_ids: &BTreeSet<i64>,
) -> Result<Vec<VisibilityRuleMetadata>, StoreError> {
    let visibility_rows = checked_rows(
        connection,
        VISIBILITY.read,
        max_bytes,
        count,
        "Visibility rule metadata",
    )
    .await?;
    let mut visibility_rules = Vec::with_capacity(visibility_rows.len());
    for row in visibility_rows {
        let uid: String = row.try_get("uid")?;
        let subject_kind: String = row.try_get("subject_kind")?;
        let subject_uid: Option<String> = row.try_get("subject_uid")?;
        let target_uid: String = row.try_get("target_uid")?;
        let field: Option<String> = row.try_get("field")?;
        require_uid(&uid, "v", "visibility rule")?;
        if subject_kind.is_empty() {
            return Err(protocol("visibility rule has an empty subject kind"));
        }
        if visibility_rules
            .last()
            .is_some_and(|rule: &VisibilityRuleMetadata| rule.uid == uid)
        {
            return Err(protocol("duplicate visibility rule identity in snapshot"));
        }
        if field.as_deref().is_some_and(str::is_empty) {
            return Err(protocol("visibility rule has an empty field"));
        }
        let subject = match subject_kind.as_str() {
            "public" => {
                if subject_uid.is_some() {
                    return Err(protocol("public visibility rule has a subject identity"));
                }
                VisibilitySubject::Public
            }
            "actor" => {
                let value = subject_uid
                    .ok_or_else(|| protocol("actor visibility rule has no subject identity"))?;
                require_uid(&value, "r", "visibility actor")?;
                require_record_kind(
                    record_kinds,
                    &value,
                    &[RecordKind::Person, RecordKind::Organ],
                    "visibility actor",
                )?;
                VisibilitySubject::Actor(value)
            }
            "organ" => {
                let value = subject_uid
                    .ok_or_else(|| protocol("Organ visibility rule has no subject identity"))?;
                require_uid(&value, "r", "visibility Organ")?;
                require_record_kind(
                    record_kinds,
                    &value,
                    &[RecordKind::Organ],
                    "visibility Organ",
                )?;
                VisibilitySubject::Organ(value)
            }
            "role" => {
                let value = subject_uid
                    .as_deref()
                    .ok_or_else(|| protocol("Role visibility rule has no subject identity"))?;
                let role_id = parse_role_identity(value)?;
                if !role_ids.contains(&role_id) {
                    return Err(protocol("visibility rule references a missing Role"));
                }
                VisibilitySubject::Role(role_id)
            }
            _ => VisibilitySubject::Unsupported {
                kind: subject_kind,
                uid: subject_uid,
            },
        };
        let target = if valid_uid(&target_uid, "r") {
            if !record_kinds.contains_key(target_uid.as_str()) {
                return Err(protocol("visibility rule references a missing Record"));
            }
            VisibilityTarget::Record(target_uid)
        } else if valid_uid(&target_uid, "c") {
            if !concept_ids.contains(target_uid.as_str()) {
                return Err(protocol("visibility rule references a missing Concept"));
            }
            VisibilityTarget::Concept(target_uid)
        } else if valid_uid(&target_uid, "pl") {
            if !places.contains(&target_uid) {
                return Err(protocol("visibility rule references a missing Place"));
            }
            VisibilityTarget::Place(target_uid)
        } else {
            return Err(protocol("visibility rule has an invalid target identity"));
        };
        visibility_rules.push(VisibilityRuleMetadata {
            uid,
            subject,
            target,
            field,
            grant: { visibility_grant(row.try_get("grant_level")?)? },
        });
    }
    Ok(visibility_rules)
}

pub(super) async fn linguas_on(
    connection: &mut SqliteConnection,
    max_bytes: i64,
    count: usize,
    record_kinds: &RecordKinds<'_>,
) -> Result<Vec<LinguaMetadata>, StoreError> {
    let lingua_rows = checked_rows(
        connection,
        LINGUAS.read,
        max_bytes,
        count,
        "Lingua metadata",
    )
    .await?;
    let mut linguas = Vec::with_capacity(lingua_rows.len());
    for row in lingua_rows {
        let uid: String = row.try_get("uid")?;
        let name: String = row.try_get("name")?;
        let owner_organ: Option<String> = row.try_get("owner_organ")?;
        require_lingua_uid(&uid)?;
        if name.is_empty() {
            return Err(protocol("stored Lingua name is empty"));
        }
        if let Some(value) = &owner_organ {
            require_uid(value, "r", "Lingua owner Organ")?;
            require_record_kind(
                record_kinds,
                value,
                &[RecordKind::Organ],
                "Lingua owner Organ",
            )?;
        }
        if linguas
            .last()
            .is_some_and(|lingua: &LinguaMetadata| lingua.uid == uid)
        {
            return Err(protocol("duplicate Lingua identity in snapshot"));
        }
        let visibility_value: &str = row.try_get("visibility")?;
        linguas.push(LinguaMetadata {
            uid,
            name,
            owner_organ,
            visibility: lingua_visibility(visibility_value)?,
        });
    }
    Ok(linguas)
}

pub(super) async fn lingua_concepts_on(
    connection: &mut SqliteConnection,
    max_bytes: i64,
    count: usize,
    linguas: &[LinguaMetadata],
    concept_ids: &ConceptIds<'_>,
) -> Result<Vec<LinguaConceptMetadata>, StoreError> {
    let membership_rows = checked_rows(
        connection,
        MEMBERSHIPS.read,
        max_bytes,
        count,
        "Lingua membership metadata",
    )
    .await?;
    let lingua_ids: BTreeSet<_> = linguas.iter().map(|row| row.uid.as_str()).collect();
    let mut lingua_concepts = Vec::with_capacity(membership_rows.len());
    for row in membership_rows {
        let lingua_uid: String = row.try_get("lingua_uid")?;
        let concept_uid: String = row.try_get("concept_uid")?;
        require_lingua_uid(&lingua_uid)?;
        require_uid(&concept_uid, "c", "Lingua member Concept")?;
        if !lingua_ids.contains(lingua_uid.as_str()) || !concept_ids.contains(concept_uid.as_str())
        {
            return Err(protocol("Lingua membership has a missing dependency"));
        }
        lingua_concepts.push(LinguaConceptMetadata {
            lingua_uid,
            concept_uid,
        });
    }
    Ok(lingua_concepts)
}

pub(super) async fn drafts_on(
    connection: &mut SqliteConnection,
    max_bytes: i64,
    count: usize,
    record_kinds: &RecordKinds<'_>,
) -> Result<Vec<MessageDraftMetadata>, StoreError> {
    let draft_rows = checked_rows(
        connection,
        DRAFTS.read,
        max_bytes,
        count,
        "MessageDraft metadata",
    )
    .await?;
    let mut message_drafts = Vec::with_capacity(draft_rows.len());
    for row in draft_rows {
        let record_uid: String = row.try_get("uid")?;
        let encoded: Option<&str> = row.try_get("fds")?;
        let metadata = object(
            encoded.ok_or_else(|| protocol("MessageDraft has no ownership metadata"))?,
            "MessageDraft ownership metadata",
        )?;
        let author_uid = metadata
            .get("author")
            .and_then(Value::as_str)
            .ok_or_else(|| protocol("MessageDraft has no author identity"))?
            .to_string();
        let operator_uid = metadata
            .get("operator")
            .and_then(Value::as_str)
            .ok_or_else(|| protocol("MessageDraft has no operator identity"))?
            .to_string();
        require_uid(&author_uid, "r", "MessageDraft author")?;
        require_uid(&operator_uid, "r", "MessageDraft operator")?;
        require_record_kind(
            record_kinds,
            &author_uid,
            &[RecordKind::Person, RecordKind::Organ],
            "MessageDraft author",
        )?;
        require_record_kind(
            record_kinds,
            &operator_uid,
            &[RecordKind::Person, RecordKind::Organ],
            "MessageDraft operator",
        )?;
        let thread_uid = match metadata.get("thread") {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) => {
                require_uid(value, "r", "MessageDraft Thread")?;
                require_record_kind(
                    record_kinds,
                    value,
                    &[RecordKind::Thread],
                    "MessageDraft Thread",
                )?;
                Some(value.clone())
            }
            Some(_) => return Err(protocol("MessageDraft Thread identity is not a string")),
        };
        message_drafts.push(MessageDraftMetadata {
            record_uid,
            author_uid,
            operator_uid,
            thread_uid,
            metadata,
        });
    }
    Ok(message_drafts)
}
