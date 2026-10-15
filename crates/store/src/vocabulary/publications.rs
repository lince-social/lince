use super::*;
use nucleus::vocabulary::Publication;
use serde_json::{Value, json};

pub const PACKAGE_LIMIT: usize = 4 * 1024 * 1024;

pub async fn retained_by_publication_on(
    connection: &mut SqliteConnection,
    concept: &str,
) -> Result<bool, StoreError> {
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM vocabulary_publication p,json_each(p.package,'$.dictionaries') d WHERE json_extract(d.value,'$.source.concept_uid')=?)")
        .bind(concept).fetch_one(connection).await
}

pub fn read_package(raw: &str) -> Result<Publication, StoreError> {
    if raw.len() > PACKAGE_LIMIT {
        return Err(invalid("A Vocabulary package can contain at most 4 MiB"));
    }
    let package: Publication = serde_json::from_str(raw)
        .map_err(|error| invalid(&format!("Invalid Vocabulary package: {error}")))?;
    validate_package(&package)?;
    Ok(package)
}

pub fn validate_package(package: &Publication) -> Result<(), StoreError> {
    if package.format != 1
        || !nucleus::valid_uid(&package.uid, "vp")
        || (!nucleus::valid_uid(&package.vocabulary_uid, "g")
            && package.vocabulary_uid != crate::linguas::LOCAL_UID)
        || !nucleus::valid_uid(&package.owner_organ, "r")
        || package.revision < 1
        || package.dictionaries.len() > 256
    {
        return Err(invalid("Invalid Vocabulary publication"));
    }
    term_text(&package.name)?;
    let mut concepts = HashSet::new();
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

pub async fn require_local_vocabulary_on(
    connection: &mut SqliteConnection,
    uid: &str,
) -> Result<(), StoreError> {
    let owner: Option<Option<String>> =
        sqlx::query_scalar("SELECT owner_organ FROM lingua WHERE uid=?")
            .bind(uid)
            .fetch_optional(&mut *connection)
            .await?;
    let owner = owner.ok_or_else(|| invalid("Vocabulary unavailable"))?;
    if let Some(owner) = owner {
        let local: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid=? AND kind='organ' AND (slug=? OR organ_uid=(SELECT uid FROM record WHERE slug=?)))")
            .bind(owner).bind(crate::organs::LOCAL_ORGAN_SLUG).bind(crate::organs::LOCAL_ORGAN_SLUG).fetch_one(connection).await?;
        if !local {
            return Err(invalid("Change a Vocabulary at its owning Organ"));
        }
    }
    Ok(())
}

pub async fn require_local_organ_on(
    connection: &mut SqliteConnection,
    uid: &str,
) -> Result<(), StoreError> {
    let permitted: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM record WHERE uid=? AND kind='organ' AND deleted_at IS NULL AND (slug=? OR organ_uid=(SELECT uid FROM record WHERE slug=?)))")
        .bind(uid).bind(crate::organs::LOCAL_ORGAN_SLUG).bind(crate::organs::LOCAL_ORGAN_SLUG).fetch_one(connection).await?;
    if !permitted {
        return Err(invalid("Choose an Organ managed by this Cell"));
    }
    Ok(())
}

pub async fn create_vocabulary_on(
    connection: &mut SqliteConnection,
    name: &str,
    organ: &str,
) -> Result<String, StoreError> {
    require_local_organ_on(connection, organ).await?;
    let name = term_text(name)?;
    let uid = nucleus::new_uid("g");
    let now = nucleus::execution::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO lingua(uid,name,owner_organ,visibility,created_at) VALUES(?,?,?,'private',?)",
    )
    .bind(&uid)
    .bind(name)
    .bind(organ)
    .bind(&now)
    .execute(&mut *connection)
    .await?;
    sqlx::query("INSERT INTO organ_vocabulary(organ_uid,vocabulary_uid,adopted_at) VALUES(?,?,?)")
        .bind(organ)
        .bind(&uid)
        .bind(now)
        .execute(connection)
        .await?;
    Ok(uid)
}

pub async fn publish_on(
    connection: &mut SqliteConnection,
    vocabulary: &str,
    expected_revision: i64,
) -> Result<Publication, StoreError> {
    if vocabulary == crate::linguas::LOCAL_UID
        || vocabulary == nucleus::vocabulary::DEFAULT_VOCABULARY_UID
    {
        return Err(invalid(
            "Create a named Vocabulary to publish your collection; the private Local home and built-in Lince Vocabulary have reserved identities",
        ));
    }
    require_local_vocabulary_on(connection, vocabulary).await?;
    let row = sqlx::query("SELECT name,owner_organ,revision FROM lingua WHERE uid=?")
        .bind(vocabulary)
        .fetch_one(&mut *connection)
        .await?;
    let revision: i64 = row.get("revision");
    if revision != expected_revision {
        return Err(invalid(
            "The Vocabulary changed; inspect it again before publishing",
        ));
    }
    let owner: Option<String> = row.get("owner_organ");
    let owner = match owner {
        Some(owner) => owner,
        None => {
            sqlx::query_scalar("SELECT uid FROM record WHERE slug=? AND kind='organ'")
                .bind(crate::organs::LOCAL_ORGAN_SLUG)
                .fetch_one(&mut *connection)
                .await?
        }
    };
    let concepts: Vec<String> = sqlx::query_scalar(
        "SELECT concept_uid FROM lingua_concept WHERE lingua_uid=? ORDER BY concept_uid LIMIT 257",
    )
    .bind(vocabulary)
    .fetch_all(&mut *connection)
    .await?;
    if concepts.len() > 256 {
        return Err(invalid("Publish at most 256 Concepts in one Vocabulary"));
    }
    let mut dictionaries = Vec::new();
    let mut bytes = 0usize;
    for concept in concepts {
        let dictionary = dictionary_on(connection, &concept).await?;
        bytes = bytes.saturating_add(
            serde_json::to_vec(&dictionary)
                .map_err(|error| invalid(&error.to_string()))?
                .len(),
        );
        if bytes > PACKAGE_LIMIT {
            return Err(invalid(
                "The publication exceeds 4 MiB; use a smaller collection",
            ));
        }
        dictionaries.push(dictionary);
    }
    let package = Publication {
        format: 1,
        uid: nucleus::new_uid("vp"),
        vocabulary_uid: vocabulary.into(),
        name: row.get("name"),
        owner_organ: owner,
        revision,
        dictionaries,
    };
    validate_package(&package)?;
    let raw = serde_json::to_string(&package).map_err(|error| invalid(&error.to_string()))?;
    if raw.len() > PACKAGE_LIMIT {
        return Err(invalid(
            "The publication exceeds 4 MiB; use a smaller collection",
        ));
    }
    sqlx::query(
        "UPDATE vocabulary_publication SET withdrawn=1 WHERE vocabulary_uid=? AND withdrawn=0",
    )
    .bind(vocabulary)
    .execute(&mut *connection)
    .await?;
    sqlx::query(
        "INSERT INTO vocabulary_publication(uid,vocabulary_uid,revision,package) VALUES(?,?,?,?)",
    )
    .bind(&package.uid)
    .bind(vocabulary)
    .bind(revision)
    .bind(raw)
    .execute(&mut *connection)
    .await?;
    sqlx::query("UPDATE lingua SET visibility='public' WHERE uid=?")
        .bind(vocabulary)
        .execute(connection)
        .await?;
    Ok(package)
}

pub async fn publication_on(
    connection: &mut SqliteConnection,
    uid: &str,
    active: bool,
) -> Result<Publication, StoreError> {
    let raw: String = sqlx::query_scalar(
        "SELECT package FROM vocabulary_publication WHERE uid=? AND (?=0 OR withdrawn=0)",
    )
    .bind(uid)
    .bind(active)
    .fetch_optional(connection)
    .await?
    .ok_or_else(|| invalid("Publication unavailable or withdrawn"))?;
    read_package(&raw)
}

pub async fn withdraw_on(connection: &mut SqliteConnection, uid: &str) -> Result<(), StoreError> {
    let package = publication_on(connection, uid, false).await?;
    require_local_vocabulary_on(connection, &package.vocabulary_uid).await?;
    sqlx::query("UPDATE vocabulary_publication SET withdrawn=1 WHERE uid=?")
        .bind(uid)
        .execute(&mut *connection)
        .await?;
    sqlx::query("UPDATE lingua SET visibility='private' WHERE uid=? AND NOT EXISTS(SELECT 1 FROM vocabulary_publication WHERE vocabulary_uid=lingua.uid AND withdrawn=0)")
        .bind(package.vocabulary_uid)
        .execute(connection)
        .await?;
    Ok(())
}

pub async fn adopt_on(
    connection: &mut SqliteConnection,
    package: &Publication,
    organ: &str,
) -> Result<(), StoreError> {
    validate_package(package)?;
    require_local_organ_on(connection, organ).await?;
    let local: String = sqlx::query_scalar("SELECT uid FROM record WHERE slug=? AND kind='organ'")
        .bind(crate::organs::LOCAL_ORGAN_SLUG)
        .fetch_one(&mut *connection)
        .await?;
    let existing: Option<String> =
        sqlx::query_scalar("SELECT package FROM vocabulary_publication WHERE uid=?")
            .bind(&package.uid)
            .fetch_optional(&mut *connection)
            .await?;
    let raw = serde_json::to_string(package).map_err(|error| invalid(&error.to_string()))?;
    if raw.len() > PACKAGE_LIMIT {
        return Err(invalid("The publication exceeds 4 MiB"));
    }
    if let Some(existing) = existing {
        if existing != raw {
            return Err(invalid(
                "The publication identity already has a different snapshot",
            ));
        }
        let withdrawn: bool =
            sqlx::query_scalar("SELECT withdrawn FROM vocabulary_publication WHERE uid=?")
                .bind(&package.uid)
                .fetch_one(&mut *connection)
                .await?;
        if withdrawn {
            let saved: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM organ_vocabulary WHERE organ_uid=? AND publication_uid=?)")
                .bind(organ).bind(&package.uid).fetch_one(connection).await?;
            if saved {
                return Ok(());
            }
            return Err(invalid(
                "This publication was withdrawn; already adopted meanings remain usable",
            ));
        }
    } else {
        if package.vocabulary_uid == crate::linguas::LOCAL_UID
            || package.vocabulary_uid == nucleus::vocabulary::DEFAULT_VOCABULARY_UID
            || package.owner_organ == local
        {
            return Err(invalid(
                "An imported package cannot claim this Cell's Vocabulary ownership",
            ));
        }
        let owner: Option<Option<String>> =
            sqlx::query_scalar("SELECT owner_organ FROM lingua WHERE uid=?")
                .bind(&package.vocabulary_uid)
                .fetch_optional(&mut *connection)
                .await?;
        if owner
            .as_ref()
            .is_some_and(|owner| owner.as_deref() != Some(&package.owner_organ))
        {
            return Err(invalid("The Vocabulary identity has a different owner"));
        }
        sqlx::query("INSERT OR IGNORE INTO lingua(uid,name,owner_organ,visibility,created_at) VALUES(?,?,?,'shared',?)")
            .bind(&package.vocabulary_uid).bind(&package.name).bind(&package.owner_organ).bind(nucleus::execution::now().to_rfc3339()).execute(&mut *connection).await?;
        sqlx::query(
            "UPDATE vocabulary_publication SET withdrawn=1 WHERE vocabulary_uid=? AND withdrawn=0",
        )
        .bind(&package.vocabulary_uid)
        .execute(&mut *connection)
        .await?;
        sqlx::query("INSERT INTO vocabulary_publication(uid,vocabulary_uid,revision,package) VALUES(?,?,?,?)")
            .bind(&package.uid).bind(&package.vocabulary_uid).bind(package.revision).bind(raw).execute(&mut *connection).await?;
    }
    for dictionary in &package.dictionaries {
        let known: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM concept_source WHERE concept_uid=?)")
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
            if dictionary.source.vocabulary_uid == crate::linguas::LOCAL_UID
                || dictionary.source.vocabulary_uid == nucleus::vocabulary::DEFAULT_VOCABULARY_UID
            {
                return Err(invalid(
                    "An imported meaning cannot claim a built-in Vocabulary",
                ));
            }
            apply_dictionary_on(connection, dictionary, &package.owner_organ, false).await?;
        }
        sqlx::query("INSERT INTO lingua_concept(lingua_uid,concept_uid,adopted_at) VALUES(?,?,?) ON CONFLICT DO NOTHING")
            .bind(&package.vocabulary_uid).bind(&dictionary.source.concept_uid).bind(nucleus::execution::now().to_rfc3339()).execute(&mut *connection).await?;
    }
    sqlx::query("INSERT INTO organ_vocabulary(organ_uid,vocabulary_uid,publication_uid,adopted_at) VALUES(?,?,?,?) ON CONFLICT(organ_uid,vocabulary_uid) DO UPDATE SET publication_uid=excluded.publication_uid,adopted_at=excluded.adopted_at")
        .bind(organ).bind(&package.vocabulary_uid).bind(&package.uid).bind(nucleus::execution::now().to_rfc3339()).execute(connection).await?;
    Ok(())
}

pub async fn require_local_concept_on(
    connection: &mut SqliteConnection,
    uid: &str,
) -> Result<(), StoreError> {
    let row=sqlx::query("SELECT c.origin_organ,s.lingua_uid FROM concept c JOIN concept_source s ON s.concept_uid=c.uid WHERE c.uid=?")
        .bind(uid).fetch_optional(&mut *connection).await?.ok_or_else(|| invalid("Concept source unavailable"))?;
    if let Some(origin) = row.get::<Option<String>, _>("origin_organ") {
        require_local_organ_on(connection, &origin).await?;
    }
    require_local_vocabulary_on(connection, &row.get::<String, _>("lingua_uid")).await
}

pub async fn collection_details(pool: &SqlitePool) -> Result<HashMap<String, Value>, StoreError> {
    let mut details: HashMap<String, Value> = HashMap::new();
    for row in sqlx::query(
        "SELECT uid,vocabulary_uid,revision,withdrawn FROM vocabulary_publication ORDER BY rowid",
    )
    .fetch_all(pool)
    .await?
    {
        let entry = details
            .entry(row.get("vocabulary_uid"))
            .or_insert_with(|| json!({"publications":[],"organs":[]}));
        entry["publications"].as_array_mut().unwrap().push(json!({"uid":row.get::<String,_>("uid"),"revision":row.get::<i64,_>("revision"),"withdrawn":row.get::<bool,_>("withdrawn")}));
    }
    for row in sqlx::query("SELECT * FROM organ_vocabulary")
        .fetch_all(pool)
        .await?
    {
        let entry = details
            .entry(row.get("vocabulary_uid"))
            .or_insert_with(|| json!({"publications":[],"organs":[]}));
        entry["organs"].as_array_mut().unwrap().push(json!({"uid":row.get::<String,_>("organ_uid"),"publication":row.get::<Option<String>,_>("publication_uid")}));
    }
    Ok(details)
}
