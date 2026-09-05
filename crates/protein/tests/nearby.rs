use engine::Engine;
use nucleus::nearby::NearbyPeer;
use protein::{Context, Protein};

async fn engine() -> Engine {
    Engine::open_memory().await.expect("engine")
}

fn nearby_protein() -> Protein {
    serde_json::from_value(serde_json::json!({ "source": "nearby" })).expect("protein")
}

fn peer(node_id: &str, name: &str) -> NearbyPeer {
    NearbyPeer {
        node_id: node_id.into(),
        fingerprint: node_id.chars().take(8).collect::<String>().to_uppercase(),
        name: name.into(),
    }
}

async fn run(e: &Engine, peers: &[NearbyPeer], subject: Option<&str>) -> Vec<serde_json::Value> {
    protein::execute_for_with_context(
        &e.store,
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
    let e = engine().await;
    let rows = run(&e, &[peer("bbb", "Tablet"), peer("aaa", "Laptop")], None).await;

    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["node_id"], "aaa");
    assert_eq!(rows[1]["node_id"], "bbb");
    assert_eq!(rows[0]["claimed_name"], "Laptop");
    assert_eq!(rows[0]["name"], serde_json::Value::Null);
    assert_eq!(rows[0]["known"], false);
}

#[tokio::test]
async fn nearby_is_never_exported_to_a_remote_subject() {
    let e = engine().await;
    let peers = [peer("aaa", "Laptop")];

    assert_eq!(
        run(&e, &peers, None).await.len(),
        1,
        "the Cell sees its LAN"
    );
    assert!(
        run(&e, &peers, Some("organ.somebody")).await.is_empty(),
        "who is physically near you must never leave the Cell, and an empty \
         answer is the shape the Decision Queue already established"
    );
}

#[tokio::test]
async fn an_unbound_endpoint_reads_as_an_empty_list_not_an_error() {
    let e = engine().await;
    let rows = protein::execute_for(&e.store, &nearby_protein(), None)
        .await
        .expect("no endpoint bound is not an error");
    assert!(rows.is_empty());
}

#[tokio::test]
async fn a_known_contact_is_matched_by_node_id_and_named_by_us() {
    let e = engine().await;
    store::organs::add_contact(&e.store.pool, "o-friend", None, "Marcia", "", 1)
        .await
        .unwrap();
    store::organs::set_node_id(&e.store.pool, "o-friend", Some("aaa"))
        .await
        .unwrap();

    let rows = run(&e, &[peer("aaa", "definitely-marcia")], None).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["known"], true);
    assert_eq!(rows[0]["name"], "Marcia");
    assert_eq!(rows[0]["claimed_name"], "definitely-marcia");
}

#[tokio::test]
async fn a_nearby_subscription_is_woken_by_the_tick_and_not_by_facts() {
    let protein = nearby_protein();
    assert!(protein::is_ephemeral(&protein));
    let fact = nucleus::Fact {
        uid: "f-1".into(),
        record_uid: "r-1".into(),
        delta: nucleus::fact::zero_delta(),
        at: chrono::Utc::now(),
        actor_uid: None,
        cause: nucleus::Cause::user_edit(),
        payload: None,
        prev_hash: String::new(),
        hash: String::new(),
        signature: None,
    };
    assert!(!protein::affects(&protein, &fact));
    let records: Protein =
        serde_json::from_value(serde_json::json!({ "source": "record" })).unwrap();
    assert!(!protein::is_ephemeral(&records));
    assert!(protein::affects(&records, &fact));
}
