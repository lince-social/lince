use engine::{Engine, actions::Action};
use nucleus::window_opening::{Properties, Rule, RULE_NAMESPACE};
use serde_json::json;

#[tokio::test]
async fn opening_extension_content_comparison_allows_only_one_concurrent_revision() {
    let engine = Engine::open_memory().await.unwrap();
    let uid = engine.act(Action::CreateRecord { slug: None, kind: nucleus::RecordKind::Plain, head: "Opening-rule owner".into(), body: String::new(), quantity: 0.0 }, None).await.unwrap().created.unwrap();
    let rule = Rule { revision: 1, enabled: true, match_all: true, conditions: vec![nucleus::property_rule::PropertyRule { property: nucleus::property_rule::Property::ApplicationId, value: "org.example.Editor".into() }], properties: Properties::default() };
    let first = serde_json::to_value(rule).unwrap();
    engine.act(Action::SetExtension { target: uid.clone(), namespace: RULE_NAMESPACE.into(), fds: first.clone() }, None).await.unwrap();
    let mut a = first.clone();
    a["revision"] = json!(2);
    a["properties"]["focus"] = json!(false);
    let mut b = first;
    b["revision"] = json!(2);
    b["properties"]["floating"] = json!(true);
    let (a_result, b_result) = tokio::join!(
        engine.act(Action::SetExtension { target: uid.clone(), namespace: RULE_NAMESPACE.into(), fds: a.clone() }, None),
        engine.act(Action::SetExtension { target: uid.clone(), namespace: RULE_NAMESPACE.into(), fds: b.clone() }, None),
    );
    assert_ne!(a_result.is_ok(), b_result.is_ok());
    let rejected = if let Err(error) = &a_result { error } else { b_result.as_ref().unwrap_err() };
    assert!(rejected.to_string().contains("changed"), "Expected a revision/content conflict, got {rejected}");
    let actual = store::records::get_extension(&engine.store.pool, &uid, RULE_NAMESPACE).await.unwrap().unwrap();
    assert_eq!(actual, if a_result.is_ok() { a } else { b });
    let before = actual.clone();
    let mut invalid = actual;
    invalid["revision"] = json!(3);
    invalid["properties"]["capabilities"] = json!(["capture"]);
    assert!(engine.act(Action::SetExtension { target: uid.clone(), namespace: RULE_NAMESPACE.into(), fds: invalid }, None).await.is_err());
    assert_eq!(store::records::get_extension(&engine.store.pool, &uid, RULE_NAMESPACE).await.unwrap().unwrap(), before);
}
