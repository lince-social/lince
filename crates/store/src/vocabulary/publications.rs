//! Publication packages, ownership checks, and adoption records.

use std::collections::{HashMap, HashSet};

use nucleus::{
    execution::now,
    json::encoded_len,
    new_uid, valid_uid,
    vocabulary::{DEFAULT_VOCABULARY_UID, Publication},
};
use serde_json::{Map, Value, from_str, to_string};
use sqlx::{Row, SqliteConnection, SqlitePool, query, query_as, query_scalar};

use super::{apply_dictionary_on, dictionary_on, invalid, term_text, validate_dictionary};
use crate::{StoreError, linguas::LOCAL_UID, organs::LOCAL_ORGAN_SLUG};

/// Maximum size of a serialized vocabulary publication.
pub const PACKAGE_LIMIT: usize = 4 * 1024 * 1024;
const MAX_CONCEPTS: usize = 256;

/// Checks whether any saved publication retains the concept, including withdrawn packages.
///
/// # Errors
/// Returns an error if the publication index cannot be queried.
pub async fn retained_by_publication_on(
    connection: &mut SqliteConnection,
    concept: &str,
) -> Result<bool, StoreError> {
    query_scalar("SELECT EXISTS(SELECT 1 FROM vocabulary_publication p,json_each(p.package,'$.dictionaries') d WHERE json_extract(d.value,'$.source.concept_uid')=?)")
        .bind(concept).fetch_one(connection).await
}

/// Reads a bounded and validated vocabulary publication package.
///
/// # Errors
/// Returns an error for oversized, invalid, or unsupported packages.
pub fn read_package(raw: &str) -> Result<Publication, StoreError> {
    if raw.len() > PACKAGE_LIMIT {
        return Err(invalid("A Vocabulary package can contain at most 4 MiB"));
    }
    let package: Publication =
        from_str(raw).map_err(|error| invalid(&format!("Invalid Vocabulary package: {error}")))?;
    validate_package(&package)?;
    Ok(package)
}

/// Validates package identities, format, and distinct concept definition histories.
///
/// # Errors
/// Returns an error if the package or any dictionary violates those requirements.
pub fn validate_package(package: &Publication) -> Result<(), StoreError> {
    if package.format != 1
        || !valid_uid(&package.uid, "vp")
        || (!valid_uid(&package.vocabulary_uid, "g") && package.vocabulary_uid != LOCAL_UID)
        || !valid_uid(&package.owner_organ, "r")
        || package.revision < 1
        || package.dictionaries.len() > MAX_CONCEPTS
    {
        return Err(invalid("Invalid Vocabulary publication"));
    }
    term_text(&package.name)?;
    let mut concepts = HashSet::with_capacity(package.dictionaries.len());
    for dictionary in &package.dictionaries {
        validate_dictionary(dictionary)?;
        if dictionary.definitions.is_empty() || !concepts.insert(&dictionary.source.concept_uid) {
            return Err(invalid(
                "A publication needs a definition history for each distinct Concept",
            ));
        }
    }
    Ok(())
}

/// Requires a vocabulary whose owner is managed by this Cell, or a local unowned vocabulary.
///
/// # Errors
/// Returns an error for an unavailable or remotely owned vocabulary, or a failed read.
pub async fn require_local_vocabulary_on(
    connection: &mut SqliteConnection,
    uid: &str,
) -> Result<(), StoreError> {
    let owner: Option<Option<String>> = query_scalar("SELECT owner_organ FROM lingua WHERE uid=?")
        .bind(uid)
        .fetch_optional(&mut *connection)
        .await?;
    let owner = owner.ok_or_else(|| invalid("Vocabulary unavailable"))?;
    if let Some(owner) = owner {
        let local: bool=query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid=? AND kind='organ' AND (slug=?2 OR organ_uid=(SELECT uid FROM record WHERE slug=?2)))")
            .bind(owner).bind(LOCAL_ORGAN_SLUG).fetch_one(connection).await?;
        if !local {
            return Err(invalid("Change a Vocabulary at its owning Organ"));
        }
    }
    Ok(())
}

/// Requires a live Organ managed by this Cell.
///
/// # Errors
/// Returns an error if the Organ is missing, remote, deleted, or unreadable.
pub async fn require_local_organ_on(
    connection: &mut SqliteConnection,
    uid: &str,
) -> Result<(), StoreError> {
    let permitted: bool=query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid=? AND kind='organ' AND deleted_at IS NULL AND (slug=?2 OR organ_uid=(SELECT uid FROM record WHERE slug=?2)))")
        .bind(uid).bind(LOCAL_ORGAN_SLUG).fetch_one(connection).await?;
    if !permitted {
        return Err(invalid("Choose an Organ managed by this Cell"));
    }
    Ok(())
}

/// Creates a private named vocabulary owned and adopted by a local Organ.
///
/// # Errors
/// Returns an error for an invalid owner or name, or a failed write.
pub async fn create_vocabulary_on(
    connection: &mut SqliteConnection,
    name: &str,
    organ: &str,
) -> Result<String, StoreError> {
    require_local_organ_on(connection, organ).await?;
    let name = term_text(name)?;
    let uid = new_uid("g");
    let now = now().to_rfc3339();
    query(
        "INSERT INTO lingua(uid,name,owner_organ,visibility,created_at) VALUES(?,?,?,'private',?)",
    )
    .bind(&uid)
    .bind(name)
    .bind(organ)
    .bind(&now)
    .execute(&mut *connection)
    .await?;
    query("INSERT INTO organ_vocabulary(organ_uid,vocabulary_uid,adopted_at) VALUES(?,?,?)")
        .bind(organ)
        .bind(&uid)
        .bind(now)
        .execute(connection)
        .await?;
    Ok(uid)
}

/// Publishes the requested vocabulary revision and withdraws its preceding publication.
///
/// # Errors
/// Returns an error for an invalid owner, stale revision, oversized package, or failed write.
pub async fn publish_on(
    connection: &mut SqliteConnection,
    vocabulary: &str,
    expected_revision: i64,
) -> Result<Publication, StoreError> {
    if reserved_vocabulary(vocabulary) {
        return Err(invalid(
            "Create a named Vocabulary to publish your collection; the private Local home and built-in Lince Vocabulary have reserved identities",
        ));
    }
    require_local_vocabulary_on(connection, vocabulary).await?;
    let row = query("SELECT name,owner_organ,revision FROM lingua WHERE uid=?")
        .bind(vocabulary)
        .fetch_one(&mut *connection)
        .await?;
    let revision: i64 = row.try_get("revision")?;
    if revision != expected_revision {
        return Err(invalid(
            "The Vocabulary changed; inspect it again before publishing",
        ));
    }
    let owner: Option<String> = row.try_get("owner_organ")?;
    let owner = match owner {
        Some(owner) => owner,
        None => {
            query_scalar("SELECT uid FROM record WHERE slug=? AND kind='organ'")
                .bind(LOCAL_ORGAN_SLUG)
                .fetch_one(&mut *connection)
                .await?
        }
    };
    let concepts: Vec<String> = query_scalar(
        "SELECT concept_uid FROM lingua_concept WHERE lingua_uid=? ORDER BY concept_uid LIMIT ?",
    )
    .bind(vocabulary)
    .bind(i64::try_from(MAX_CONCEPTS + 1).map_err(|error| invalid(&error.to_string()))?)
    .fetch_all(&mut *connection)
    .await?;
    if concepts.len() > MAX_CONCEPTS {
        return Err(invalid("Publish at most 256 Concepts in one Vocabulary"));
    }
    let mut dictionaries = Vec::with_capacity(concepts.len());
    let mut bytes = 0usize;
    for concept in concepts {
        let dictionary = dictionary_on(connection, &concept).await?;
        bytes = bytes
            .saturating_add(encoded_len(&dictionary).map_err(|error| invalid(&error.to_string()))?);
        if bytes > PACKAGE_LIMIT {
            return Err(invalid(
                "The publication exceeds 4 MiB; use a smaller collection",
            ));
        }
        dictionaries.push(dictionary);
    }
    let package = Publication {
        format: 1,
        uid: new_uid("vp"),
        vocabulary_uid: vocabulary.into(),
        name: row.try_get("name")?,
        owner_organ: owner,
        revision,
        dictionaries,
    };
    validate_package(&package)?;
    let raw = encode_package(
        &package,
        "The publication exceeds 4 MiB; use a smaller collection",
    )?;
    replace_publication(connection, &package, &raw).await?;
    query("UPDATE lingua SET visibility='public' WHERE uid=?")
        .bind(vocabulary)
        .execute(connection)
        .await?;
    Ok(package)
}

fn reserved_vocabulary(uid: &str) -> bool {
    matches!(uid, LOCAL_UID | DEFAULT_VOCABULARY_UID)
}

fn encode_package(package: &Publication, size_error: &str) -> Result<String, StoreError> {
    let raw = to_string(package).map_err(|error| invalid(&error.to_string()))?;
    if raw.len() > PACKAGE_LIMIT {
        return Err(invalid(size_error));
    }
    Ok(raw)
}

async fn replace_publication(
    connection: &mut SqliteConnection,
    package: &Publication,
    raw: &str,
) -> Result<(), StoreError> {
    query("UPDATE vocabulary_publication SET withdrawn=1 WHERE vocabulary_uid=? AND withdrawn=0")
        .bind(&package.vocabulary_uid)
        .execute(&mut *connection)
        .await?;
    query(
        "INSERT INTO vocabulary_publication(uid,vocabulary_uid,revision,package) VALUES(?,?,?,?)",
    )
    .bind(&package.uid)
    .bind(&package.vocabulary_uid)
    .bind(package.revision)
    .bind(raw)
    .execute(connection)
    .await?;
    Ok(())
}

/// Reads a saved package, requiring it to remain active when `active` is true.
///
/// # Errors
/// Returns an error for an unavailable or invalid publication, or a failed read.
pub async fn publication_on(
    connection: &mut SqliteConnection,
    uid: &str,
    active: bool,
) -> Result<Publication, StoreError> {
    let raw: String = query_scalar(
        "SELECT package FROM vocabulary_publication WHERE uid=? AND (?=0 OR withdrawn=0)",
    )
    .bind(uid)
    .bind(active)
    .fetch_optional(connection)
    .await?
    .ok_or_else(|| invalid("Publication unavailable or withdrawn"))?;
    read_package(&raw)
}

/// Withdraws a locally owned publication while retaining its snapshot and prior adoptions.
///
/// # Errors
/// Returns an error if the package is invalid, remotely owned, unavailable, or cannot be updated.
pub async fn withdraw_on(connection: &mut SqliteConnection, uid: &str) -> Result<(), StoreError> {
    let package = publication_on(connection, uid, false).await?;
    require_local_vocabulary_on(connection, &package.vocabulary_uid).await?;
    query("UPDATE vocabulary_publication SET withdrawn=1 WHERE uid=?")
        .bind(uid)
        .execute(&mut *connection)
        .await?;
    query("UPDATE lingua SET visibility='private' WHERE uid=? AND NOT EXISTS(SELECT 1 FROM vocabulary_publication WHERE vocabulary_uid=lingua.uid AND withdrawn=0)")
        .bind(package.vocabulary_uid)
        .execute(connection)
        .await?;
    Ok(())
}

/// Adopts a validated publication and makes its retained meanings available to the local Organ.
///
/// # Errors
/// Returns an error for invalid ownership, conflicting meanings, forbidden withdrawals, or failed storage.
pub async fn adopt_on(
    connection: &mut SqliteConnection,
    package: &Publication,
    organ: &str,
) -> Result<(), StoreError> {
    validate_package(package)?;
    require_local_organ_on(connection, organ).await?;
    let local: String = query_scalar("SELECT uid FROM record WHERE slug=? AND kind='organ'")
        .bind(LOCAL_ORGAN_SLUG)
        .fetch_one(&mut *connection)
        .await?;
    let existing: Option<(String, bool)> =
        query_as("SELECT package,withdrawn FROM vocabulary_publication WHERE uid=?")
            .bind(&package.uid)
            .fetch_optional(&mut *connection)
            .await?;
    let raw = encode_package(package, "The publication exceeds 4 MiB")?;
    if let Some((existing, withdrawn)) = existing {
        if existing != raw {
            return Err(invalid(
                "The publication identity already has a different snapshot",
            ));
        }
        if withdrawn {
            let saved: bool=query_scalar("SELECT EXISTS(SELECT 1 FROM organ_vocabulary WHERE organ_uid=? AND publication_uid=?)")
                .bind(organ).bind(&package.uid).fetch_one(connection).await?;
            if saved {
                return Ok(());
            }
            return Err(invalid(
                "This publication was withdrawn; already adopted meanings remain usable",
            ));
        }
    } else {
        if reserved_vocabulary(&package.vocabulary_uid) || package.owner_organ == local {
            return Err(invalid(
                "An imported package cannot claim this Cell's Vocabulary ownership",
            ));
        }
        let owner: Option<Option<String>> =
            query_scalar("SELECT owner_organ FROM lingua WHERE uid=?")
                .bind(&package.vocabulary_uid)
                .fetch_optional(&mut *connection)
                .await?;
        if owner
            .as_ref()
            .is_some_and(|owner| owner.as_deref() != Some(&package.owner_organ))
        {
            return Err(invalid("The Vocabulary identity has a different owner"));
        }
        query("INSERT OR IGNORE INTO lingua(uid,name,owner_organ,visibility,created_at) VALUES(?,?,?,'shared',?)")
            .bind(&package.vocabulary_uid).bind(&package.name).bind(&package.owner_organ).bind(now().to_rfc3339()).execute(&mut *connection).await?;
        replace_publication(connection, package, &raw).await?;
    }
    for dictionary in &package.dictionaries {
        let known: bool =
            query_scalar("SELECT EXISTS(SELECT 1 FROM concept_source WHERE concept_uid=?)")
                .bind(&dictionary.source.concept_uid)
                .fetch_one(&mut *connection)
                .await?;
        if known {
            let saved = dictionary_on(connection, &dictionary.source.concept_uid).await?;
            if saved.source != dictionary.source
                || dictionary.definitions.iter().any(|version| {
                    !saved.definitions.iter().any(|saved| {
                        saved.revision == version.revision && saved.definition == version.definition
                    })
                })
            {
                return Err(invalid(
                    "The package changes a saved meaning; review its source update separately",
                ));
            }
        } else {
            if reserved_vocabulary(&dictionary.source.vocabulary_uid) {
                return Err(invalid(
                    "An imported meaning cannot claim a built-in Vocabulary",
                ));
            }
            apply_dictionary_on(connection, dictionary, &package.owner_organ, false).await?;
        }
        query("INSERT INTO lingua_concept(lingua_uid,concept_uid,adopted_at) VALUES(?,?,?) ON CONFLICT DO NOTHING")
            .bind(&package.vocabulary_uid).bind(&dictionary.source.concept_uid).bind(now().to_rfc3339()).execute(&mut *connection).await?;
    }
    query("INSERT INTO organ_vocabulary(organ_uid,vocabulary_uid,publication_uid,adopted_at) VALUES(?,?,?,?) ON CONFLICT(organ_uid,vocabulary_uid) DO UPDATE SET publication_uid=excluded.publication_uid,adopted_at=excluded.adopted_at")
        .bind(organ).bind(&package.vocabulary_uid).bind(&package.uid).bind(now().to_rfc3339()).execute(connection).await?;
    Ok(())
}

/// Requires a concept whose source vocabulary and originating Organ are managed by this Cell.
///
/// # Errors
/// Returns an error if the concept source is absent, remote, or unreadable.
pub async fn require_local_concept_on(
    connection: &mut SqliteConnection,
    uid: &str,
) -> Result<(), StoreError> {
    let row=query("SELECT c.origin_organ,s.lingua_uid FROM concept c JOIN concept_source s ON s.concept_uid=c.uid WHERE c.uid=?")
        .bind(uid).fetch_optional(&mut *connection).await?.ok_or_else(|| invalid("Concept source unavailable"))?;
    if let Some(origin) = row.try_get::<Option<&str>, _>("origin_organ")? {
        require_local_organ_on(connection, origin).await?;
    }
    require_local_vocabulary_on(connection, row.try_get("lingua_uid")?).await
}

#[derive(Default)]
struct CollectionSummary {
    publications: Vec<Value>,
    organs: Vec<Value>,
}

/// Lists the publication and Organ summaries for each vocabulary that has them.
///
/// # Errors
/// Returns an error if a summary row cannot be read or decoded.
pub async fn collection_details(pool: &SqlitePool) -> Result<HashMap<String, Value>, StoreError> {
    let mut details = HashMap::<String, CollectionSummary>::new();
    let publications: Vec<(String, String, i64, bool)> = query_as(
        "SELECT uid,vocabulary_uid,revision,withdrawn FROM vocabulary_publication ORDER BY rowid",
    )
    .fetch_all(pool)
    .await?;
    for (uid, vocabulary, revision, withdrawn) in publications {
        details
            .entry(vocabulary)
            .or_default()
            .publications
            .push(Value::Object(Map::from_iter([
                ("uid".into(), uid.into()),
                ("revision".into(), revision.into()),
                ("withdrawn".into(), withdrawn.into()),
            ])));
    }
    let organs: Vec<(String, String, Option<String>)> =
        query_as("SELECT vocabulary_uid,organ_uid,publication_uid FROM organ_vocabulary")
            .fetch_all(pool)
            .await?;
    for (vocabulary, uid, publication) in organs {
        details
            .entry(vocabulary)
            .or_default()
            .organs
            .push(Value::Object(Map::from_iter([
                ("uid".into(), uid.into()),
                ("publication".into(), publication.into()),
            ])));
    }
    Ok(details
        .into_iter()
        .map(|(uid, summary)| {
            let value = Value::Object(Map::from_iter([
                ("publications".into(), Value::Array(summary.publications)),
                ("organs".into(), Value::Array(summary.organs)),
            ]));
            (uid, value)
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{collection_details, create_vocabulary_on, publish_on, withdraw_on};
    use crate::{Store, organs::local, write_tx};

    #[tokio::test]
    async fn summaries_keep_publication_order_and_null_organ_adoption() {
        let store = Store::open_memory().await.unwrap();
        let organ = local(&store.pool).await.unwrap().unwrap().uid;
        let mut tx = write_tx(&store.pool).await.unwrap();
        let vocabulary = create_vocabulary_on(&mut tx, "Published collection", &organ)
            .await
            .unwrap();
        let first = publish_on(&mut tx, &vocabulary, 1).await.unwrap();
        let second = publish_on(&mut tx, &vocabulary, 2).await.unwrap();
        withdraw_on(&mut tx, &second.uid).await.unwrap();
        tx.commit().await.unwrap();

        let details = collection_details(&store.pool).await.unwrap();
        assert_eq!(
            details.get(&vocabulary).unwrap(),
            &json!({
                "publications": [
                    {"uid": first.uid, "revision": 1, "withdrawn": true},
                    {"uid": second.uid, "revision": 2, "withdrawn": true}
                ],
                "organs": [{"uid": organ, "publication": null}],
            })
        );
    }
}
