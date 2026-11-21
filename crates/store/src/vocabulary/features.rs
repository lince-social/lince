//! Built-in feature bindings and the assertion shapes they accept.

use std::collections::HashSet;

use nucleus::vocabulary::{FeatureBinding, parse_meaning_set};
use serde_json::{Value, json, to_string};
use sqlx::{SqliteConnection, SqlitePool, query, query_as, query_scalar};

use super::{invalid, resolve};
use crate::{
    StoreError,
    assertions::{AssertionRole, NewAssertion},
    concepts::descendants_including,
};

const MAX_FEATURE_MEANINGS: usize = 128;

/// Built-in feature names in display order.
pub const FEATURES: [&str; 21] = [
    "assignment",
    "containment",
    "thread",
    "message",
    "reply",
    "reference",
    "call",
    "group",
    "membership",
    "operator",
    "ancestry",
    "agent",
    "actor",
    "person",
    "organ",
    "location",
    "participant",
    "tagging",
    "call_session",
    "recording",
    "transcript",
];

/// Reads one feature's revision, accepted concepts, and write concept.
///
/// # Errors
/// Returns an error for an unknown feature or failed binding read.
pub async fn feature_binding_on(
    connection: &mut SqliteConnection,
    feature: &str,
) -> Result<FeatureBinding, StoreError> {
    let (role, write_concept_uid, revision): (String, String, i64) = query_as("SELECT r.uid,r.write_concept_uid,r.revision FROM vocabulary_feature f JOIN vocabulary_role r ON r.uid=f.role_uid WHERE f.feature=?")
        .bind(feature).fetch_optional(&mut *connection).await?.ok_or_else(|| invalid("Unknown built-in feature"))?;
    let concept_uids = query_scalar(
        "SELECT concept_uid FROM vocabulary_role_concept WHERE role_uid=? ORDER BY concept_uid",
    )
    .bind(role)
    .fetch_all(connection)
    .await?;
    Ok(FeatureBinding {
        feature: feature.into(),
        write_concept_uid,
        concept_uids,
        revision,
    })
}

/// Reads one feature binding from the pool.
///
/// # Errors
/// Returns an error for an unknown feature or failed binding read.
pub async fn feature_binding(
    pool: &SqlitePool,
    feature: &str,
) -> Result<FeatureBinding, StoreError> {
    let mut connection = pool.acquire().await?;
    feature_binding_on(&mut connection, feature).await
}

/// Reads all built-in bindings in feature order.
///
/// # Errors
/// Returns an error if any binding is missing or unreadable.
pub async fn feature_bindings(pool: &SqlitePool) -> Result<Vec<FeatureBinding>, StoreError> {
    let mut connection = pool.acquire().await?;
    let mut bindings = Vec::with_capacity(FEATURES.len());
    for feature in FEATURES {
        bindings.push(feature_binding_on(&mut connection, feature).await?);
    }
    Ok(bindings)
}

/// Returns the current write concept for a built-in feature.
///
/// # Errors
/// Returns an error for an unknown feature or failed binding read.
pub async fn feature_write(pool: &SqlitePool, feature: &str) -> Result<String, StoreError> {
    Ok(feature_binding(pool, feature).await?.write_concept_uid)
}

/// Resolves a feature to its write concept.
///
/// # Errors
/// Returns an error for an unknown feature or failed binding read.
pub async fn feature_resolve(
    pool: &SqlitePool,
    feature: &str,
) -> Result<Option<String>, StoreError> {
    feature_write(pool, feature).await.map(Some)
}

/// Validates a feature and returns its explicit read selector.
///
/// # Errors
/// Returns an error for an unknown feature or failed binding read.
pub async fn feature_read(pool: &SqlitePool, feature: &str) -> Result<Option<String>, StoreError> {
    feature_binding(pool, feature).await?;
    Ok(Some(format!("feature:{feature}")))
}

/// Returns the feature's write concept on the caller's connection.
///
/// # Errors
/// Returns an error for an unknown feature or failed binding read.
pub async fn feature_write_on(
    connection: &mut SqliteConnection,
    feature: &str,
) -> Result<String, StoreError> {
    Ok(feature_binding_on(connection, feature)
        .await?
        .write_concept_uid)
}

/// Checks whether the concept is accepted by the feature.
///
/// # Errors
/// Returns an error if the membership cannot be read.
pub async fn is_feature_on(
    connection: &mut SqliteConnection,
    feature: &str,
    concept: &str,
) -> Result<bool, StoreError> {
    query_scalar("SELECT EXISTS(SELECT 1 FROM vocabulary_feature f JOIN vocabulary_role_concept c ON c.role_uid=f.role_uid WHERE f.feature=? AND c.concept_uid=?)")
        .bind(feature).bind(concept).fetch_one(connection).await
}

/// Validates the assertion against feature shapes that accept its predicate.
///
/// # Errors
/// Returns an error for an incompatible assertion or failed feature lookup.
pub async fn validate_feature_assertion_on(
    connection: &mut SqliteConnection,
    assertion: &NewAssertion<'_>,
) -> Result<(), StoreError> {
    let features:Vec<String>=query_scalar("SELECT f.feature FROM vocabulary_feature f JOIN vocabulary_role_concept c ON c.role_uid=f.role_uid WHERE c.concept_uid=?").bind(assertion.predicate_uid).fetch_all(&mut *connection).await?;
    for feature in features {
        if matches!(feature.as_str(), "agent" | "actor" | "person" | "organ") {
            if assertion.role != AssertionRole::Identity || assertion.object_uid.is_some() {
                return Err(invalid(
                    "This feature meaning requires an identity Assertion",
                ));
            }
        } else if feature != "location" {
            if assertion.role != AssertionRole::Ordinary
                || assertion.object_uid.is_none()
                || assertion.object_uid == Some(assertion.subject_uid)
                || assertion.unit_uid.is_some()
                || (feature != "containment" && assertion.quantity.is_some())
            {
                return Err(invalid(
                    "This feature meaning requires an ordinary relationship with a distinct target",
                ));
            }
            if feature == "assignment" {
                let person:bool=query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid=? AND kind='person' AND deleted_at IS NULL)").bind(assertion.object_uid).fetch_one(&mut *connection).await?;
                if !person {
                    return Err(invalid("Assign an existing Person"));
                }
            }
        }
    }
    Ok(())
}

/// Expands an explicit meaning set, feature selector, or concept hierarchy.
///
/// # Errors
/// Returns an error for invalid or unavailable selections, or failed reads.
pub async fn meaning_family(
    pool: &SqlitePool,
    token: &str,
    person: Option<&str>,
) -> Result<Vec<String>, StoreError> {
    if let Some(concepts) = parse_meaning_set(token).map_err(invalid)? {
        let selection = to_string(&concepts).map_err(|error| invalid(&error.to_string()))?;
        let all_present: bool = query_scalar(
            "SELECT NOT EXISTS(SELECT 1 FROM json_each(?) AS selected
             WHERE NOT EXISTS(SELECT 1 FROM concept WHERE uid = selected.value))",
        )
        .bind(selection)
        .fetch_one(pool)
        .await?;
        if !all_present {
            return Err(invalid("A selected meaning is unavailable"));
        }
        return Ok(concepts);
    }
    if let Some(feature) = token.strip_prefix("feature:") {
        return Ok(feature_binding(pool, feature).await?.concept_uids);
    }
    match resolve(pool, token, person).await? {
        Some(concept) => descendants_including(pool, &concept).await,
        None => Ok(Vec::new()),
    }
}

/// Previews a concept addition or removal and reports incompatible active assertions.
///
/// # Errors
/// Returns an error for an unknown definition, invalid write selection, or failed reads.
pub async fn feature_preview_on(
    connection: &mut SqliteConnection,
    feature: &str,
    concept: &str,
    enabled: bool,
    write: bool,
) -> Result<Value, StoreError> {
    let before = feature_binding_on(connection, feature).await?;
    let (uses, incompatible) =
        preview_counts_on(connection, &before, concept, enabled, write).await?;
    let mut after = before.clone();
    after.concept_uids.retain(|uid| uid != concept);
    if enabled {
        after.concept_uids.push(concept.into());
    }
    after.concept_uids.sort();
    if write {
        after.write_concept_uid = concept.into();
    }
    Ok(json!({
        "concept_uid": concept, "enabled": enabled, "write": write, "before": before,
        "after": after, "active_assertions": uses, "incompatible_assertions": incompatible,
        "can_apply": !enabled || incompatible == 0,
    }))
}

async fn preview_counts_on(
    connection: &mut SqliteConnection,
    before: &FeatureBinding,
    concept: &str,
    enabled: bool,
    write: bool,
) -> Result<(i64, i64), StoreError> {
    let exists: bool =
        query_scalar("SELECT EXISTS(SELECT 1 FROM concept_source WHERE concept_uid=?)")
            .bind(concept)
            .fetch_one(&mut *connection)
            .await?;
    if !exists {
        return Err(invalid("Choose a Concept with a saved definition"));
    }
    if !enabled && (write || before.write_concept_uid == concept) {
        return Err(invalid(
            "Choose another write Concept before removing this meaning",
        ));
    }
    let accepted = before
        .concept_uids
        .iter()
        .filter(|uid| uid.as_str() != concept)
        .count()
        + usize::from(enabled);
    if accepted > MAX_FEATURE_MEANINGS {
        return Err(invalid("Accept at most 128 meanings per feature"));
    }
    query_as(
        "SELECT COUNT(*), COALESCE(SUM(CASE
             WHEN ?2 IN ('agent','actor','person','organ') THEN
                 role!='identity' OR object_uid IS NOT NULL
             WHEN ?2 = 'assignment' THEN
                 role!='ordinary' OR object_uid IS NULL OR subject_uid=object_uid
                 OR quantity_mantissa IS NOT NULL OR unit_uid IS NOT NULL
                 OR NOT EXISTS(SELECT 1 FROM record WHERE uid=record_assertion.object_uid
                               AND kind='person' AND deleted_at IS NULL)
             WHEN ?2 = 'containment' THEN
                 role!='ordinary' OR object_uid IS NULL OR subject_uid=object_uid OR unit_uid IS NOT NULL
             ELSE role!='ordinary' OR object_uid IS NULL OR subject_uid=object_uid
                 OR quantity_mantissa IS NOT NULL OR unit_uid IS NOT NULL
         END), 0) FROM record_assertion WHERE predicate_uid=?1 AND retracted_at IS NULL",
    )
    .bind(concept)
    .bind(&before.feature)
    .fetch_one(connection)
    .await
}

/// Replaces a feature binding in the caller's write transaction.
///
/// # Errors
/// Returns an error for stale bindings, incompatible assertions, invalid concepts, or failed writes.
pub async fn bind_feature_on(
    connection: &mut SqliteConnection,
    binding: &FeatureBinding,
    expected_revision: i64,
) -> Result<(), StoreError> {
    let previous = feature_binding_on(connection, &binding.feature).await?;
    if previous.revision != expected_revision {
        return Err(invalid(
            "Feature meanings changed; inspect them again before saving",
        ));
    }
    if binding.concept_uids.is_empty()
        || binding.concept_uids.len() > MAX_FEATURE_MEANINGS
        || !binding.concept_uids.contains(&binding.write_concept_uid)
    {
        return Err(invalid(
            "Choose up to 128 accepted meanings including the write Concept",
        ));
    }
    let mut unique = HashSet::with_capacity(binding.concept_uids.len());
    for concept in &binding.concept_uids {
        if !unique.insert(concept.as_str()) {
            return Err(invalid(
                "Each accepted meaning must have a distinct Concept identity",
            ));
        }
        let (_, incompatible) =
            preview_counts_on(connection, &previous, concept, true, false).await?;
        if incompatible != 0 {
            return Err(invalid(
                "Existing Assertions are incompatible with this feature",
            ));
        }
    }
    if previous.concept_uids == binding.concept_uids
        && previous.write_concept_uid == binding.write_concept_uid
    {
        return Ok(());
    }
    for concept in &binding.concept_uids {
        query("INSERT OR IGNORE INTO vocabulary_role_concept(role_uid,concept_uid) SELECT role_uid,? FROM vocabulary_feature WHERE feature=?")
            .bind(concept).bind(&binding.feature).execute(&mut *connection).await?;
    }
    query("UPDATE vocabulary_role SET write_concept_uid=?,revision=revision+1 WHERE uid=(SELECT role_uid FROM vocabulary_feature WHERE feature=?)")
        .bind(&binding.write_concept_uid).bind(&binding.feature).execute(&mut *connection).await?;
    for concept in previous
        .concept_uids
        .iter()
        .filter(|concept| !unique.contains(concept.as_str()))
    {
        query("DELETE FROM vocabulary_role_concept WHERE role_uid=(SELECT role_uid FROM vocabulary_feature WHERE feature=?) AND concept_uid=?")
            .bind(&binding.feature).bind(concept).execute(&mut *connection).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use nucleus::{RecordKind, new_uid, vocabulary::FeatureBinding};
    use serde_json::{from_value, json};

    use super::{bind_feature_on, feature_binding_on, feature_preview_on};
    use crate::{
        Store,
        assertions::{AssertionRole, NewAssertion, insert_tx, retract_tx},
        concepts::create,
        exact::zero,
        records::{NewRecord, create as create_record},
        write_tx,
    };

    #[tokio::test]
    async fn preview_and_binding_enforce_the_same_active_assertion_shapes() {
        let store = Store::open_memory().await.unwrap();
        let pool = &store.pool;
        let record = create_record(
            pool,
            NewRecord {
                slug: None,
                kind: RecordKind::Plain,
                head: "Feature subject",
                body: "",
                quantity: zero(),
            },
        )
        .await
        .unwrap();
        let concept = create(pool, "Candidate feature", &[]).await.unwrap();
        let assertion = new_uid("a");
        let mut tx = write_tx(pool).await.unwrap();
        insert_tx(
            &mut tx,
            &assertion,
            NewAssertion {
                subject_uid: &record.uid,
                predicate_uid: &concept,
                object_uid: None,
                role: AssertionRole::Identity,
                quantity: None,
                unit_uid: None,
                asserted_by: None,
            },
        )
        .await
        .unwrap();

        let reference = feature_preview_on(&mut tx, "reference", &concept, true, false)
            .await
            .unwrap();
        assert_eq!(reference.get("active_assertions"), Some(&json!(1)));
        assert_eq!(reference.get("incompatible_assertions"), Some(&json!(1)));
        assert_eq!(reference.get("can_apply"), Some(&json!(false)));
        let identity = feature_preview_on(&mut tx, "agent", &concept, true, false)
            .await
            .unwrap();
        assert_eq!(identity.get("can_apply"), Some(&json!(true)));

        let binding: FeatureBinding = from_value(reference.get("after").unwrap().clone()).unwrap();
        let error = bind_feature_on(&mut tx, &binding, binding.revision)
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Existing Assertions are incompatible")
        );
        assert!(retract_tx(&mut tx, &assertion, None).await.unwrap());
        bind_feature_on(&mut tx, &binding, binding.revision)
            .await
            .unwrap();
        let saved = feature_binding_on(&mut tx, "reference").await.unwrap();
        assert_eq!(saved.concept_uids, binding.concept_uids);
        assert_eq!(saved.write_concept_uid, binding.write_concept_uid);
    }
}
