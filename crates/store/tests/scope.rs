//! Per-contact field narrowing (Ontology §12, cluster C5).
//!
//! The narrowing itself is one predicate; what these pin is the rule that
//! makes it safe to apply at all.

use store::sync_ops::{OpRow, narrow_ops_to_scope};

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

/// A scope names what travels. Everything else stays home.
#[test]
fn only_named_columns_travel() {
    let scope = vec!["head".to_string()];
    let kept = narrow_ops_to_scope(
        vec![op("head", "set"), op("body", "set"), op("quantity", "set")],
        Some(&scope),
    );
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].field, "head");
}

/// A TOMBSTONE IS NOT A FIELD, and this is the test that matters most here.
///
/// A tombstone uses `field = ''`, so an allowlist naming columns would exclude
/// every DELETE by construction. A narrowed contact would never learn a Record
/// was removed and their copy would live forever — worse than the leak the
/// narrowing was for. It rides under every scope there is, including none.
#[test]
fn a_delete_rides_under_every_scope() {
    for scope in [vec!["head".to_string()], Vec::new()] {
        let kept = narrow_ops_to_scope(
            vec![op("", "tombstone"), op("quantity", "set")],
            Some(&scope),
        );
        assert_eq!(kept.len(), 1, "the delete rides and the column does not");
        assert_eq!(kept[0].kind, "tombstone");
    }
}

/// Collaborative text is NOT a tombstone, and sharing the empty `field` is the
/// only thing they have in common. A `crdt` op carries the whole Loro
/// document — which is `head` and `body` — so keying on the empty field alone
/// shipped both columns past every scope that excluded them. Withholding them
/// strands nothing, so they are filtered by what they actually carry.
#[test]
fn collaborative_text_is_filtered_by_what_it_carries() {
    let wants_text = vec!["head".to_string(), "body".to_string()];
    let kept = narrow_ops_to_scope(vec![op("", "crdt"), op("", "snapshot")], Some(&wants_text));
    assert_eq!(kept.len(), 2, "a scope naming the text gets the document");

    let no_text = vec!["quantity".to_string()];
    let kept = narrow_ops_to_scope(vec![op("", "crdt"), op("", "snapshot")], Some(&no_text));
    assert!(
        kept.is_empty(),
        "a scope naming neither head nor body must not receive the document that holds both"
    );

    let nothing: Vec<String> = Vec::new();
    let kept = narrow_ops_to_scope(vec![op("", "crdt")], Some(&nothing));
    assert!(kept.is_empty(), "and the empty scope least of all");
}

/// An EMPTY scope is a real answer — nothing but deletes — and is not the same
/// as no scope at all.
#[test]
fn an_empty_scope_is_not_an_absent_one() {
    let none: Vec<String> = Vec::new();
    let kept = narrow_ops_to_scope(vec![op("head", "set"), op("", "tombstone")], Some(&none));
    assert_eq!(kept.len(), 1, "the delete still rides");
    assert_eq!(kept[0].kind, "tombstone");

    let unnarrowed = narrow_ops_to_scope(vec![op("head", "set"), op("", "tombstone")], None);
    assert_eq!(unnarrowed.len(), 2, "no scope means everything");
}

fn op_on(tbl: &str, field: &str, kind: &str) -> OpRow {
    OpRow {
        tbl: tbl.into(),
        ..op(field, kind)
    }
}

/// The empty `field` marks "not a column", never "always send" — and the rule
/// has to hold for EVERY table that uses one, not just the one that was
/// noticed first.
///
/// `crdt`/`snapshot` were fixed alone, and `fact`, `record_assertion` and
/// `concept` kept riding past every scope for another day. A contact narrowed
/// to `head` was still receiving every quantity change, every link, and the
/// whole vocabulary of this Cell.
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
    );
    assert!(
        kept.is_empty(),
        "not one of these names a column the scope asked for"
    );
}

/// A fact IS the `quantity` column — it logs under an empty field because the
/// fact's own uid is its identity, which is not the same as being field-less.
#[test]
fn a_fact_answers_for_the_quantity_column() {
    let wants = vec!["quantity".to_string()];
    assert_eq!(
        narrow_ops_to_scope(vec![op_on("fact", "", "fact")], Some(&wants)).len(),
        1,
        "a scope naming quantity gets the deltas that make it"
    );
    let other = vec!["head".to_string()];
    assert!(
        narrow_ops_to_scope(vec![op_on("fact", "", "fact")], Some(&other)).is_empty(),
        "and one that does not, does not"
    );
}

/// Only a RECORD tombstone is exempt. The exemption exists because a withheld
/// delete strands a row the peer already holds — and an Assertion they never
/// received strands nothing.
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
    );
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].tbl, "record");
}

/// A table nobody has classified travels to nobody narrowed. The alternative
/// is that adding a sixth logged table leaks it by default, which is the
/// deny-list failure this cluster refuses everywhere else.
#[test]
fn an_unclassified_table_stays_home() {
    let wide = vec!["head".to_string(), "body".to_string(), "quantity".to_string()];
    assert!(
        narrow_ops_to_scope(vec![op_on("something_new", "head", "set")], Some(&wide)).is_empty()
    );
    assert_eq!(
        narrow_ops_to_scope(vec![op_on("something_new", "head", "set")], None).len(),
        1,
        "an unnarrowed contact is unaffected — this is a narrowing rule, not a table allowlist"
    );
}
