//! References pinned to exact vocabulary definitions and conversion versions.

use nucleus::{
    valid_uid,
    vocabulary::{
        DefinitionVersion,
        reference::{ConversionReference, MeaningReference},
    },
};
use sqlx::{SqliteConnection, query_as};

use super::invalid;
use crate::StoreError;

/// Reads the latest meaning reference in the concept's source vocabulary.
///
/// # Errors
/// Returns an error for an invalid concept, an unavailable version or membership, or a failed read.
pub async fn current_meaning_reference_on(
    connection: &mut SqliteConnection,
    concept: &str,
) -> Result<MeaningReference, StoreError> {
    if !valid_uid(concept, "c") {
        return Err(invalid("Choose a canonical Concept identity"));
    }
    let (vocabulary, revision, definition, created_at, member): (
        String,
        i64,
        String,
        String,
        bool,
    ) = query_as(
        "SELECT s.lingua_uid, v.revision, v.definition, v.created_at, EXISTS (
             SELECT 1 FROM lingua_concept c
             WHERE c.lingua_uid = s.lingua_uid AND c.concept_uid = v.concept_uid
         ) FROM concept_source s JOIN concept_definition_version v ON v.concept_uid = s.concept_uid
         WHERE s.concept_uid = ? ORDER BY v.revision DESC LIMIT 1",
    )
    .bind(concept)
    .fetch_optional(connection)
    .await?
    .ok_or_else(|| invalid("The Concept has no known versioned definition"))?;
    if !member {
        return Err(invalid("Meaning version is not in this Vocabulary"));
    }
    reference(&vocabulary, concept, revision, definition, created_at)
}

/// Reads a conversion and the exact endpoint definitions saved with its revision.
///
/// # Errors
/// Returns an error if the version, endpoint membership, or stored ratio is invalid or unavailable.
pub async fn conversion_reference_on(
    connection: &mut SqliteConnection,
    from_vocabulary: &str,
    from: &str,
    to_vocabulary: &str,
    to: &str,
    revision: i64,
) -> Result<ConversionReference, StoreError> {
    let row: Option<(i64,i64,String,String,bool)> = query_as(
        "SELECT a_revision,b_revision,numerator,denominator,withdrawn FROM concept_conversion_version WHERE a_uid=? AND b_uid=? AND revision=?",
    ).bind(from).bind(to).bind(revision).fetch_optional(&mut *connection).await?;
    let (from_revision, to_revision, numerator, denominator, withdrawn) =
        row.ok_or_else(|| invalid("Choose a saved conversion version"))?;
    let from = meaning_reference_on(connection, from_vocabulary, from, from_revision).await?;
    let to = meaning_reference_on(connection, to_vocabulary, to, to_revision).await?;
    ConversionReference::new(
        from,
        to,
        revision,
        numerator
            .parse()
            .map_err(|error| invalid(&format!("Invalid conversion numerator: {error}")))?,
        denominator
            .parse()
            .map_err(|error| invalid(&format!("Invalid conversion denominator: {error}")))?,
        withdrawn,
    )
    .map_err(invalid)
}

/// Reads an exact meaning version through its membership in the selected vocabulary.
///
/// # Errors
/// Returns an error if the saved version or membership is unavailable, invalid, or unreadable.
pub async fn meaning_reference_on(
    connection: &mut SqliteConnection,
    vocabulary: &str,
    concept: &str,
    revision: i64,
) -> Result<MeaningReference, StoreError> {
    let row: Option<(String, String)> = query_as(
        "SELECT v.definition,v.created_at FROM concept_definition_version v JOIN lingua_concept c ON c.concept_uid=v.concept_uid WHERE c.lingua_uid=? AND v.concept_uid=? AND v.revision=?",
    )
    .bind(vocabulary)
    .bind(concept)
    .bind(revision)
    .fetch_optional(connection)
    .await?;
    let (definition, created_at) =
        row.ok_or_else(|| invalid("Meaning version is not in this Vocabulary"))?;
    reference(vocabulary, concept, revision, definition, created_at)
}

fn reference(
    vocabulary: &str,
    concept: &str,
    revision: i64,
    definition: String,
    created_at: String,
) -> Result<MeaningReference, StoreError> {
    MeaningReference::from_definition(
        vocabulary,
        &DefinitionVersion {
            concept_uid: concept.into(),
            revision,
            definition,
            created_at,
        },
    )
    .map_err(invalid)
}

#[cfg(test)]
mod tests {
    use super::{current_meaning_reference_on, meaning_reference_on};
    use crate::{
        Store,
        concepts::create_in,
        linguas::{create as create_vocabulary, remove_concept},
        vocabulary::revise_definition_on,
        write_tx,
    };

    #[tokio::test]
    async fn current_meaning_keeps_the_latest_version_and_requires_source_membership() {
        let store = Store::open_memory().await.unwrap();
        let pool = &store.pool;
        let vocabulary = create_vocabulary(pool, "References", None, "private")
            .await
            .unwrap();
        let concept = create_in(pool, &vocabulary, "parent", &[]).await.unwrap();
        let mut tx = write_tx(pool).await.unwrap();
        revise_definition_on(&mut tx, &concept, 1, "An origin for a new meaning")
            .await
            .unwrap();
        let expected = meaning_reference_on(&mut tx, &vocabulary, &concept, 2)
            .await
            .unwrap();
        assert_eq!(
            current_meaning_reference_on(&mut tx, &concept)
                .await
                .unwrap(),
            expected
        );
        tx.commit().await.unwrap();
        assert!(remove_concept(pool, &vocabulary, &concept).await.unwrap());

        let mut connection = pool.acquire().await.unwrap();
        let error = current_meaning_reference_on(&mut connection, &concept)
            .await
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Meaning version is not in this Vocabulary")
        );
    }
}
