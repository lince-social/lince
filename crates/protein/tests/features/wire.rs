use super::fixtures::base;
use protein::{Predicate, Protein, Source, validate};
use serde_json::{from_value, json};

#[test]
fn filter_groups_allow_ten_indents_and_reject_eleven() {
    fn nested(levels: usize) -> Predicate {
        let mut predicate = Predicate::UidEq("record".into());
        for _ in 0..=levels {
            predicate = Predicate::All(vec![predicate]);
        }
        predicate
    }
    assert!(validate(&base(Source::Record, vec![nested(10)])).is_ok());
    let error = validate(&base(Source::Record, vec![nested(11)])).unwrap_err();
    assert!(error.to_string().contains("protein_filter_depth_exceeded"));
}

#[test]
fn filter_groups_cannot_be_negated() {
    let query = base(
        Source::Record,
        vec![Predicate::Not(Box::new(Predicate::All(vec![
            Predicate::KindEq("plain".into()),
        ])))],
    );
    let error = validate(&query).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("protein_filter_group_negation_unsupported")
    );
}

#[test]
fn legacy_link_include_spelling_is_rejected() {
    let result = from_value::<Protein>(json!({
        "source": "record",
        "include": { "links": { "kind": "tag" } }
    }));
    assert!(result.is_err());
}

#[test]
fn legacy_filter_field_is_rejected() {
    let result = from_value::<Protein>(json!({
        "source": "record",
        "filter": [{ "kind_eq": "plain" }]
    }));
    assert!(result.is_err());
}
