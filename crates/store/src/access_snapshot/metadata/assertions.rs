use std::collections::{BTreeMap, BTreeSet};

use sqlx::{Row, SqliteConnection, query_as};

use super::{ConceptIds, RecordKinds, queries::ASSERTIONS};
use crate::{
    StoreError,
    access_snapshot::{
        AssertionMetadata, AssertionRole, protocol,
        sql::checked_rows,
        validate::{assertion_role, read_quantity, require_uid},
    },
};

pub(super) async fn assertions_on(
    connection: &mut SqliteConnection,
    max_bytes: i64,
    count: usize,
    record_kinds: &RecordKinds<'_>,
    concept_ids: &ConceptIds<'_>,
) -> Result<Vec<AssertionMetadata>, StoreError> {
    let assertion_rows = checked_rows(
        connection,
        ASSERTIONS.read,
        max_bytes,
        count,
        "Assertion metadata",
    )
    .await?;
    let mut assertions = Vec::with_capacity(assertion_rows.len());
    for row in assertion_rows {
        let uid: String = row.try_get("uid")?;
        let subject_uid: String = row.try_get("subject_uid")?;
        let predicate_uid: String = row.try_get("predicate_uid")?;
        let object_uid: Option<String> = row.try_get("object_uid")?;
        let object_uri: Option<String> = row.try_get("object_uri")?;
        if object_uri.as_ref().is_some_and(|uri| {
            uri.is_empty() || uri.len() > 8192 || uri.chars().any(char::is_control)
        }) || (object_uri.is_some() && object_uid.is_some())
        {
            return Err(protocol("Invalid URI assertion endpoint"));
        }
        let unit_uid: Option<String> = row.try_get("unit_uid")?;
        require_uid(&uid, "a", "Assertion")?;
        require_uid(&subject_uid, "r", "Assertion subject")?;
        require_uid(&predicate_uid, "c", "Assertion predicate")?;
        if let Some(value) = &object_uid {
            require_uid(value, "r", "Assertion object")?;
        }
        if let Some(value) = &unit_uid {
            require_uid(value, "c", "Assertion unit")?;
        }
        if !record_kinds.contains_key(subject_uid.as_str())
            || object_uid
                .as_ref()
                .is_some_and(|value| !record_kinds.contains_key(value.as_str()))
            || !concept_ids.contains(predicate_uid.as_str())
            || unit_uid
                .as_ref()
                .is_some_and(|value| !concept_ids.contains(value.as_str()))
        {
            return Err(protocol("Assertion has a missing dependency"));
        }
        if assertions
            .last()
            .is_some_and(|assertion: &AssertionMetadata| assertion.uid == uid)
        {
            return Err(protocol("duplicate Assertion identity in snapshot"));
        }
        let has_mantissa = row
            .try_get::<Option<&str>, _>("quantity_mantissa")?
            .is_some();
        let has_scale = row.try_get::<Option<i64>, _>("quantity_scale")?.is_some();
        if has_mantissa != has_scale {
            return Err(protocol("Assertion has an incomplete exact quantity"));
        }
        let quantity = has_mantissa.then(|| read_quantity(&row)).transpose()?;
        let role_value: &str = row.try_get("role")?;
        let role = assertion_role(role_value)?;
        if role == AssertionRole::Identity
            && (object_uid.is_some()
                || object_uri.is_some()
                || quantity.is_some()
                || unit_uid.is_some())
        {
            return Err(protocol("identity Assertion has an invalid stored shape"));
        }
        assertions.push(AssertionMetadata {
            uid,
            subject_uid,
            predicate_uid,
            object_uid,
            object_uri,
            role,
            quantity,
            unit_uid,
        });
    }
    Ok(assertions)
}

pub(super) async fn resolved_links_on(
    connection: &mut SqliteConnection,
    assertions: &[AssertionMetadata],
    record_kinds: &RecordKinds<'_>,
) -> Result<BTreeMap<String, String>, StoreError> {
    let assertion_ids: BTreeSet<_> = assertions.iter().map(|row| row.uid.as_str()).collect();
    let resolved_links: BTreeMap<String,String> = query_as::<_,(String,String)>("SELECT uid,target_uid FROM record_assertion_link WHERE object_uri IS NOT NULL AND target_uid IS NOT NULL AND retracted_at IS NULL")
        .fetch_all(&mut *connection).await?.into_iter().collect();
    for (assertion, target) in &resolved_links {
        require_uid(assertion, "a", "Resolved assertion")?;
        require_uid(target, "r", "Resolved assertion target")?;
        if !assertion_ids.contains(assertion.as_str())
            || !record_kinds.contains_key(target.as_str())
        {
            return Err(protocol("Invalid resolved assertion target"));
        }
    }
    Ok(resolved_links)
}
