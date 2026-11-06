use std::collections::BTreeMap;
use store::sqlx::Row;

async fn snapshot(pool: &store::sqlx::SqlitePool) -> BTreeMap<String, Vec<Vec<String>>> {
    let tables: Vec<String> = store::sqlx::query_scalar("SELECT name FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%' AND name NOT LIKE '_sqlx_%' ORDER BY name")
        .fetch_all(pool).await.unwrap();
    let mut state = BTreeMap::new();
    for table in tables {
        let mut columns: Vec<String> =
            store::sqlx::query_scalar("SELECT name FROM pragma_table_info(?)")
                .bind(&table)
                .fetch_all(pool)
                .await
                .unwrap();
        columns.sort();
        let expressions = columns
            .iter()
            .map(|name| {
                let name = name.replace('"', "\"\"");
                format!("typeof(\"{name}\") || ':' || hex(CAST(\"{name}\" AS BLOB))")
            })
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT {expressions} FROM \"{}\"",
            table.replace('"', "\"\"")
        );
        let rows = store::sqlx::query(&sql).fetch_all(pool).await.unwrap();
        let mut values: Vec<Vec<String>> = rows
            .iter()
            .map(|row| (0..columns.len()).map(|index| row.get(index)).collect())
            .collect();
        values.sort();
        state.insert(table, values);
    }
    state
}

async fn body_is_last(pool: &store::sqlx::SqlitePool) -> bool {
    store::sqlx::query_scalar::<_, String>(
        "SELECT name FROM pragma_table_info('record') ORDER BY cid DESC LIMIT 1",
    )
    .fetch_one(pool)
    .await
    .unwrap()
        == "body"
}

#[tokio::test]
async fn migration_preserves_every_table_and_rolls_back_without_fake_edits() {
    let pool = store::sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    store::sqlx::query("PRAGMA foreign_keys=ON")
        .execute(&pool)
        .await
        .unwrap();
    let mut previous = store::sqlx::migrate!("./migrations");
    previous.migrations = std::borrow::Cow::Owned(
        previous
            .iter()
            .filter(|migration| migration.version < 44)
            .cloned()
            .collect(),
    );
    previous.run(&pool).await.unwrap();
    store::linguas::ensure_local(&pool).await.unwrap();
    store::organs::ensure_local(&pool, "").await.unwrap();
    store::projection::install(&pool).await.unwrap();
    let store = store::Store::from_pool(pool);
    let mut ids = Vec::new();
    for (index, body) in [
        String::new(),
        "café 👩‍💻\n\0retained".into(),
        "large café 👩‍💻\n".repeat(65536),
    ]
    .iter()
    .enumerate()
    {
        let record = store::records::create(
            &store.pool,
            store::records::NewRecord {
                slug: None,
                kind: if index == 1 {
                    nucleus::RecordKind::Person
                } else {
                    nucleus::RecordKind::Plain
                },
                head: "Preserved title",
                body,
                quantity: nucleus::DecimalValue::parse_inferred("9007199254740993.125").unwrap(),
            },
        )
        .await
        .unwrap();
        store::records::set_text(&store.pool, &record.uid, Some("Updated title"), None)
            .await
            .unwrap();
        if index == 2 {
            store::sqlx::query("UPDATE record SET deleted_at='2026-11-06' WHERE uid=?")
                .bind(&record.uid)
                .execute(&store.pool)
                .await
                .unwrap();
        }
        ids.push((record.uid, body.clone()));
    }
    assert!(!body_is_last(&store.pool).await);
    let before = snapshot(&store.pool).await;
    let identities: Vec<(i64, String)> =
        store::sqlx::query_as("SELECT rowid,uid FROM record ORDER BY rowid")
            .fetch_all(&store.pool)
            .await
            .unwrap();
    let migration = include_str!("../migrations/0044_record_body_last.sql");
    let mut tx = store.pool.begin().await.unwrap();
    store::sqlx::raw_sql(migration)
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert!(!body_is_last(&store.pool).await);
    assert_eq!(snapshot(&store.pool).await, before);
    store::sqlx::migrate!("./migrations")
        .run(&store.pool)
        .await
        .unwrap();
    assert!(body_is_last(&store.pool).await);
    assert_eq!(snapshot(&store.pool).await, before);
    store::projection::install(&store.pool).await.unwrap();
    assert_eq!(
        store::sqlx::query_as::<_, (i64, String)>("SELECT rowid,uid FROM record ORDER BY rowid")
            .fetch_all(&store.pool)
            .await
            .unwrap(),
        identities
    );
    for (uid, body) in &ids {
        assert_eq!(
            store::sqlx::query_scalar::<_, String>("SELECT body FROM record WHERE uid=?")
                .bind(uid)
                .fetch_one(&store.pool)
                .await
                .unwrap(),
            *body
        );
    }
    assert!(
        store::sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&store.pool)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        store::sqlx::query_scalar::<_, String>("PRAGMA integrity_check")
            .fetch_one(&store.pool)
            .await
            .unwrap(),
        "ok"
    );
    store::records::set_text(&store.pool, &ids[0].0, None, Some("Next description"))
        .await
        .unwrap();
    let revision: i64 =
        store::sqlx::query_scalar("SELECT revision FROM record_revision WHERE record_uid=?")
            .bind(&ids[0].0)
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_eq!(revision, 3);
    let body: String = store::sqlx::query_scalar("SELECT json_extract(content,'$.body') FROM record_content_version WHERE record_uid=? ORDER BY revision DESC LIMIT 1").bind(&ids[0].0).fetch_one(&store.pool).await.unwrap();
    assert_eq!(body, "Next description");
    let created = store::records::create(
        &store.pool,
        store::records::NewRecord {
            slug: None,
            kind: nucleus::RecordKind::Plain,
            head: "New record",
            body: "New body",
            quantity: store::exact::zero(),
        },
    )
    .await
    .unwrap();
    let versions: i64 =
        store::sqlx::query_scalar("SELECT count(*) FROM record_content_version WHERE record_uid=?")
            .bind(&created.uid)
            .fetch_one(&store.pool)
            .await
            .unwrap();
    assert_eq!(versions, 1);
}
