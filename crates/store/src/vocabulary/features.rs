use super::{StoreError, invalid};
use nucleus::vocabulary::FeatureBinding;
use serde_json::{Value, json};
use sqlx::{SqliteConnection, SqlitePool};

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

pub async fn feature_binding_on(
    connection: &mut SqliteConnection,
    feature: &str,
) -> Result<FeatureBinding, StoreError> {
    let (role, write_concept_uid, revision): (String, String, i64) = sqlx::query_as("SELECT r.uid,r.write_concept_uid,r.revision FROM vocabulary_feature f JOIN vocabulary_role r ON r.uid=f.role_uid WHERE f.feature=?")
        .bind(feature).fetch_optional(&mut *connection).await?.ok_or_else(|| invalid("Unknown built-in feature"))?;
    let concept_uids = sqlx::query_scalar(
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

pub async fn feature_binding(
    pool: &SqlitePool,
    feature: &str,
) -> Result<FeatureBinding, StoreError> {
    let mut connection = pool.acquire().await?;
    feature_binding_on(&mut connection, feature).await
}

pub async fn feature_bindings(pool: &SqlitePool) -> Result<Vec<FeatureBinding>, StoreError> {
    let mut connection = pool.acquire().await?;
    let mut bindings = Vec::with_capacity(FEATURES.len());
    for feature in FEATURES {
        bindings.push(feature_binding_on(&mut connection, feature).await?);
    }
    Ok(bindings)
}

pub async fn feature_write(pool: &SqlitePool, feature: &str) -> Result<String, StoreError> {
    Ok(feature_binding(pool, feature).await?.write_concept_uid)
}

pub async fn feature_resolve(
    pool: &SqlitePool,
    feature: &str,
) -> Result<Option<String>, StoreError> {
    feature_write(pool, feature).await.map(Some)
}

pub async fn feature_read(pool: &SqlitePool, feature: &str) -> Result<Option<String>, StoreError> {
    feature_binding(pool, feature).await?;
    Ok(Some(format!("feature:{feature}")))
}

pub async fn feature_write_on(
    connection: &mut SqliteConnection,
    feature: &str,
) -> Result<String, StoreError> {
    Ok(feature_binding_on(connection, feature)
        .await?
        .write_concept_uid)
}

pub async fn is_feature_on(
    connection: &mut SqliteConnection,
    feature: &str,
    concept: &str,
) -> Result<bool, StoreError> {
    Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM vocabulary_feature f JOIN vocabulary_role_concept c ON c.role_uid=f.role_uid WHERE f.feature=? AND c.concept_uid=?)")
        .bind(feature).bind(concept).fetch_one(connection).await?)
}

pub async fn validate_feature_assertion_on(
    connection: &mut SqliteConnection,
    assertion: &crate::assertions::NewAssertion<'_>,
) -> Result<(), StoreError> {
    let features:Vec<String>=sqlx::query_scalar("SELECT f.feature FROM vocabulary_feature f JOIN vocabulary_role_concept c ON c.role_uid=f.role_uid WHERE c.concept_uid=?").bind(assertion.predicate_uid).fetch_all(&mut *connection).await?;
    for feature in features {
        if matches!(feature.as_str(), "agent" | "actor" | "person" | "organ") {
            if assertion.role != crate::assertions::AssertionRole::Identity
                || assertion.object_uid.is_some()
            {
                return Err(invalid(
                    "This feature meaning requires an identity Assertion",
                ));
            }
        } else if feature != "location" {
            if assertion.role != crate::assertions::AssertionRole::Ordinary
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
                let person:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid=? AND kind='person' AND deleted_at IS NULL)").bind(assertion.object_uid).fetch_one(&mut *connection).await?;
                if !person {
                    return Err(invalid("Assign an existing Person"));
                }
            }
        }
    }
    Ok(())
}

pub async fn meaning_family(
    pool: &SqlitePool,
    token: &str,
    person: Option<&str>,
) -> Result<Vec<String>, StoreError> {
    if let Some(concepts) = nucleus::vocabulary::parse_meaning_set(token).map_err(invalid)? {
        for concept in &concepts {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM concept WHERE uid=?)")
                    .bind(concept)
                    .fetch_one(pool)
                    .await?;
            if !exists {
                return Err(invalid("A selected meaning is unavailable"));
            }
        }
        return Ok(concepts);
    }
    if let Some(feature) = token.strip_prefix("feature:") {
        return Ok(feature_binding(pool, feature).await?.concept_uids);
    }
    match super::resolve(pool, token, person).await? {
        Some(concept) => crate::concepts::descendants_including(pool, &concept).await,
        None => Ok(Vec::new()),
    }
}

pub async fn feature_preview_on(
    connection: &mut SqliteConnection,
    feature: &str,
    concept: &str,
    enabled: bool,
    write: bool,
) -> Result<Value, StoreError> {
    let before = feature_binding_on(connection, feature).await?;
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM concept_source WHERE concept_uid=?)")
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
    let mut after = before.clone();
    after.concept_uids.retain(|uid| uid != concept);
    if enabled {
        after.concept_uids.push(concept.into());
    }
    after.concept_uids.sort();
    if after.concept_uids.len() > 128 {
        return Err(invalid("Accept at most 128 meanings per feature"));
    }
    if write {
        after.write_concept_uid = concept.into();
    }
    let shape = if matches!(feature, "agent" | "actor" | "person" | "organ") {
        "(role!='identity' OR object_uid IS NOT NULL)"
    } else if feature == "assignment" {
        "(role!='ordinary' OR object_uid IS NULL OR subject_uid=object_uid OR quantity_mantissa IS NOT NULL OR unit_uid IS NOT NULL OR NOT EXISTS(SELECT 1 FROM record WHERE uid=record_assertion.object_uid AND kind='person' AND deleted_at IS NULL))"
    } else if feature == "containment" {
        "(role!='ordinary' OR object_uid IS NULL OR subject_uid=object_uid OR unit_uid IS NOT NULL)"
    } else {
        "(role!='ordinary' OR object_uid IS NULL OR subject_uid=object_uid OR quantity_mantissa IS NOT NULL OR unit_uid IS NOT NULL)"
    };
    let (uses, incompatible): (i64, i64) = sqlx::query_as(&format!("SELECT COUNT(*),COALESCE(SUM({shape}),0) FROM record_assertion WHERE predicate_uid=? AND retracted_at IS NULL"))
        .bind(concept).fetch_one(connection).await?;
    Ok(
        json!({"concept_uid":concept,"enabled":enabled,"write":write,"before":before,"after":after,"active_assertions":uses,"incompatible_assertions":incompatible,"can_apply":!enabled || incompatible==0}),
    )
}

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
        || binding.concept_uids.len() > 128
        || !binding.concept_uids.contains(&binding.write_concept_uid)
    {
        return Err(invalid(
            "Choose up to 128 accepted meanings including the write Concept",
        ));
    }
    let mut unique = std::collections::HashSet::new();
    for concept in &binding.concept_uids {
        if !unique.insert(concept) {
            return Err(invalid(
                "Each accepted meaning must have a distinct Concept identity",
            ));
        }
        if feature_preview_on(connection, &binding.feature, concept, true, false).await?["can_apply"]
            != true
        {
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
        sqlx::query("INSERT OR IGNORE INTO vocabulary_role_concept(role_uid,concept_uid) SELECT role_uid,? FROM vocabulary_feature WHERE feature=?")
            .bind(concept).bind(&binding.feature).execute(&mut *connection).await?;
    }
    sqlx::query("UPDATE vocabulary_role SET write_concept_uid=?,revision=revision+1 WHERE uid=(SELECT role_uid FROM vocabulary_feature WHERE feature=?)")
        .bind(&binding.write_concept_uid).bind(&binding.feature).execute(&mut *connection).await?;
    for concept in previous
        .concept_uids
        .iter()
        .filter(|concept| !binding.concept_uids.contains(concept))
    {
        sqlx::query("DELETE FROM vocabulary_role_concept WHERE role_uid=(SELECT role_uid FROM vocabulary_feature WHERE feature=?) AND concept_uid=?")
            .bind(&binding.feature).bind(concept).execute(&mut *connection).await?;
    }
    Ok(())
}
