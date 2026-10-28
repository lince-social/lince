use super::*;

#[test]
fn full_queue_preserves_one_loading_deadline_and_no_unbounded_pending_work() {
    let (sender, mut receiver) = tokio::sync::mpsc::channel(1);
    sender.try_send(ClientMessage::Unsubscribe { id: "occupied".into() }).unwrap();
    let mut feed = Feed::new(RULE_NAMESPACE);
    let ids = BTreeSet::from([nucleus::new_uid("r")]);
    subscribe(&mut feed, ids.clone(), Some(&sender));
    let deadline = feed.deadline.unwrap();
    for _ in 0..32 {
        feed.retry = None;
        subscribe(&mut feed, ids.clone(), Some(&sender));
        assert_eq!(feed.deadline, Some(deadline));
        assert!(feed.id.is_none());
    }
    assert!(matches!(receiver.try_recv().unwrap(), ClientMessage::Unsubscribe { id } if id == "occupied"));
    assert!(receiver.try_recv().is_err());
}

#[test]
fn retirement_keeps_the_original_connection_and_blocks_new_subscriptions_until_sent() {
    let (old, mut old_requests) = tokio::sync::mpsc::channel(1);
    let (new, mut new_requests) = tokio::sync::mpsc::channel(1);
    old.try_send(ClientMessage::Unsubscribe { id: "occupied".into() }).unwrap();
    let mut feed = Feed::new(RULE_NAMESPACE);
    feed.id = Some("original-subscription".into());
    retire(&mut feed, Some(&old));
    assert!(feed.retiring.is_some());
    let ids = BTreeSet::from([nucleus::new_uid("r")]);
    subscribe(&mut feed, ids.clone(), Some(&new));
    assert!(new_requests.try_recv().is_err());
    assert!(feed.id.is_none());
    old_requests.try_recv().unwrap();
    feed.retry = None;
    subscribe(&mut feed, ids, Some(&new));
    assert!(matches!(old_requests.try_recv().unwrap(), ClientMessage::Unsubscribe { id } if id == "original-subscription"));
    assert!(matches!(new_requests.try_recv().unwrap(), ClientMessage::Subscribe { .. }));
    assert!(feed.retiring.is_none());
    assert!(feed.id.is_some());
}

#[test]
fn missing_window_extension_is_inheritance_but_an_invalid_rule_stays_unavailable() {
    let uid = nucleus::new_uid("r");
    let (_, entry) = read_entry(WINDOW_NAMESPACE, serde_json::json!({"uid":uid,"head":"Captured window"})).unwrap();
    assert!(matches!(entry.payload, Ok(Payload::Window(Window { revision: 0, properties })) if properties == Properties::default()));
    let (_, entry) = read_entry(RULE_NAMESPACE, serde_json::json!({"uid":uid,"head":"Broken rule"})).unwrap();
    assert!(entry.exists);
    assert!(entry.payload.is_err());
}
