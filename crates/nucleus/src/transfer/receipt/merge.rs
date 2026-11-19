use super::{Document, Review, view};

fn merge_review(current: view::Bundle<'_>, incoming: view::Bundle<'_>) -> Result<Review, String> {
    if current.publication.scope != incoming.publication.scope {
        return Err("Changed receipt terms require a new scope and new approvals".into());
    }
    let mut merged = Review {
        person_approvals: current.publication.person_approvals.to_vec(),
        scope: current.publication.scope.clone(),
        approvals: current.publication.approvals.to_vec(),
        authorities: current.authorities.to_vec(),
    };
    for approval in incoming.publication.person_approvals {
        if let Some(previous) = merged
            .person_approvals
            .iter()
            .find(|item| item.role == approval.role)
        {
            if previous != approval {
                return Err("Conflicting personal consent requires a separate review".into());
            }
        } else {
            merged.person_approvals.push(approval.clone());
        }
    }
    merged
        .person_approvals
        .sort_by_key(|approval| approval.role);
    for approval in incoming.publication.approvals {
        if let Some(previous) = merged
            .approvals
            .iter()
            .find(|item| item.role == approval.role)
        {
            if previous != approval {
                return Err("Conflicting approvals must be reviewed separately".into());
            }
        } else {
            merged.approvals.push(approval.clone());
        }
    }
    merged.approvals.sort_by_key(|approval| approval.role);
    for authority in incoming.authorities {
        if !merged.authorities.contains(authority) {
            merged.authorities.push(authority.clone());
        }
    }
    Ok(merged)
}

pub(super) fn documents(
    current: &Document,
    incoming: &Document,
    now: i64,
) -> Result<Document, String> {
    let _span = crate::performance::Span::new("receipt-model-merge");
    if current == incoming {
        current.validate(now)?;
        return Ok(current.clone());
    }
    let merged = match (current, incoming) {
        (Document::Authority(current), Document::Authority(incoming)) if current == incoming => {
            Document::Authority(current.clone())
        }
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
