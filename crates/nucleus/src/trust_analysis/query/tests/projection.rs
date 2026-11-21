use super::*;

#[test]
fn protocol_context_keeps_the_exact_historical_need_referenced_by_a_receipt() {
    let mut publication = publication();
    let original = publication.events[0].event.id().unwrap();
    let mut revised = publication.events[0].event.content.clone();
    if let Content::Need { requested, .. } = &mut revised {
        *requested = decimal("20");
    }
    push(&mut publication, revised, Some(original), "revised-need");
    let result = analyze(
        vec![source(&publication)],
        &contexts(&publication),
        None,
        &query(&publication),
    )
    .unwrap();
    assert_eq!(result.groups[0].raw_quantity.unwrap().canonical(), "4");
    assert!(result.protocol.iter().any(|row| !row.current
        && matches!(&row.content,Content::Need {requested,..} if requested.canonical()=="10")));
    assert!(result.protocol.iter().any(|row| row.current
        && matches!(&row.content,Content::Need {requested,..} if requested.canonical()=="20")));
}

#[test]
fn corrections_conflicts_withdrawals_and_disputes_do_not_inflate_contributions() {
    let input = |publication: &Publication| {
        let mut source = source(publication);
        for copy in &mut source.copies {
            if !matches!(copy.content, Content::Need { .. })
                && publication
                    .events
                    .iter()
                    .find(|signed| signed.event.id().unwrap() == copy.projection.event)
                    .unwrap()
                    .event
                    .supersedes
                    .is_none()
            {
                copy.provenance = Provenance::Artifact {
                    hash: hash("mixed-original-before-control"),
                };
            }
        }
        source
    };
    let mut publication = publication();
    let old = publication.events[2].event.id().unwrap();
    let mut corrected = publication.events[2].event.content.clone();
    if let Content::Statement { quantity, .. } = &mut corrected {
        *quantity = decimal("3");
    }
    let next = push(
        &mut publication,
        corrected.clone(),
        Some(old.clone()),
        "receiver-correction",
    );
    let context = contexts(&publication);
    let query = query(&publication);
    let result = analyze(vec![input(&publication)], &context, None, &query).unwrap();
    assert_eq!(result.groups[0].raw_quantity.unwrap().canonical(), "3");
    assert!(
        !result
            .rows
            .iter()
            .find(|row| row.selected)
            .unwrap()
            .records
            .is_empty()
    );
    let mut forked = publication.clone();
    if let Content::Statement { quantity, .. } = &mut corrected {
        *quantity = decimal("2");
    }
    push(&mut forked, corrected, Some(old), "fork");
    let result = analyze(vec![input(&forked)], &context, None, &query).unwrap();
    assert!(result.groups.is_empty());
    assert_eq!(result.withheld[0].reason, "conflicting_current_outcomes");
    let mut withdrawn = publication.clone();
    let mut statement = withdrawn.events[2].event.content.clone();
    if let Content::Statement {
        quantity,
        withdrawn,
        ..
    } = &mut statement
    {
        *quantity = decimal("0");
        *withdrawn = true;
    }
    push(&mut withdrawn, statement, Some(next), "withdrawn");
    let result = analyze(vec![input(&withdrawn)], &context, None, &query).unwrap();
    assert!(result.groups.is_empty());
    assert!(result.rows.iter().any(|row| row.withdrawn && !row.selected));
    push(
        &mut publication,
        Content::Dispute {
            role: Role::Receiver,
            active: true,
        },
        None,
        "dispute",
    );
    let result = analyze(vec![input(&publication)], &context, None, &query).unwrap();
    assert!(result.groups.is_empty());
    assert!(result.rows.iter().all(|row| row.disputed && !row.selected));
}
