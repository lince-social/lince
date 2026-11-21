use super::*;
use crate::transfer::receipt_fragment::{State, resolve};

#[test]
fn fragment_resolution_handles_the_full_budget_without_recursive_ancestry() {
    let publication = publication();
    let mut events = BTreeMap::new();
    let mut parent = None;
    for index in 0..5000 {
        let mut event = publication.events[0].event.clone();
        event.source_commitment = id(&format!("fragment-source-{index}"));
        event.supersedes = parent;
        let key = event.id().unwrap();
        parent = Some(key.clone());
        events.insert(key, event);
    }
    let selected = events.keys().cloned().collect();
    let result = resolve(&publication.scope, &events, &selected, 1100).unwrap();
    assert_eq!(result.projections.len(), 5000);
    assert!(result.unresolved.is_empty());
    assert!(result.missing.is_empty());
    let mut extra = publication.events[0].event.clone();
    extra.source_commitment = id("over-budget");
    events.insert(extra.id().unwrap(), extra);
    assert!(resolve(&publication.scope, &events, &selected, 1100).is_err());
}

#[test]
fn fragments_propagate_missing_history_and_ignore_unrelated_dangling_events() {
    let publication = publication();
    let root = publication.events[0].event.clone();
    let mut child = root.clone();
    child.supersedes = Some(root.id().unwrap());
    child.source_commitment = id("child");
    let mut dangling = root.clone();
    dangling.supersedes = Some(id("unavailable"));
    dangling.source_commitment = id("unrelated");
    let mut events = [
        (child.id().unwrap(), child.clone()),
        (dangling.id().unwrap(), dangling),
    ]
    .into_iter()
    .collect::<BTreeMap<_, _>>();
    let selected = [child.id().unwrap()].into_iter().collect();
    let result = resolve(&publication.scope, &events, &selected, 1100).unwrap();
    assert!(result.projections.is_empty());
    assert_eq!(result.unresolved[0].state, State::Incomplete);
    assert_eq!(result.missing, vec![root.id().unwrap()]);
    events.insert(root.id().unwrap(), root);
    let result = resolve(&publication.scope, &events, &selected, 1100).unwrap();
    assert_eq!(result.projections.len(), 1);
    assert!(result.unresolved.is_empty());
    assert!(result.missing.is_empty());
}

#[test]
fn fragment_dependencies_cannot_change_role_need_type_or_publication_order() {
    let publication = publication();
    let root = publication.events[0].event.clone();
    for mode in 0..3 {
        let mut invalid = root.clone();
        invalid.source_commitment = id(&format!("invalid-link-{mode}"));
        if mode == 0 {
            invalid.content = Content::Dispute {
                role: Role::Giver,
                active: true,
            };
            invalid.supersedes = Some(root.id().unwrap());
        } else if mode == 1 {
            if let Content::Statement { need, .. } = &mut invalid.content {
                *need = Some(root.id().unwrap());
            }
        } else {
            invalid.issued_at -= 1;
            invalid.supersedes = Some(root.id().unwrap());
        }
        let mut child = invalid.clone();
        child.supersedes = Some(invalid.id().unwrap());
        child.source_commitment = id(&format!("invalid-descendant-{mode}"));
        let selected = [invalid.id().unwrap(), child.id().unwrap()]
            .into_iter()
            .collect();
        let events = [root.clone(), invalid, child]
            .into_iter()
            .map(|event| (event.id().unwrap(), event))
            .collect();
        let result = resolve(&publication.scope, &events, &selected, 1100).unwrap();
        assert!(result.projections.is_empty());
        assert_eq!(result.unresolved.len(), 2);
        assert!(
            result
                .unresolved
                .iter()
                .all(|issue| issue.state == State::Invalid)
        );
    }
}
