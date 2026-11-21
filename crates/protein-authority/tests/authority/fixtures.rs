use std::collections::BTreeMap;

use nucleus::{DecimalValue, id::ulid_from};
use protein_authority::authority::{
    AssertionRole, AssertionState, AuthorityError, GraphSnapshot, MutationTarget, Property,
    RecordContent, VisibilityCeiling,
};

pub(crate) fn uid(prefix: &str, number: u128) -> String {
    format!("{prefix}_{}", ulid_from(0, number))
}

pub(crate) fn r(number: u128) -> String {
    uid("r", number)
}

pub(crate) fn c(number: u128) -> String {
    uid("c", number)
}

pub(crate) fn a(number: u128) -> String {
    uid("a", number)
}

pub(crate) fn quantity(amount: &str) -> DecimalValue {
    DecimalValue::parse_inferred(amount).expect("fixture quantity")
}

pub(crate) fn document(head: String, body: &str) -> RecordContent {
    RecordContent {
        slug: None,
        head,
        body: body.into(),
        quantity: quantity("0"),
        unit_uid: None,
        place_uid: None,
        extensions: BTreeMap::new(),
    }
}

pub(crate) fn assertion(
    number: u128,
    subject: u128,
    predicate: u128,
    object: Option<u128>,
) -> AssertionState {
    AssertionState {
        uid: a(number),
        subject_uid: r(subject),
        predicate_uid: c(predicate),
        object_uid: object.map(r),
        object_uri: None,
        role: AssertionRole::Ordinary,
        quantity: None,
        unit_uid: None,
    }
}

pub(crate) fn ceiling(graph: &GraphSnapshot) -> VisibilityCeiling {
    VisibilityCeiling {
        records: graph
            .records
            .iter()
            .map(|record| record.uid.clone())
            .collect(),
        concepts: graph
            .concepts
            .iter()
            .map(|concept| concept.uid.clone())
            .collect(),
        places: graph.places.clone(),
    }
}

pub(crate) fn target(number: u128, touched: impl IntoIterator<Item = Property>) -> MutationTarget {
    MutationTarget {
        record_uid: r(number),
        touched_properties: touched.into_iter().collect(),
    }
}

pub(crate) fn minimum_steps(
    mut high: usize,
    mut operation: impl FnMut(usize) -> Result<(), AuthorityError>,
) -> usize {
    let mut low = 0;
    assert_eq!(
        operation(high),
        Ok(()),
        "fixture must fit within the upper budget"
    );
    while low < high {
        let middle = low.midpoint(high);
        match operation(middle) {
            Ok(()) => high = middle,
            Err(AuthorityError::LimitExceeded) => low = middle + 1,
            Err(error) => panic!("unexpected authority error: {error}"),
        }
    }
    low
}
