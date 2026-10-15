use super::invalid;
use crate::StoreError;
use nucleus::vocabulary::{DefinitionVersion, reference::MeaningReference};
use sqlx::SqliteConnection;

pub async fn conversion_reference_on(
    connection: &mut SqliteConnection,
    from_vocabulary: &str,
    from: &str,
    to_vocabulary: &str,
    to: &str,
    revision: i64,
) -> Result<nucleus::vocabulary::reference::ConversionReference, StoreError> {
    let row: Option<(i64,i64,String,String,bool)> = sqlx::query_as(
        "SELECT a_revision,b_revision,numerator,denominator,withdrawn FROM concept_conversion_version WHERE a_uid=? AND b_uid=? AND revision=?",
    ).bind(from).bind(to).bind(revision).fetch_optional(&mut *connection).await?;
    let (from_revision, to_revision, numerator, denominator, withdrawn) =
        row.ok_or_else(|| invalid("Choose a saved conversion version"))?;
    let from = meaning_reference_on(connection, from_vocabulary, from, from_revision).await?;
    let to = meaning_reference_on(connection, to_vocabulary, to, to_revision).await?;
    nucleus::vocabulary::reference::ConversionReference::new(
        from,
        to,
        revision,
        numerator
            .parse()
            .map_err(|_| invalid("Invalid conversion numerator"))?,
        denominator
            .parse()
            .map_err(|_| invalid("Invalid conversion denominator"))?,
        withdrawn,
    )
    .map_err(invalid)
}

pub async fn meaning_reference_on(
    connection: &mut SqliteConnection,
    vocabulary: &str,
    concept: &str,
    revision: i64,
) -> Result<MeaningReference, StoreError> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT v.definition,v.created_at FROM concept_definition_version v JOIN lingua_concept c ON c.concept_uid=v.concept_uid WHERE c.lingua_uid=? AND v.concept_uid=? AND v.revision=?",
    )
    .bind(vocabulary)
    .bind(concept)
    .bind(revision)
    .fetch_optional(connection)
    .await?;
    let (definition, created_at) =
        row.ok_or_else(|| invalid("Meaning version is not in this Vocabulary"))?;
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
