use sqlx::{Row, SqliteConnection};

use super::{
    ConceptIds, RecordKinds,
    queries::{CONCEPTS, PARENTS, RECORDS},
};
use crate::{
    StoreError,
    access_snapshot::{
        ConceptMetadata, ConceptParentMetadata, RecordMetadata, protocol,
        sql::checked_rows,
        validate::{record_kind, require_record_kind, require_uid},
    },
};
use nucleus::RecordKind;

pub(super) async fn records_on(
    connection: &mut SqliteConnection,
    max_bytes: i64,
    count: usize,
) -> Result<Vec<RecordMetadata>, StoreError> {
    let record_rows = checked_rows(
        connection,
        RECORDS.read,
        max_bytes,
        count,
        "Record metadata",
    )
    .await?;
    let mut records = Vec::with_capacity(record_rows.len());
    for row in record_rows {
        let uid: String = row.try_get("uid")?;
        require_uid(&uid, "r", "Record")?;
        let kind_value: &str = row.try_get("kind")?;
        let kind = record_kind(kind_value)?;
        let organ_uid: Option<String> = row.try_get("organ_uid")?;
        let replica_root: Option<String> = row.try_get("replica_root")?;
        if let Some(value) = &organ_uid {
            require_uid(value, "r", "Record Organ")?;
        }
        if let Some(value) = &replica_root {
            require_uid(value, "r", "replica root")?;
        }
        if records
            .last()
            .is_some_and(|record: &RecordMetadata| record.uid == uid)
        {
            return Err(protocol("duplicate Record identity in snapshot"));
        }
        records.push(RecordMetadata {
            uid,
            kind,
            organ_uid,
            deleted: row.try_get::<i64, _>("deleted")? != 0,
            replica_root,
        });
    }
    Ok(records)
}

pub(super) async fn concepts_on(
    connection: &mut SqliteConnection,
    max_bytes: i64,
    count: usize,
) -> Result<Vec<ConceptMetadata>, StoreError> {
    let concept_rows = checked_rows(
        connection,
        CONCEPTS.read,
        max_bytes,
        count,
        "Concept metadata",
    )
    .await?;
    let mut concepts = Vec::with_capacity(concept_rows.len());
    for row in concept_rows {
        let uid: String = row.try_get("uid")?;
        require_uid(&uid, "c", "Concept")?;
        let name: String = row.try_get("canonical_name")?;
        if name.is_empty() {
            return Err(protocol("stored Concept name is empty"));
        }
        let origin_organ: Option<String> = row.try_get("origin_organ")?;
        if let Some(value) = &origin_organ {
            require_uid(value, "r", "Concept origin Organ")?;
        }
        if concepts
            .last()
            .is_some_and(|concept: &ConceptMetadata| concept.uid == uid)
        {
            return Err(protocol("duplicate Concept identity in snapshot"));
        }
        concepts.push(ConceptMetadata {
            uid,
            name,
            origin_organ,
        });
    }
    Ok(concepts)
}

pub(super) async fn parents_on(
    connection: &mut SqliteConnection,
    max_bytes: i64,
    count: usize,
    concept_ids: &ConceptIds<'_>,
) -> Result<Vec<ConceptParentMetadata>, StoreError> {
    let parent_rows = checked_rows(
        connection,
        PARENTS.read,
        max_bytes,
        count,
        "Concept parent metadata",
    )
    .await?;
    let mut concept_parents = Vec::with_capacity(parent_rows.len());
    for row in parent_rows {
        let concept_uid: String = row.try_get("concept_uid")?;
        let parent_uid: String = row.try_get("parent_uid")?;
        require_uid(&concept_uid, "c", "Concept parent child")?;
        require_uid(&parent_uid, "c", "Concept parent")?;
        if !concept_ids.contains(concept_uid.as_str()) || !concept_ids.contains(parent_uid.as_str())
        {
            return Err(protocol("Concept parent edge has a missing endpoint"));
        }
        concept_parents.push(ConceptParentMetadata {
            concept_uid,
            parent_uid,
        });
    }
    Ok(concept_parents)
}

pub(super) fn validate_origins(
    records: &[RecordMetadata],
    concepts: &[ConceptMetadata],
    record_kinds: &RecordKinds<'_>,
) -> Result<(), StoreError> {
    for record in records {
        if let Some(organ_uid) = &record.organ_uid {
            require_record_kind(
                record_kinds,
                organ_uid,
                &[RecordKind::Organ],
                "Record Organ origin",
            )?;
        }
        if let Some(root_uid) = &record.replica_root
            && !record_kinds.contains_key(root_uid.as_str())
        {
            return Err(protocol("Record references a missing replica root"));
        }
    }
    for concept in concepts {
        if let Some(origin_uid) = &concept.origin_organ {
            require_record_kind(
                record_kinds,
                origin_uid,
                &[RecordKind::Organ],
                "Concept origin Organ",
            )?;
        }
    }
    Ok(())
}
