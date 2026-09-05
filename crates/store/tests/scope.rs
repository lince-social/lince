use store::sync_ops::{LinkScope, OpRow, narrow_ops_to_scope};

fn no_links() -> LinkScope {
    LinkScope::default()
}

fn op(field: &str, kind: &str) -> OpRow {
    OpRow {
        seq: 1,
        tbl: "record".into(),
        uid: "r-1".into(),
        field: field.into(),
        kind: kind.into(),
        value: None,
        hlc: 1,
        actor_cell: "c-1".into(),
        organ_uid: "o-1".into(),
        replica_root: None,
    }
}

#[test]
fn only_named_columns_travel() {
    let scope = vec!["head".to_string()];
    let kept = narrow_ops_to_scope(
        vec![op("head", "set"), op("body", "set"), op("quantity", "set")],
        Some(&scope),
        &no_links(),
    );
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].field, "head");
}

#[test]
fn a_delete_rides_under_every_scope() {
    for scope in [vec!["head".to_string()], Vec::new()] {
        let kept = narrow_ops_to_scope(
            vec![op("", "tombstone"), op("quantity", "set")],
            Some(&scope),
            &no_links(),
        );
        assert_eq!(kept.len(), 1, "the delete rides and the column does not");
        assert_eq!(kept[0].kind, "tombstone");
    }
}

#[test]
fn collaborative_text_is_filtered_by_what_it_carries() {
    let wants_text = vec!["head".to_string(), "body".to_string()];
    let kept = narrow_ops_to_scope(
        vec![op("", "crdt"), op("", "snapshot")],
        Some(&wants_text),
        &no_links(),
    );
    assert_eq!(kept.len(), 2, "a scope naming the text gets the document");

    let no_text = vec!["quantity".to_string()];
    let kept = narrow_ops_to_scope(
        vec![op("", "crdt"), op("", "snapshot")],
        Some(&no_text),
        &no_links(),
    );
    assert!(
        kept.is_empty(),
        "a scope naming neither head nor body must not receive the document that holds both"
    );

    let nothing: Vec<String> = Vec::new();
    let kept = narrow_ops_to_scope(vec![op("", "crdt")], Some(&nothing), &no_links());
    assert!(kept.is_empty(), "and the empty scope least of all");
}

#[test]
fn an_empty_scope_is_not_an_absent_one() {
    let none: Vec<String> = Vec::new();
    let kept = narrow_ops_to_scope(
        vec![op("head", "set"), op("", "tombstone")],
        Some(&none),
        &no_links(),
    );
    assert_eq!(kept.len(), 1, "the delete still rides");
    assert_eq!(kept[0].kind, "tombstone");

    let unnarrowed = narrow_ops_to_scope(
        vec![op("head", "set"), op("", "tombstone")],
        None,
        &no_links(),
    );
    assert_eq!(unnarrowed.len(), 2, "no scope means everything");
}

fn op_on(tbl: &str, field: &str, kind: &str) -> OpRow {
    OpRow {
        tbl: tbl.into(),
        ..op(field, kind)
    }
}

#[test]
fn no_table_rides_on_an_empty_field_alone() {
    let head_only = vec!["head".to_string()];
    let kept = narrow_ops_to_scope(
        vec![
            op_on("fact", "", "fact"),
            op_on("record_assertion", "", "set"),
            op_on("record_assertion", "", "tombstone"),
            op_on("concept", "", "tombstone"),
            op_on("concept", "canonical_name", "set"),
        ],
        Some(&head_only),
        &no_links(),
    );
    assert!(
        kept.is_empty(),
        "not one of these names a column the scope asked for"
    );
}

#[test]
fn a_fact_answers_for_the_quantity_column() {
    let wants = vec!["quantity".to_string()];
    assert_eq!(
        narrow_ops_to_scope(vec![op_on("fact", "", "fact")], Some(&wants), &no_links()).len(),
        1,
        "a scope naming quantity gets the deltas that make it"
    );
    let other = vec!["head".to_string()];
    assert!(
        narrow_ops_to_scope(vec![op_on("fact", "", "fact")], Some(&other), &no_links()).is_empty(),
        "and one that does not, does not"
    );
}

#[test]
fn the_tombstone_exemption_belongs_to_records_alone() {
    let nothing: Vec<String> = Vec::new();
    let kept = narrow_ops_to_scope(
        vec![
            op_on("record", "", "tombstone"),
            op_on("record_assertion", "", "tombstone"),
            op_on("concept", "", "tombstone"),
        ],
        Some(&nothing),
        &no_links(),
    );
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].tbl, "record");
}

#[test]
fn an_unclassified_table_stays_home() {
    let wide = vec![
        "head".to_string(),
        "body".to_string(),
        "quantity".to_string(),
    ];
    assert!(
        narrow_ops_to_scope(
            vec![op_on("something_new", "head", "set")],
            Some(&wide),
            &no_links()
        )
        .is_empty()
    );
    assert_eq!(
        narrow_ops_to_scope(
            vec![op_on("something_new", "head", "set")],
            None,
            &no_links()
        )
        .len(),
        1,
        "an unnarrowed contact is unaffected — this is a narrowing rule, not a table allowlist"
    );
}

fn link_op(kind: &str, predicate_uid: Option<&str>) -> OpRow {
    let mut row = op_on("record_assertion", "", kind);
    row.value = predicate_uid.map(|uid| serde_json::json!({ "predicate_uid": uid }).to_string());
    row
}

fn links(all: bool, predicates: &[&str]) -> LinkScope {
    LinkScope {
        all,
        predicates: predicates.iter().map(|uid| uid.to_string()).collect(),
    }
}

#[test]
fn a_scope_naming_no_links_receives_none() {
    let scope = vec!["head".to_string()];
    let kept = narrow_ops_to_scope(
        vec![link_op("set", Some("c_part_of")), op("head", "set")],
        Some(&scope),
        &no_links(),
    );
    assert_eq!(kept.len(), 1, "fail-closed: narrowing costs the graph");
    assert_eq!(kept[0].tbl, "record");
}

#[test]
fn link_star_carries_every_link() {
    let scope = vec!["head".to_string(), "link:*".to_string()];
    let kept = narrow_ops_to_scope(
        vec![
            link_op("set", Some("c_part_of")),
            link_op("set", Some("c_blocks")),
            op("head", "set"),
        ],
        Some(&scope),
        &links(true, &[]),
    );
    assert_eq!(kept.len(), 3);
}

#[test]
fn a_named_predicate_carries_only_its_own_links() {
    let scope = vec!["head".to_string(), "link:part-of".to_string()];
    let kept = narrow_ops_to_scope(
        vec![
            link_op("set", Some("c_part_of")),
            link_op("set", Some("c_blocks")),
        ],
        Some(&scope),
        &links(false, &["c_part_of"]),
    );
    assert_eq!(kept.len(), 1);
    assert!(kept[0].value.as_deref().unwrap().contains("c_part_of"));
}

#[test]
fn a_link_whose_predicate_cannot_be_read_stays_home() {
    let scope = vec!["link:part-of".to_string()];
    let kept = narrow_ops_to_scope(
        vec![link_op("set", None)],
        Some(&scope),
        &links(false, &["c_part_of"]),
    );
    assert!(kept.is_empty());
}

#[test]
fn a_retraction_rides_whenever_any_link_does() {
    let scope = vec!["link:part-of".to_string()];
    let kept = narrow_ops_to_scope(
        vec![link_op("tombstone", None)],
        Some(&scope),
        &links(false, &["c_part_of"]),
    );
    assert_eq!(
        kept.len(),
        1,
        "a retraction carries no predicate, and withholding it leaves a ghost link forever"
    );
}

#[test]
fn the_predicate_concept_rides_with_the_links_that_need_it() {
    let scope = vec!["link:*".to_string()];
    let kept = narrow_ops_to_scope(
        vec![op_on("concept", "", "set")],
        Some(&scope),
        &links(true, &[]),
    );
    assert_eq!(
        kept.len(),
        1,
        "a link whose predicate has no name is unreadable"
    );

    let no_links_scope = vec!["head".to_string()];
    let kept = narrow_ops_to_scope(
        vec![op_on("concept", "", "set")],
        Some(&no_links_scope),
        &no_links(),
    );
    assert!(
        kept.is_empty(),
        "and a scope wanting no links needs no names"
    );
}
