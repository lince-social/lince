//! Query entry points and dispatch to each read source.

mod access;

use self::access::{can_read_source, readable_records};
use crate::{
    Protein, ProteinError, Source, calendar, conjunction, karma_rules, schedule,
    sources::{
        execute_assertions, execute_auth, execute_concepts, execute_decisions, execute_entries,
        execute_facts, execute_frequency, execute_karma, execute_linguas, execute_nearby,
        execute_promises, execute_records, execute_recurrence, execute_timeline,
    },
    transfers, validate,
};
use nucleus::nearby::NearbyPeer;
use serde_json::{Value, from_value};
use store::{
    Store,
    records::{get_extension, resolve},
    sqlx::Error::Protocol,
};

/// Live data supplied by the caller in addition to the stored records.
#[derive(Debug, Clone, Copy, Default)]
pub struct Context<'a> {
    /// Current peers available to a Nearby query.
    pub nearby: Option<&'a [NearbyPeer]>,
}

/// Runs a query in the local, unscoped read context.
///
/// # Errors
/// Returns an error when validation or a source read fails.
pub async fn execute(store: &Store, protein: &Protein) -> Result<Vec<Value>, ProteinError> {
    execute_for(store, protein, None).await
}

/// Runs a query within the given subject's read scope when supplied.
///
/// # Errors
/// Returns an error when validation, permission lookup, or a source read fails.
pub async fn execute_for(
    store: &Store,
    protein: &Protein,
    subject: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    execute_for_with_signer(store, protein, subject, None).await
}

/// Runs a query with the installed signer available to signer-dependent sources.
///
/// # Errors
/// Returns an error when validation, permission lookup, or a source read fails.
pub async fn execute_for_with_signer(
    store: &Store,
    protein: &Protein,
    subject: Option<&str>,
    installed_signer_actor: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    execute_for_with_context(
        store,
        protein,
        subject,
        installed_signer_actor,
        Context::default(),
    )
    .await
}

/// Runs a query with the caller's read scope, signer, and live data.
///
/// # Errors
/// Returns an error when validation, permission lookup, or a source read fails.
pub async fn execute_for_with_context(
    store: &Store,
    protein: &Protein,
    subject: Option<&str>,
    installed_signer_actor: Option<&str>,
    context: Context<'_>,
) -> Result<Vec<Value>, ProteinError> {
    validate(protein)?;
    let prepared = conjunction::prepare(protein)?;
    let protein = prepared.as_ref();
    let visible = readable_records(store, subject).await?;
    let visible = visible.as_ref();
    if !can_read_source(store, subject, protein.source).await? {
        return Ok(Vec::new());
    }
    match protein.source {
        Source::Decision | Source::Karma | Source::Nearby if visible.is_some() => Ok(Vec::new()),
        Source::Record => execute_records(store, protein, visible, subject).await,
        Source::Promise => execute_promises(store, protein, visible, subject).await,
        Source::Decision => execute_decisions(store, protein).await,
        Source::Fact => execute_facts(store, protein, visible, subject).await,
        Source::Timeline => execute_timeline(store, protein, visible, subject).await,
        Source::Calendar => calendar::execute(store, protein, visible, subject).await,
        Source::Schedule => schedule::execute(store, protein, visible, subject).await,
        Source::Entry => execute_entries(store, protein, visible).await,
        Source::Frequency => execute_frequency(store, visible).await,
        Source::Recurrence => execute_recurrence(store, protein, visible).await,
        Source::KarmaRule => {
            karma_rules::execute(store, protein, visible, subject, installed_signer_actor).await
        }
        Source::Concept => execute_concepts(store, protein, subject).await,
        Source::Lingua => execute_linguas(store, protein).await,
        Source::Assertion => execute_assertions(store, protein, subject).await,
        Source::Transfer => {
            transfers::execute(store, protein, visible, subject, installed_signer_actor).await
        }
        Source::TransferSettlementPreview => {
            transfers::execute_transfer_settlement_preview(
                store,
                protein,
                subject,
                installed_signer_actor,
                visible,
            )
            .await
        }
        Source::TransferBulkCompletionPreview => {
            transfers::execute_transfer_bulk_completion_preview(
                store,
                protein,
                subject,
                installed_signer_actor,
            )
            .await
        }
        Source::Auth => execute_auth(store, subject).await,
        Source::Karma => execute_karma(store, protein).await,
        Source::Nearby => execute_nearby(store, protein, context.nearby.unwrap_or_default()).await,
    }
}

/// Loads a saved query by Record slug or UID and runs it within the read scope.
///
/// # Errors
/// Returns an error if the query cannot be loaded, decoded, or executed.
pub async fn execute_saved(
    store: &Store,
    slug_or_uid: &str,
    subject: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    execute_saved_with_signer(store, slug_or_uid, subject, None).await
}

/// Runs a saved query with the installed signer available to its source.
///
/// # Errors
/// Returns an error if the query cannot be loaded, decoded, or executed.
pub async fn execute_saved_with_signer(
    store: &Store,
    slug_or_uid: &str,
    subject: Option<&str>,
    installed_signer_actor: Option<&str>,
) -> Result<Vec<Value>, ProteinError> {
    let record = resolve(&store.pool, slug_or_uid)
        .await?
        .ok_or_else(|| Protocol(format!("no saved protein {slug_or_uid}")))?;
    let ast = get_extension(&store.pool, &record.uid, "lince.protein")
        .await?
        .ok_or_else(|| Protocol(format!("{slug_or_uid} has no protein AST")))?;
    let protein = from_value(ast).map_err(|error| Protocol(format!("bad protein AST: {error}")))?;
    execute_for_with_signer(store, &protein, subject, installed_signer_actor).await
}
