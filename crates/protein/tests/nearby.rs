use nucleus::{Cause, Fact, RecordKind, fact::zero_delta, nearby::NearbyPeer};
use protein::{Context, Protein, affects, execute_for, execute_for_with_context, is_ephemeral};
use serde_json::{Value, from_value, json};
use store::{
    Store,
    organs::{add_contact, set_node_id},
    records::{NewRecord, create},
};

fn nearby_protein() -> Protein {
    from_value(json!({ "source": "nearby" })).expect("protein")
}

fn peer(node_id: &str, name: &str) -> NearbyPeer {
    NearbyPeer {
        node_id: node_id.into(),
        fingerprint: node_id.chars().take(8).collect::<String>().to_uppercase(),
        name: name.into(),
    }
}

async fn run(store: &Store, peers: &[NearbyPeer], subject: Option<&str>) -> Vec<Value> {
    execute_for_with_context(
        store,
        &nearby_protein(),
        subject,
        None,
        Context {
            nearby: Some(peers),
        },
    )
    .await
    .expect("execute")
}

#[tokio::test]
async fn nearby_serves_the_peers_the_wire_currently_sees() {
    let store = Store::open_memory().await.expect("Nearby fixture store");
    let rows = run(
        &store,
        &[peer("bbb", "Tablet"), peer("aaa", "Laptop")],
        None,
    )
    .await;

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["node_id"], "aaa");
    assert_eq!(rows[1]["node_id"], "bbb");
    assert_eq!(rows[0]["claimed_name"], "Laptop");
    assert_eq!(rows[0]["name"], Value::Null);
    assert_eq!(rows[0]["known"], false);
}

#[tokio::test]
async fn nearby_is_never_exported_to_a_remote_subject() {
    let store = Store::open_memory().await.expect("Nearby fixture store");
    let peers = [peer("aaa", "Laptop")];
    let person = create(
        &store.pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Person,
            head: "Remote reader",
            body: "",
            quantity: zero_delta(),
        },
    )
    .await
    .expect("remote Person");

    assert_eq!(
        run(&store, &peers, None).await.len(),
        1,
        "the Cell sees its LAN"
    );
    assert!(
        run(&store, &peers, Some(&person.uid)).await.is_empty(),
        "who is physically near you must never leave the Cell, and an empty \
         answer is the shape the Decision Queue already established"
    );
}

#[tokio::test]
async fn an_unbound_endpoint_reads_as_an_empty_list_not_an_error() {
    let store = Store::open_memory().await.expect("Nearby fixture store");
    let rows = execute_for(&store, &nearby_protein(), None)
        .await
        .expect("no endpoint bound is not an error");
    assert!(rows.is_empty());
}

#[tokio::test]
async fn a_known_contact_is_matched_by_node_id_and_named_by_us() {
    let store = Store::open_memory().await.expect("Nearby fixture store");
    add_contact(&store.pool, "o-friend", None, "Marcia", "", 1)
        .await
        .unwrap();
    set_node_id(&store.pool, "o-friend", Some("aaa"))
        .await
        .unwrap();

    let rows = run(&store, &[peer("aaa", "definitely-marcia")], None).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["known"], true);
    assert_eq!(rows[0]["name"], "Marcia");
    assert_eq!(rows[0]["claimed_name"], "definitely-marcia");
}

#[tokio::test]
async fn a_nearby_subscription_is_woken_by_the_tick_and_not_by_facts() {
    let protein = nearby_protein();
    assert!(is_ephemeral(&protein));
    let fact = Fact {
        uid: "f-1".into(),
        record_uid: "r-1".into(),
        delta: zero_delta(),
        at: "2026-11-14T10:00:00Z".parse().expect("fixture timestamp"),
        actor_uid: None,
        cause: Cause::user_edit(),
        payload: None,
        prev_hash: String::new(),
        hash: String::new(),
        signature: None,
    };
    assert!(!affects(&protein, &fact, None));
    let records: Protein = from_value(json!({ "source": "record" })).unwrap();
    assert!(!is_ephemeral(&records));
    assert!(affects(&records, &fact, None));
}
