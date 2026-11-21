use std::collections::BTreeSet;

use nucleus::RecordKind;
use protein_authority::{
    authority::{ConceptState, GraphSnapshot, Limits, RecordState, selector_membership_changes},
    query::{LinkDirection, Predicate},
};

use crate::fixtures::{assertion, c, r};

pub(super) fn record(number: u128) -> RecordState {
    RecordState {
        uid: r(number),
        kind: RecordKind::Plain,
        organ_uid: None,
        deleted: false,
        content: None,
    }
}

pub(super) fn graph() -> GraphSnapshot {
    GraphSnapshot {
        records: (1..=6).map(record).collect(),
        concepts: (1..=4)
            .map(|number| ConceptState {
                uid: c(number),
                name: String::new(),
                parents: if number == 2 {
                    BTreeSet::from([c(1)])
                } else {
                    BTreeSet::new()
                },
            })
            .collect(),
        assertions: Vec::new(),
        resolved_links: Default::default(),
        places: BTreeSet::new(),
    }
}

pub(super) fn under(include_self: bool) -> Predicate {
    Predicate::Under {
        record: r(1),
        kind: c(4),
        include_self,
    }
}

pub(super) fn hierarchy() -> GraphSnapshot {
    let mut graph = graph();
    graph.assertions = vec![
        assertion(1, 3, 4, Some(1)),
        assertion(2, 4, 4, Some(3)),
        assertion(3, 5, 4, Some(4)),
    ];
    graph
}

pub(super) fn relation(direction: LinkDirection, other: Option<u128>) -> Predicate {
    Predicate::Relation {
        kind: c(1),
        direction,
        other: other.map(r),
    }
}

pub(super) fn changed(
    selectors: &[Predicate],
    current: &GraphSnapshot,
    proposed: &GraphSnapshot,
) -> BTreeSet<String> {
    selector_membership_changes(selectors, current, proposed, &Limits::default())
        .expect("valid selector fixture")
}

pub(super) fn ids(numbers: &[u128]) -> BTreeSet<String> {
    numbers.iter().copied().map(r).collect()
}
