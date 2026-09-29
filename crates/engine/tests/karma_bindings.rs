use engine::{Engine, actions::Action};
use nucleus::karma::rule_field::{RuleFieldInput, RuleFieldKind};

mod support;

fn field(source: &str) -> RuleFieldInput {
    RuleFieldInput::Text {
        source: source.into(),
    }
}

async fn rule(
    engine: &Engine,
    condition: &str,
    target: &str,
) -> Result<String, engine::EngineError> {
    engine
        .act(
            Action::SaveKarmaRule {
                identity: None,
                rule: None,
                expected_revision: None,
                fields: [field(condition), field(">0"), field(&format!("@{target}"))],
                request_id: nucleus::new_uid("req"),
            },
            None,
        )
        .await
        .map(|outcome| outcome.created.unwrap())
}

async fn rename(engine: &Engine, target: &str, slug: &str) {
    engine
        .act(
            Action::SetSlug {
                target: target.into(),
                slug: Some(slug.into()),
            },
            None,
        )
        .await
        .unwrap();
}

async fn level(engine: &Engine, record: &str) -> String {
    store::facts::level(&engine.store.pool, record)
        .await
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn saved_references_survive_renames_slug_reuse_and_unrelated_edits() {
    let engine = support::engine().await;
    let source = support::plain(&engine, "source", 2.0).await;
    let target = support::plain(&engine, "target", 0.0).await;
    let uid = rule(&engine, "@source", "target").await.unwrap();
    rename(&engine, &source, "original-source").await;
    let replacement = support::plain(&engine, "source", 90.0).await;
    rename(&engine, &target, "original-target").await;
    let new_target = support::plain(&engine, "target", 0.0).await;
    let threshold = store::karma_fields::for_rule(&engine.store.pool, &uid)
        .await
        .unwrap()
        .into_iter()
        .find(|value| value.kind == RuleFieldKind::Threshold)
        .unwrap();
    engine
        .act(
            Action::ReviseKarmaField {
                field: threshold.uid,
                expected_revision: threshold.revision,
                source: ">=1".into(),
                request_id: "threshold-edit".into(),
            },
            None,
        )
        .await
        .unwrap();
    engine.append_user(&source, 1.0).await.unwrap();
    assert_eq!(level(&engine, &target).await, "3");
    assert_eq!(level(&engine, &new_target).await, "0");
    engine.append_user(&replacement, 1.0).await.unwrap();
    assert_eq!(level(&engine, &target).await, "3");
    let stored = store::recurrence::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    let condition = stored.condition.unwrap();
    assert_eq!(condition.source, "@source");
    assert_eq!(condition.bindings[0].target.as_str(), source);
    assert_eq!(stored.record_uid, target);
}

#[tokio::test]
async fn missing_and_wrong_kind_references_do_not_save_a_rule() {
    let engine = support::engine().await;
    support::plain(&engine, "ordinary", 1.0).await;
    support::plain(&engine, "target", 0.0).await;
    for condition in ["@missing", "freq(@ordinary)"] {
        assert!(rule(&engine, condition, "target").await.is_err());
    }
    assert!(
        store::recurrence::all(&engine.store.pool)
            .await
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn cells_resolve_the_same_slug_to_their_own_identity() {
    let first = support::engine().await;
    let second = support::engine().await;
    let mut targets = Vec::new();
    for engine in [&first, &second] {
        let source = support::plain(engine, "source", 1.0).await;
        support::plain(engine, "target", 0.0).await;
        let uid = rule(engine, "@source", "target").await.unwrap();
        let stored = store::recurrence::get(&engine.store.pool, &uid)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            stored.condition.unwrap().bindings[0].target.as_str(),
            source
        );
        targets.push(source);
    }
    assert_ne!(targets[0], targets[1]);
}

#[tokio::test]
async fn shared_fields_keep_bindings_when_linked_after_slug_reuse() {
    let engine = support::engine().await;
    let source = support::plain(&engine, "source", 2.0).await;
    let target = support::plain(&engine, "target", 0.0).await;
    let original = rule(&engine, "@source", "target").await.unwrap();
    rename(&engine, &source, "original-source").await;
    rename(&engine, &target, "original-target").await;
    support::plain(&engine, "source", 90.0).await;
    support::plain(&engine, "target", 0.0).await;
    let fields = store::karma_fields::for_rule(&engine.store.pool, &original)
        .await
        .unwrap();
    let linked = engine
        .act(
            Action::SaveKarmaRule {
                identity: None,
                rule: None,
                expected_revision: None,
                fields: RuleFieldKind::ALL.map(|kind| {
                    let field = fields.iter().find(|field| field.kind == kind).unwrap();
                    RuleFieldInput::Reference {
                        uid: field.uid.clone(),
                        revision: field.revision,
                    }
                }),
                request_id: "shared-after-rename".into(),
            },
            None,
        )
        .await
        .unwrap()
        .created
        .unwrap();
    let stored = store::recurrence::get(&engine.store.pool, &linked)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.record_uid, target);
    assert_eq!(
        stored.condition.unwrap().bindings[0].target.as_str(),
        source
    );
}

#[tokio::test]
async fn deleted_target_never_rebinds_to_a_reused_slug() {
    let engine = support::engine().await;
    let source = support::plain(&engine, "source", 2.0).await;
    let target = support::plain(&engine, "target", 0.0).await;
    let uid = rule(&engine, "@source", "target").await.unwrap();
    rename(&engine, &source, "original-source").await;
    engine
        .act(
            Action::DeleteRecord {
                target: source.clone(),
            },
            None,
        )
        .await
        .unwrap();
    let before = level(&engine, &target).await;
    let replacement = support::plain(&engine, "source", 80.0).await;
    engine.append_user(&replacement, 1.0).await.unwrap();
    assert_eq!(level(&engine, &target).await, before);
    let stored = store::recurrence::get(&engine.store.pool, &uid)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        stored.condition.unwrap().bindings[0].target.as_str(),
        source
    );
}
