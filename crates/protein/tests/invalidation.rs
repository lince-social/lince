use nucleus::{
    Cause, CauseKind, Fact, NewFact,
    fact::{seal, zero_delta},
};
use protein::{
    Aggregate, AggregateOp, ExtensionInclude, FactChanges, FactsInclude, GroupBy, Include,
    Predicate, Protein, Source, affects,
};

fn query(uid: &str) -> Protein {
    Protein {
        source: Source::Record,
        filter: vec![Predicate::UidEq(uid.into())],
        fields: Some(["uid", "head", "updated_at"].map(str::to_owned).into()),
        include: Default::default(),
        aggregate: None,
        order: Vec::new(),
        limit: None,
    }
}

fn text_fact() -> Fact {
    seal(
        NewFact {
            uid: Some("f_invalidation".into()),
            record_uid: "r_edited".into(),
            delta: zero_delta(),
            at: None,
            actor_uid: None,
            cause: Cause::user_edit(),
            payload: Some("{\"collab\":true}".into()),
        },
        "",
        "2026-11-05T00:00:00Z"
            .parse()
            .expect("fixed RFC 3339 test instant"),
    )
}

#[test]
fn local_direct_record_queries_follow_their_own_text_facts() {
    let fact = text_fact();
    assert!(affects(&query("r_edited"), &fact, None));
    assert!(!affects(&query("r_other"), &fact, None));
    let mut empty = query("r_other");
    empty.fields = Some(Vec::new());
    assert!(!affects(&empty, &fact, None));
}

#[test]
fn delegated_viewers_and_unknown_text_origins_remain_conservative() {
    let query = query("r_other");
    let fact = text_fact();
    assert!(affects(&query, &fact, Some("reader")));
    let edits: [fn(&mut Fact); 5] = [
        |fact| fact.actor_uid = Some("writer".into()),
        |fact| fact.cause.kind = CauseKind::Rule,
        |fact| fact.cause.uid = Some("cause".into()),
        |fact| fact.delta = store::exact::from_f64(1.0),
        |fact| fact.record_uid.clear(),
    ];
    for edit in edits {
        let mut changed = fact.clone();
        edit(&mut changed);
        assert!(affects(&query, &changed, None), "unclassified text edit");
    }
    for payload in [
        None,
        Some("{}"),
        Some("{\"collab\":true,\"other\":true}"),
        Some("{ collab"),
    ] {
        let mut changed = fact.clone();
        changed.payload = payload.map(str::to_owned);
        assert!(affects(&query, &changed, None), "payload {payload:?}");
    }
}

#[test]
fn unrestricted_derived_and_compound_queries_keep_the_full_refresh_path() {
    let fact = text_fact();
    let base = query("r_other");
    let edits: [fn(&mut Protein); 7] = [
        |query| query.filter.clear(),
        |query| query.filter = vec![Predicate::All(vec![Predicate::UidEq("r_other".into())])],
        |query| query.filter = vec![Predicate::UidEq(String::new())],
        |query| query.fields = None,
        |query| query.fields = Some(vec!["concept_name".into()]),
        |query| query.source = Source::Fact,
        |query| {
            query.aggregate = Some(Aggregate {
                op: AggregateOp::Count,
                by: GroupBy::Total,
            })
        },
    ];
    for edit in edits {
        let mut changed = base.clone();
        edit(&mut changed);
        assert!(affects(&changed, &fact, None), "query {changed:?}");
    }
    for include in [
        Include {
            reference_reads: true,
            ..Default::default()
        },
        Include {
            facts: Some(FactsInclude { limit: 1 }),
            ..Default::default()
        },
        Include {
            extension: Some(ExtensionInclude {
                namespace: "derived".into(),
            }),
            ..Default::default()
        },
        Include {
            record_after: Some("r_cursor".into()),
            ..Default::default()
        },
        Include {
            availability: true,
            ..Default::default()
        },
        Include {
            shader_effect: true,
            ..Default::default()
        },
        Include {
            numeric_extensions: true,
            ..Default::default()
        },
        Include {
            contact: true,
            ..Default::default()
        },
        Include {
            conversations: true,
            ..Default::default()
        },
    ] {
        let changed = Protein {
            include,
            ..base.clone()
        };
        assert!(affects(&changed, &fact, None), "query {changed:?}");
    }
}

#[test]
fn ephemeral_sources_still_use_their_own_update_signal() {
    let mut query = query("r_other");
    query.source = Source::Nearby;
    for subject in [None, Some("reader")] {
        assert!(!affects(&query, &text_fact(), subject));
    }
}

#[test]
fn local_batch_distinguishes_single_multiple_and_unclassified_edits() {
    let edited = text_fact();
    let second = Fact {
        record_uid: "r_second".into(),
        ..edited.clone()
    };
    let unknown = Fact {
        payload: None,
        ..edited.clone()
    };
    let outsider = query("r_other");

    let repeated = [edited.clone(), edited.clone()];
    let changes = FactChanges::new(&repeated, None);
    assert!(changes.affects(&query("r_edited")), "first UID is affected");
    assert!(!changes.affects(&outsider), "same UID repeated");

    let mixed = [edited.clone(), second];
    let changes = FactChanges::new(&mixed, None);
    assert!(changes.affects(&query("r_edited")), "first of two UIDs");
    assert!(changes.affects(&query("r_second")), "second of two UIDs");
    assert!(!changes.affects(&outsider), "unrelated UID");

    let general = [edited, unknown];
    assert!(
        FactChanges::new(&general, None).affects(&outsider),
        "unknown edit affects all local reads"
    );
    assert!(
        !FactChanges::new(&[], None).affects(&outsider),
        "empty batch"
    );
}
