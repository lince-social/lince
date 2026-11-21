//! Attaches read-only Karma state within a Transfer row's disclosure scope.

use std::collections::BTreeSet;

use serde_json::{Value, to_value};
use store::{Store, sqlx::Error::Protocol, transfers::karma_snapshot::read};

use crate::ProteinError;

pub(super) async fn append(store: &Store, row: &mut Value) -> Result<(), ProteinError> {
    let Some(uid) = row["uid"].as_str() else {
        return Ok(());
    };
    let mut state = read(&store.pool, uid).await?;
    if row["revision"].as_u64() != Some(state.revision) {
        return Err(Protocol(
            "transfer_karma_snapshot_changed: Transfer terms changed while reading".into(),
        ));
    }
    let people: BTreeSet<_> = row["parties"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|party| party["actor"].as_str())
        .collect();
    state
        .participants
        .retain(|person, _| people.contains(person.as_str()));
    row["karma_state"] = to_value(state).map_err(|error| Protocol(error.to_string()))?;
    Ok(())
}
