use nucleus::RecordKind;
use store::Store;

async fn record(store: &Store, head: &str) -> String {
    store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: RecordKind::Plain,
            head,
            body: "",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap()
    .uid
}

async fn move_refuses_without_partial_changes(extension: bool, invalid_new: bool, missing: bool) {
    let store = Store::open_memory().await.unwrap();
    let first = record(&store, "First").await;
    let second = record(&store, "Second").await;
    let marker = record(&store, "Transaction marker").await;
    let row_query;
    let move_query;
    let row_uid;
    if extension {
        store::records::set_extension_raw(
            &store.pool,
            &first,
            "test.move",
            &serde_json::json!({"original": true}),
        )
        .await
        .unwrap();
        row_uid = first.clone();
        row_query = "SELECT json_array(record_uid, namespace, version, fds)
                     FROM record_extension WHERE namespace = 'test.move'";
        move_query = "UPDATE record_extension SET record_uid = ?, fds = '{}', version = 9
                      WHERE record_uid = ? AND namespace = 'test.move'";
    } else {
        let predicate = store::concepts::ensure(&store.pool, "move-edge")
            .await
            .unwrap();
        row_uid = store::assertions::assert(
            &store.pool,
            store::assertions::NewAssertion {
                subject_uid: &first,
                predicate_uid: &predicate,
                object_uid: None,
                role: store::assertions::AssertionRole::Ordinary,
                quantity: None,
                unit_uid: None,
                asserted_by: None,
            },
        )
        .await
        .unwrap();
        row_query = "SELECT json_array(uid, subject_uid, predicate_uid, object_uid, role,
                         quantity_mantissa, quantity_scale, unit_uid, asserted_by,
                         created_at, retracted_at, retracted_by)
                     FROM record_assertion";
        move_query = "UPDATE record_assertion SET subject_uid = ?, asserted_by = 'changed'
                      WHERE uid = ?";
    }
    let invalid = if invalid_new { &second } else { &first };
    if missing {
        store::sqlx::query("DELETE FROM record_revision WHERE record_uid = ?")
            .bind(invalid)
            .execute(&store.pool)
            .await
            .unwrap();
    } else {
        store::sqlx::query("UPDATE record_revision SET revision = ? WHERE record_uid = ?")
            .bind(i64::MAX)
            .bind(invalid)
            .execute(&store.pool)
            .await
            .unwrap();
    }
    let mut tx = store::write_tx(&store.pool).await.unwrap();
    store::sqlx::query("UPDATE record SET head = 'Prior statement retained' WHERE uid = ?")
        .bind(&marker)
        .execute(&mut *tx)
        .await
        .unwrap();
    let revisions_query = "SELECT record_uid, revision FROM record_revision ORDER BY record_uid";
    let records_query = "SELECT uid, head, body FROM record ORDER BY uid";
    let before_revisions = store::sqlx::query_as::<_, (String, i64)>(revisions_query)
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    let before_records = store::sqlx::query_as::<_, (String, String, String)>(records_query)
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    let before_rows = store::sqlx::query_scalar::<_, String>(row_query)
        .fetch_all(&mut *tx)
        .await
        .unwrap();
    let error = store::sqlx::query(move_query)
        .bind(&second)
        .bind(&row_uid)
        .execute(&mut *tx)
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Record revision is missing or exhausted"),
        "{error}"
    );
    assert_eq!(
        store::sqlx::query_as::<_, (String, i64)>(revisions_query)
            .fetch_all(&mut *tx)
            .await
            .unwrap(),
        before_revisions
    );
    assert_eq!(
        store::sqlx::query_as::<_, (String, String, String)>(records_query)
            .fetch_all(&mut *tx)
            .await
            .unwrap(),
        before_records
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, String>(row_query)
            .fetch_all(&mut *tx)
            .await
            .unwrap(),
        before_rows
    );
    tx.commit().await.unwrap();
    assert_eq!(
        store::sqlx::query_as::<_, (String, i64)>(revisions_query)
            .fetch_all(&store.pool)
            .await
            .unwrap(),
        before_revisions
    );
    assert_eq!(
        store::sqlx::query_as::<_, (String, String, String)>(records_query)
            .fetch_all(&store.pool)
            .await
            .unwrap(),
        before_records
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, String>(row_query)
            .fetch_all(&store.pool)
            .await
            .unwrap(),
        before_rows
    );
}

#[tokio::test]
async fn assertion_move_old_max_revision_aborts_entire_statement() {
    move_refuses_without_partial_changes(false, false, false).await;
}

#[tokio::test]
async fn assertion_move_old_missing_revision_aborts_entire_statement() {
    move_refuses_without_partial_changes(false, false, true).await;
}

#[tokio::test]
async fn assertion_move_new_max_revision_aborts_entire_statement() {
    move_refuses_without_partial_changes(false, true, false).await;
}

#[tokio::test]
async fn assertion_move_new_missing_revision_aborts_entire_statement() {
    move_refuses_without_partial_changes(false, true, true).await;
}

#[tokio::test]
async fn extension_move_old_max_revision_aborts_entire_statement() {
    move_refuses_without_partial_changes(true, false, false).await;
}

#[tokio::test]
async fn extension_move_old_missing_revision_aborts_entire_statement() {
    move_refuses_without_partial_changes(true, false, true).await;
}

#[tokio::test]
async fn extension_move_new_max_revision_aborts_entire_statement() {
    move_refuses_without_partial_changes(true, true, false).await;
}

#[tokio::test]
async fn extension_move_new_missing_revision_aborts_entire_statement() {
    move_refuses_without_partial_changes(true, true, true).await;
}
