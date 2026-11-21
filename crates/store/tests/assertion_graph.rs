use nucleus::RecordKind;
use serde_json::json;
use store::{
    Store, StoreError,
    assertions::{
        AssertionRole, NewAssertion, assert, get, record_descendants, refine, transition_unary,
    },
    concepts,
    records::{NewRecord, create},
    sqlx::query_scalar,
    write_tx,
};

async fn record(store: &Store, head: &str) -> Result<String, StoreError> {
    Ok(create(
        &store.pool,
        NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await?
    .uid)
}

#[tokio::test]
async fn descendants_remain_breadth_first_through_cycles_and_duplicate_links()
-> Result<(), StoreError> {
    let store = Store::open_memory().await?;
    let root = record(&store, "Root").await?;
    let child = record(&store, "Child").await?;
    let sibling = record(&store, "Sibling").await?;
    let grandchild = record(&store, "Grandchild").await?;
    let leaf = record(&store, "Leaf").await?;
    let predicate = concepts::ensure(&store.pool, "descendant").await?;
    let alias = concepts::ensure(&store.pool, "descendant-alias").await?;
    for (subject, parent, concept) in [
        (&child, &root, &predicate),
        (&sibling, &root, &predicate),
        (&grandchild, &child, &predicate),
        (&grandchild, &child, &alias),
        (&leaf, &grandchild, &predicate),
        (&root, &leaf, &predicate),
    ] {
        assert(
            &store.pool,
            NewAssertion {
                subject_uid: subject,
                predicate_uid: concept,
                object_uid: Some(parent),
                role: AssertionRole::Ordinary,
                quantity: None,
                unit_uid: None,
                asserted_by: None,
            },
        )
        .await?;
    }
    let family = [predicate, alias];
    let with_root = record_descendants(&store.pool, &root, &family, true).await?;
    let without_root = record_descendants(&store.pool, &root, &family, false).await?;
    let [first, second, third, fourth] = without_root.as_slice() else {
        panic!("expected four distinct descendants, got {without_root:?}");
    };
    assert!(first == &child && second == &sibling || first == &sibling && second == &child);
    assert_eq!(third, &grandchild);
    assert_eq!(fourth, &leaf);
    assert_eq!(
        with_root,
        [&root]
            .into_iter()
            .chain(&without_root)
            .cloned()
            .collect::<Vec<_>>()
    );
    assert_eq!(
        record_descendants(&store.pool, &root, &[], true).await?,
        [root.as_str()],
    );
    assert!(
        record_descendants(&store.pool, &root, &[], false)
            .await?
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
async fn unary_transition_and_refinement_keep_their_sync_payloads() -> Result<(), StoreError> {
    let store = Store::open_memory().await?;
    let subject = record(&store, "Subject").await?;
    let object = record(&store, "Object").await?;
    let predicate = concepts::ensure(&store.pool, "refinement").await?;
    let mut tx = write_tx(&store.pool).await?;
    transition_unary(
        &mut tx,
        &subject,
        &[],
        std::slice::from_ref(&predicate),
        None,
    )
    .await?;
    tx.commit().await?;

    let unary = query_scalar::<_, String>(
        "SELECT uid FROM record_assertion WHERE subject_uid=? AND predicate_uid=?",
    )
    .bind(&subject)
    .bind(&predicate)
    .fetch_one(&store.pool)
    .await?;
    let binary = refine(&store.pool, &subject, &predicate, &object, None).await?;
    assert_ne!(unary, binary);
    assert_eq!(
        refine(&store.pool, &subject, &predicate, &object, None).await?,
        binary
    );

    for (uid, endpoint) in [(&unary, None), (&binary, Some(&object))] {
        let row = get(&store.pool, uid)
            .await?
            .expect("the authored Assertion remains stored");
        assert_eq!(row.retracted_at.is_some(), endpoint.is_none());
        let payload = query_scalar::<_, String>(
            "SELECT value FROM sync_op WHERE tbl='record_assertion' AND uid=? AND kind='set'",
        )
        .bind(uid)
        .fetch_one(&store.pool)
        .await?;
        assert_eq!(
            payload,
            json!({
                "subject_uid": subject,
                "predicate_uid": predicate,
                "object_uid": endpoint,
                "role": "ordinary",
                "quantity_mantissa": null,
                "quantity_scale": null,
                "unit_uid": null,
                "asserted_by": null,
                "created_at": row.created_at,
                "predicate_revision": row.predicate_revision,
                "unit_revision": row.unit_revision,
            })
            .to_string()
        );
    }
    Ok(())
}
