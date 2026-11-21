//! Merge independently received approvals and event evidence for the same scope.

use super::{Document, Review, view};
use crate::performance::Span;

fn merge_review(current: view::Bundle<'_>, incoming: view::Bundle<'_>) -> Result<Review, String> {
    if current.publication.scope != incoming.publication.scope {
        return Err("Changed receipt terms require a new scope and new approvals".into());
    }
    let mut person_approvals = current.publication.person_approvals.to_vec();
    for approval in incoming.publication.person_approvals {
        if let Some(previous) = person_approvals
            .iter()
            .find(|item| item.role == approval.role)
        {
            if previous != approval {
                return Err("Conflicting personal consent requires a separate review".into());
            }
        } else {
            person_approvals.push(approval.clone());
        }
    }
    person_approvals.sort_by_key(|approval| approval.role);

    let mut approvals = current.publication.approvals.to_vec();
    for approval in incoming.publication.approvals {
        if let Some(previous) = approvals.iter().find(|item| item.role == approval.role) {
            if previous != approval {
                return Err("Conflicting approvals must be reviewed separately".into());
            }
        } else {
            approvals.push(approval.clone());
        }
    }
    approvals.sort_by_key(|approval| approval.role);

    let mut authorities = current.authorities.to_vec();
    for authority in incoming.authorities {
        if !authorities.contains(authority) {
            authorities.push(authority.clone());
        }
    }
    Ok(Review {
        person_approvals,
        scope: current.publication.scope.clone(),
        approvals,
        authorities,
    })
}

pub(super) fn documents(
    current: &Document,
    incoming: &Document,
    now: i64,
) -> Result<Document, String> {
    let _span = Span::new("receipt-model-merge");
    if current == incoming {
        current.validate(now)?;
        return Ok(current.clone());
    }
    let merged = match (current, incoming) {
        (Document::Review(current), Document::Review(incoming)) => Document::Review(merge_review(
            view::Bundle::from(current),
            view::Bundle::from(incoming),
        )?),
        (Document::Publication(current), Document::Publication(incoming)) => {
            let review = merge_review(view::Bundle::from(current), view::Bundle::from(incoming))?;
            let mut bundle = review.into_bundle();
            bundle.publication.events = current.publication.events.clone();
            for signed in &incoming.publication.events {
                if let Some(previous) = bundle
                    .publication
                    .events
                    .iter()
                    .find(|item| item.event == signed.event)
                {
                    if previous != signed {
                        return Err("An event has conflicting signature envelopes".into());
                    }
                } else {
                    bundle.publication.events.push(signed.clone());
                }
            }
            Document::Publication(bundle)
        }
        _ => return Err("Receipt document identity or kind changed".into()),
    };
    merged.validate(now)?;
    Ok(merged)
}
