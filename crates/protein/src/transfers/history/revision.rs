use std::collections::BTreeSet;

use nucleus::{Fact, performance::Span, transfer::TransferRevisionEvidence};
use serde_json::{Value, from_str, json, to_value};
use store::{Store, transfers::revision_fact};

use crate::{ProteinError, transfers::transfer_query_error};

pub(in crate::transfers) fn parse_transfer_revision_evidence(
    fact: &Fact,
) -> Option<TransferRevisionEvidence> {
    from_str(fact.payload.as_deref()?).ok()
}

fn transfer_revision_summary(
    fact: &Fact,
    evidence: &TransferRevisionEvidence,
    include_terms: bool,
) -> Value {
    let mut value = json!({
        "revision": evidence.terms.revision,
        "previous_revision": evidence.previous_revision,
        "action": evidence.action,
        "fact": fact.uid,
        "actor": fact.actor_uid,
        "at": fact.at.to_rfc3339(),
        "hash": fact.hash,
        "signature": fact.signature,
        "signed": fact.signature.is_some(),
        "party_count": evidence.terms.parties.len(),
        "invitation_count": evidence.terms.invitations.len(),
        "promise_count": evidence.terms.promises.len(),
    });
    if include_terms {
        value["terms"] = json!(evidence.terms);
    }
    value
}

fn collect_changed_fields(before: &Value, after: &Value, path: &mut String, out: &mut Vec<String>) {
    match (before, after) {
        (Value::Object(before), Value::Object(after)) => {
            let keys: BTreeSet<_> = before.keys().chain(after.keys()).collect();
            let base = path.len();
            for key in keys {
                if base == 0 && key == "revision" {
                    continue;
                }
                if base != 0 {
                    path.push('.');
                }
                path.push_str(key);
                match (before.get(key), after.get(key)) {
                    (Some(before), Some(after)) => collect_changed_fields(before, after, path, out),
                    _ => out.push(path.clone()),
                }
                path.truncate(base);
            }
        }
        _ if before != after => out.push(path.clone()),
        _ => {}
    }
}

pub(in crate::transfers) async fn transfer_revision_projection(
    store: &Store,
    transfer_uid: &str,
    revision: u64,
    include_terms: bool,
) -> Result<Value, ProteinError> {
    let _evidence = Span::new("protein-transfer-revision-evidence-await");
    if revision == 0 {
        return Ok(json!({
            "current": null,
            "previous": null,
            "changed_fields": [],
            "blocking_reason": "legacy_transfer_requires_adoption",
        }));
    }
    let current = revision_fact(&store.pool, transfer_uid, revision).await?;
    let current_evidence = current.as_ref().and_then(parse_transfer_revision_evidence);
    let previous = if revision > 1 {
        revision_fact(&store.pool, transfer_uid, revision - 1).await?
    } else {
        None
    };
    let previous_evidence = previous.as_ref().and_then(parse_transfer_revision_evidence);

    let mut changed_fields = Vec::new();
    if let (Some(previous), Some(current)) = (&previous_evidence, &current_evidence) {
        let before = to_value(&previous.terms)
            .map_err(|error| transfer_query_error("protein_transfer_evidence_invalid", error))?;
        let after = to_value(&current.terms)
            .map_err(|error| transfer_query_error("protein_transfer_evidence_invalid", error))?;
        collect_changed_fields(&before, &after, &mut String::new(), &mut changed_fields);
    }

    Ok(json!({
        "current": current.as_ref().zip(current_evidence.as_ref())
            .map(|(fact, evidence)| transfer_revision_summary(fact, evidence, include_terms)),
        "previous": previous.as_ref().zip(previous_evidence.as_ref())
            .map(|(fact, evidence)| transfer_revision_summary(fact, evidence, false)),
        "changed_fields": changed_fields,
        "blocking_reason": if current_evidence.is_none() {
            Some("transfer_revision_evidence_missing")
        } else {
            None
        },
    }))
}
