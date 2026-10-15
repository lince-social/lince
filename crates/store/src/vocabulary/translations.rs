use super::{StoreError, invalid};
use nucleus::vocabulary::TranslationPackage;
use serde_json::{Value, json};
use sqlx::SqliteConnection;

pub fn read_translations(raw: &str) -> Result<TranslationPackage, StoreError> {
    if raw.len() > 4 * 1024 * 1024 {
        return Err(invalid("A translation package can contain at most 4 MiB"));
    }
    let package: TranslationPackage = serde_json::from_str(raw)
        .map_err(|error| invalid(&format!("Invalid translation package: {error}")))?;
    if package.format != 1
        || (!nucleus::valid_uid(&package.vocabulary_uid, "g")
            && package.vocabulary_uid != crate::linguas::LOCAL_UID)
        || package.dictionaries.len() > 256
    {
        return Err(invalid(
            "Choose a supported translation package with at most 256 meanings",
        ));
    }
    let mut concepts = std::collections::HashSet::new();
    for dictionary in &package.dictionaries {
        super::validate_dictionary(dictionary)?;
        if dictionary.definitions.is_empty() || !concepts.insert(&dictionary.source.concept_uid) {
            return Err(invalid(
                "Include a definition history for each distinct translated meaning",
            ));
        }
    }
    Ok(package)
}

pub async fn export_translations_on(
    connection: &mut SqliteConnection,
    vocabulary: &str,
) -> Result<TranslationPackage, StoreError> {
    let concepts: Vec<String> = sqlx::query_scalar(
        "SELECT concept_uid FROM lingua_concept WHERE lingua_uid=? ORDER BY concept_uid LIMIT 257",
    )
    .bind(vocabulary)
    .fetch_all(&mut *connection)
    .await?;
    if concepts.len() > 256 {
        return Err(invalid(
            "Share at most 256 translated meanings in one package",
        ));
    }
    let mut package = TranslationPackage {
        format: 1,
        vocabulary_uid: vocabulary.into(),
        dictionaries: Vec::new(),
    };
    let mut size = 0;
    for concept in concepts {
        let dictionary = super::dictionary_on(connection, &concept).await?;
        size += serde_json::to_vec(&dictionary)
            .map_err(|error| invalid(&error.to_string()))?
            .len();
        if size > 4 * 1024 * 1024 {
            return Err(invalid("A translation package can contain at most 4 MiB"));
        }
        package.dictionaries.push(dictionary);
    }
    Ok(package)
}

pub async fn preview_translations_on(
    connection: &mut SqliteConnection,
    package: &TranslationPackage,
) -> Result<Value, StoreError> {
    let mut entries = Vec::new();
    let mut missing = Vec::new();
    for incoming in &package.dictionaries {
        let concept = &incoming.source.concept_uid;
        let exists: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM concept_source WHERE concept_uid=?)")
                .bind(concept)
                .fetch_one(&mut *connection)
                .await?;
        if !exists {
            missing.push(concept.clone());
            continue;
        }
        let saved = super::dictionary_on(connection, concept).await?;
        if saved.source != incoming.source
            || incoming.definitions.iter().any(|version| {
                !saved.definitions.iter().any(|known| {
                    known.concept_uid == version.concept_uid
                        && known.revision == version.revision
                        && known.definition == version.definition
                })
            })
        {
            return Err(invalid(
                "A translation package cannot change saved meaning definitions; inspect and adopt its Vocabulary separately",
            ));
        }
        let mut new = 0;
        for term in incoming.terms.iter().filter(|term| !term.retired) {
            if !saved.terms.iter().any(|known| {
                !known.retired
                    && known.name == term.name
                    && known.language.eq_ignore_ascii_case(&term.language)
            }) {
                new += 1;
            }
        }
        if saved.terms.len() + new > 512 {
            return Err(invalid(
                "A translated meaning can contain at most 512 shared terms",
            ));
        }
        entries.push(json!({"concept_uid":concept,"original_name":incoming.source.original_name,"new_terms":new,"definition_revision":incoming.definitions.last().map(|version|version.revision),"definition":incoming.definitions.last().map(|version|&version.definition),"terms":incoming.terms.iter().filter(|term|!term.retired).map(|term|json!({"name":term.name,"language":term.language})).collect::<Vec<_>>()}));
    }
    Ok(
        json!({"vocabulary_uid":package.vocabulary_uid,"entries":entries,"missing_meanings":missing,"can_import":missing.is_empty(),"changes_feature_bindings":false}),
    )
}

pub async fn import_translations_on(
    connection: &mut SqliteConnection,
    package: &TranslationPackage,
) -> Result<Value, StoreError> {
    let preview = preview_translations_on(connection, package).await?;
    if preview["can_import"] != true {
        return Err(invalid(
            "Adopt the missing source meanings before importing their translations",
        ));
    }
    for incoming in &package.dictionaries {
        for term in incoming.terms.iter().filter(|term| !term.retired) {
            let name = super::term_text(&term.name)?;
            let language = super::language_tag(&term.language)?;
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM concept_term WHERE concept_uid=? AND name=? AND LOWER(language)=? AND owner_uid IS NULL AND retired=0)").bind(&term.concept_uid).bind(&name).bind(&language).fetch_one(&mut *connection).await?;
            if !exists {
                super::add_term_on(connection, &term.concept_uid, &name, &language, None, false)
                    .await?;
            }
        }
    }
    Ok(preview)
}
