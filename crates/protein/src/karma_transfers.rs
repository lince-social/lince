use serde_json::Value;

use crate::ProteinError;

pub(crate) async fn append(store: &store::Store, row: &mut Value) -> Result<(), ProteinError> {
    let Some(uid) = row["uid"].as_str() else {
        return Ok(());
    };
    let mut state = store::transfers::karma_snapshot::read(&store.pool, uid).await?;
    if row["revision"].as_u64() != Some(state.revision) {
        return Err(store::StoreError::Protocol(
            "transfer_karma_snapshot_changed: Transfer terms changed while reading".into(),
        ));
    }
    let people: std::collections::BTreeSet<_> = row["parties"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|party| party["actor"].as_str().map(str::to_owned))
        .collect();
    state
        .participants
        .retain(|person, _| people.contains(person));
    row["karma_state"] = serde_json::to_value(state)
        .map_err(|error| store::StoreError::Protocol(error.to_string()))?;
    Ok(())
}
