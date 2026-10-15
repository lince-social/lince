use nucleus::{
    RecordKind,
    visibility::{Condition, Data, Policy},
};
use store::{Store, records::NewRecord};

#[tokio::test]
async fn saved_place_projection_hides_private_points_and_respects_linked_area_rules() {
    let store = Store::open_memory().await.unwrap();
    let reader = nucleus::new_uid("r");
    let mut records = Vec::new();
    for name in ["Journey", "Private street 4"] {
        records.push(
            store::records::create(
                &store.pool,
                NewRecord {
                    slug: None,
                    kind: RecordKind::Plain,
                    head: name,
                    body: "",
                    quantity: store::exact::zero(),
                },
            )
            .await
            .unwrap()
            .uid,
        );
    }
    let place = store::places::create(&store.pool, -23.5505, -46.6333, Some("Private street 4"))
        .await
        .unwrap();
    store::places::set_record_place(&store.pool, &records[1], &place)
        .await
        .unwrap();
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::assertions::insert_tx(
        &mut tx,
        &nucleus::new_uid("a"),
        store::assertions::NewAssertion {
            subject_uid: &records[0],
            predicate_uid: nucleus::vocabulary::TOPOS_UID,
            object_uid: Some(&records[1]),
            role: store::assertions::AssertionRole::Ordinary,
            quantity: None,
            unit_uid: None,
            asserted_by: None,
        },
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    let hidden = store::places::saved_locations_for_person(&store.pool, None, Some(&reader))
        .await
        .unwrap();
    assert!(hidden[&records[0]].candidates.is_empty());
    assert!(hidden[&records[1]].candidates.is_empty());
    for (index, record) in records.iter().enumerate() {
        let policy = Policy {
            include: vec![Condition {
                persons: vec![reader.clone()],
                radius_metres: Some(if index == 0 { 1000 } else { 5000 }),
                ..Default::default()
            }],
            exclude: vec![],
        };
        let mut tx = store::write_tx(&store.pool).await.unwrap();
        let revision: Option<i64> = store::sqlx::query_scalar(
            "SELECT revision FROM data_visibility WHERE record_uid=? AND data='place'",
        )
        .bind(record)
        .fetch_optional(&mut *tx)
        .await
        .unwrap();
        store::data_visibility::save_on(
            &mut tx,
            record,
            Data::Place,
            None,
            &policy,
            revision.unwrap_or(0) as u64,
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
    }
    let shared = store::places::saved_locations_for_person(&store.pool, None, Some(&reader))
        .await
        .unwrap();
    let point = shared[&records[0]].selected.as_ref().unwrap();
    assert_eq!(point.radius_metres, Some(5000));
    assert_eq!(point.head, "Approximate area");
    assert!(point.slug.is_none());
    assert_ne!(point.latitude, -23.5505);
    let raw = store::places::saved_locations_for_person(&store.pool, None, None)
        .await
        .unwrap();
    assert_eq!(
        raw[&records[1]].selected.as_ref().unwrap().latitude,
        -23.5505
    );
}

#[tokio::test]
async fn person_record_audience_bounds_role_visibility_in_the_same_organ() {
    let store = Store::open_memory().await.unwrap();
    let role = store::auth::ensure_role(&store.pool, "reader")
        .await
        .unwrap();
    let first = store::auth::create_person_login(&store.pool, "First", "first", "hash", role)
        .await
        .unwrap();
    let second = store::auth::create_person_login(&store.pool, "Second", "second", "hash", role)
        .await
        .unwrap();
    let record = store::records::create(
        &store.pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head: "Private record",
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid;
    let policy = Policy {
        include: vec![Condition {
            persons: vec![first.clone()],
            ..Default::default()
        }],
        exclude: vec![],
    };
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::data_visibility::save_on(&mut tx, &record, Data::Record, None, &policy, 0)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(
        store::visibility::role_targets(&store.pool, &first, role)
            .await
            .unwrap()
            .contains(&record)
    );
    assert!(
        !store::visibility::role_targets(&store.pool, &second, role)
            .await
            .unwrap()
            .contains(&record)
    );
    let mut policy = policy;
    policy.exclude.push(Condition {
        persons: vec![first.clone()],
        ..Default::default()
    });
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::data_visibility::save_on(&mut tx, &record, Data::Record, None, &policy, 1)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(
        !store::visibility::role_targets(&store.pool, &first, role)
            .await
            .unwrap()
            .contains(&record)
    );
}
