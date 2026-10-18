use super::*;
use nucleus::trust_analysis::{claims, query};

#[cfg(test)]
mod tests;

pub(super) fn claim_checks(
    review: &Review,
    originals: &Originals,
    groups: &[evidence::ReceiptGroup],
    contexts: &BTreeMap<Identity, IdentityContext>,
    paths: &paths::ResultSet,
    heads: &[evidence::ClaimHead],
    partial: bool,
    now: i64,
) -> Result<Vec<Value>, EngineError> {
    let mut checks = Vec::new();
    for selected in &review.draft.linked_claims {
        let head = heads.iter().find(|head| head.issuer == selected.issuer && head.claim == selected.claim);
        let mut result = json!({"issuer":selected.issuer,"claim":selected.claim,"state":"unknown","review_required":false});
        let Some(head) = head else {
            checks.push(result);
            continue;
        };
        result["head"] = json!(head);
        if head.state != evidence::HeadState::CurrentObserved {
            result["state"] = json!(head.state);
            checks.push(result);
            continue;
        }
        let document = originals.items.iter().find_map(|original| {
            if original.signature != Verification::Accepted || original.provenance.is_none() {
                return None;
            }
            match original.artifact.as_ref() {
                Artifact::Claim(document) if document.statement.issuer == selected.issuer
                    && document.statement.claim == selected.claim
                    && Some(document.statement.revision) == head.revision => Some(document),
                _ => None,
            }
        });
        let Some(document) = document else {
            checks.push(result);
            continue;
        };
        result["document"] = json!(document);
        let same_scope = selected.snapshot.as_ref().is_some_and(|snapshot| {
            snapshot.statement.descriptor == document.statement.descriptor
                && snapshot.statement.audience == document.statement.audience
        });
        if !same_scope {
            result["state"] = json!("review_required");
            result["review_required"] = json!(true);
            checks.push(result);
            continue;
        }
        let query = document.statement.descriptor.checking_query(&review.draft.query, document.statement.as_of).map_err(invalid)?;
        let mut analysis = query::analyze(
            groups.iter().map(evidence::ReceiptGroup::source_evidence).collect(),
            contexts,
            query.contacts.as_ref().map(|_| paths),
            &query,
        ).map_err(invalid)?;
        analysis.partial |= partial || query != review.draft.query;
        let support = claims::compare(&document.statement, &query, &analysis).map_err(invalid)?;
        result["state"] = json!("checked");
        result["support"] = json!(support);
        result["query"] = json!(query);
        result["analysis"] = json!(analysis);
        result["checked_at"] = json!(now);
        checks.push(result);
    }
    Ok(checks)
}
