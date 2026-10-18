use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use nucleus::transfer::{
    TransferRevisionEvidence,
    receipt::{Authority, Document, Participation},
};
use sqlx::{Sqlite, Transaction};

use super::ActivateOccurrencesInput;
use crate::StoreError;

fn invalid(message: impl ToString) -> StoreError {
    StoreError::Protocol(message.to_string())
}

pub(super) async fn read_on(
    tx: &mut Transaction<'_, Sqlite>,
    records: &[String],
    input: &ActivateOccurrencesInput,
    revision: &TransferRevisionEvidence,
    fact_hash: &str,
    now: DateTime<Utc>,
) -> Result<Vec<Authority>, StoreError> {
    if records.is_empty() {
        return Ok(Vec::new());
    }
    if revision.terms.transfer.uid != input.transfer_uid
        || revision.terms.revision != input.expected_revision
    {
        return Err(invalid(
            "Original participation requires the exact signed Transfer revision",
        ));
    }
    let mut expected =
        vec![Participation::from_revision(revision, fact_hash, None).map_err(invalid)?];
    let people: BTreeSet<_> = input
        .occurrences
        .iter()
        .flat_map(|occurrence| {
            [
                occurrence.giver_person_uid.as_str(),
                occurrence.receiver_person_uid.as_str(),
            ]
        })
        .collect();
    for person in people {
        expected.push(
            Participation::from_revision(revision, fact_hash, Some(person)).map_err(invalid)?,
        );
    }
    let mut scopes = BTreeSet::new();
    let mut authorities = Vec::with_capacity(records.len());
    for uid in records {
        let saved = crate::transfer_receipts::get_on(tx, uid, None).await?;
        let Document::Authority(authority) = saved.document else {
            return Err(invalid("Choose an original participation approval"));
        };
        authority.validate(now.timestamp()).map_err(invalid)?;
        let terms = authority.participation.as_ref().ok_or_else(|| {
            invalid("Receipt signing authority is not an original participation approval")
        })?;
        if !expected.contains(terms)
            || authority.issued_at > now.timestamp()
            || authority.expires_at <= now.timestamp()
            || !scopes.insert((terms.organ.clone(), terms.party_commitment.clone()))
        {
            return Err(invalid(
                "Participation approval does not match the activated parties, revision or time",
            ));
        }
        authorities.push(authority);
    }
    Ok(authorities)
}
