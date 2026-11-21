use std::collections::HashSet;

use nucleus::{RecordKind, new_uid, vocabulary::TOPOS_UID};
use serde_json::json;
use sqlx::{SqlitePool, query};

use super::{create, of_record, saved_locations_for_person_among, set_record_place};
use crate::{
    Store,
    assertions::{AssertionRole, NewAssertion, insert_tx},
    exact::zero,
    records::{NewRecord, create as create_record},
    write_tx,
};

async fn record(pool: &SqlitePool, head: &str) -> String {
    create_record(
        pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head,
            body: "",
            quantity: zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

#[tokio::test]
async fn scoped_locations_preserve_competing_candidates_and_newer_choices() {
    let store = Store::open_memory().await.unwrap();
    let pool = &store.pool;
    let root = record(pool, "Journey").await;
    let public = record(pool, "Visible address").await;
    let private = record(pool, "Hidden address").await;
    let visible_assertion = new_uid("a");
    let hidden_assertion = new_uid("a");
    for (target, assertion, latitude) in [
        (&public, &visible_assertion, 1.0),
        (&private, &hidden_assertion, 2.0),
    ] {
        let place = create(pool, latitude, 3.0, None).await.unwrap();
        set_record_place(pool, target, &place).await.unwrap();
        let mut tx = write_tx(pool).await.unwrap();
        insert_tx(
            &mut tx,
            assertion,
            NewAssertion {
                subject_uid: &root,
                predicate_uid: TOPOS_UID,
                object_uid: Some(target),
                role: AssertionRole::Ordinary,
                quantity: None,
                unit_uid: None,
                asserted_by: None,
            },
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
    let direct = of_record(pool, &public).await.unwrap().unwrap();
    assert_eq!((direct.lat, direct.lon), (1.0, 3.0));
    let visible = HashSet::from([root.clone(), public]);
    let roots = [root.clone()];
    let read = async || {
        saved_locations_for_person_among(pool, Some(&visible), None, &roots)
            .await
            .unwrap()
            .remove(&root)
            .unwrap()
    };
    let projection = read().await;
    assert_eq!(projection.candidates.len(), 1);
    assert!(!projection.explicit && projection.selected.is_none());

    query("INSERT INTO record_location_choice (record_uid, source, assertion_uid, revision) VALUES (?, 'assertion', ?, 1)")
        .bind(&root).bind(&visible_assertion).execute(pool).await.unwrap();
    let projection = read().await;
    assert_eq!(projection.revision, 1);
    assert_eq!(
        projection.selected.unwrap().assertion_uid.as_deref(),
        Some(visible_assertion.as_str())
    );

    query("INSERT INTO record_property (record_uid, property, clock, peer, change_uid, value) VALUES (?, 'location.source', 2, 'test', 'change', ?)")
        .bind(&root).bind(json!({"assertion": hidden_assertion}).to_string()).execute(pool).await.unwrap();
    let projection = read().await;
    assert_eq!(projection.revision, 2);
    assert!(projection.explicit && projection.unavailable && projection.selected.is_none());
}
