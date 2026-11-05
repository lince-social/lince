use protein::{Include, Predicate, Protein, Source};

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

fn text_fact() -> nucleus::Fact {
    nucleus::fact::seal(
        nucleus::NewFact {
            uid: Some("f_invalidation".into()),
            record_uid: "r_edited".into(),
            delta: nucleus::fact::zero_delta(),
            at: None,
            actor_uid: None,
            cause: nucleus::Cause::user_edit(),
            payload: Some("{\"collab\":true}".into()),
        },
        "",
        "2026-11-05T00:00:00Z".parse().unwrap(),
    )
}

#[test]
fn local_direct_record_queries_follow_their_own_text_facts() {
    let fact = text_fact();
    assert!(protein::affects(&query("r_edited"), &fact, None));
    assert!(!protein::affects(&query("r_other"), &fact, None));
    let mut empty = query("r_other");
    empty.fields = Some(Vec::new());
    assert!(!protein::affects(&empty, &fact, None));
}

#[test]
fn delegated_viewers_and_unknown_text_origins_remain_conservative() {
    let query = query("r_other");
    let fact = text_fact();
    assert!(protein::affects(&query, &fact, Some("reader")));
    let mut variants = Vec::new();
    let mut changed = fact.clone();
    changed.actor_uid = Some("writer".into());
    variants.push(changed);
    let mut changed = fact.clone();
    changed.cause.kind = nucleus::CauseKind::Rule;
    variants.push(changed);
    let mut changed = fact.clone();
    changed.cause.uid = Some("cause".into());
    variants.push(changed);
    let mut changed = fact.clone();
    changed.delta = store::exact::from_f64(1.0);
    variants.push(changed);
    let mut changed = fact.clone();
    changed.record_uid.clear();
    variants.push(changed);
    for payload in [
        None,
        Some("{}"),
        Some("{\"collab\":true,\"other\":true}"),
        Some("{ collab"),
    ] {
        let mut changed = fact.clone();
        changed.payload = payload.map(str::to_owned);
        variants.push(changed);
    }
    for changed in variants {
        assert!(protein::affects(&query, &changed, None));
    }
}

#[test]
fn unrestricted_derived_and_compound_queries_keep_the_full_refresh_path() {
    let fact = text_fact();
    let base = query("r_other");
    let mut variants = Vec::new();
    let mut changed = base.clone();
    changed.filter.clear();
    variants.push(changed);
    let mut changed = base.clone();
    changed.filter = vec![Predicate::All(vec![Predicate::UidEq("r_other".into())])];
    variants.push(changed);
    let mut changed = base.clone();
    changed.filter = vec![Predicate::UidEq(String::new())];
    variants.push(changed);
    let mut changed = base.clone();
    changed.fields = None;
    variants.push(changed);
    let mut changed = base.clone();
    changed.fields = Some(vec!["concept_name".into()]);
    variants.push(changed);
    let mut changed = base.clone();
    changed.source = Source::Fact;
    variants.push(changed);
    let mut changed = base.clone();
    changed.aggregate = Some(protein::Aggregate {
        op: protein::AggregateOp::Count,
        by: protein::GroupBy::Total,
    });
    variants.push(changed);
    for include in [
        Include {
            reference_reads: true,
            ..Default::default()
        },
        Include {
            facts: Some(protein::FactsInclude { limit: 1 }),
            ..Default::default()
        },
        Include {
            extension: Some(protein::ExtensionInclude {
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
        let mut changed = base.clone();
        changed.include = include;
        variants.push(changed);
    }
    for changed in variants {
        assert!(protein::affects(&changed, &fact, None));
    }
}

#[test]
fn ephemeral_sources_still_use_their_own_update_signal() {
    let mut query = query("r_other");
    query.source = Source::Nearby;
    for subject in [None, Some("reader")] {
        assert!(!protein::affects(&query, &text_fact(), subject));
    }
}
