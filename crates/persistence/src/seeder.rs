use sqlx::{Pool, Sqlite};
use std::io::Error;

/// Idempotent database seeder.
///
/// This will insert default rows only when they don't already exist.
/// It's safe to run multiple times after migrations have been applied.
///
/// Usage:
/// - call `persistence::seeder::seed(&*db).await?` after running migrations.
///
/// This implementation avoids using workspace macros so it doesn't require
/// compile-time query macros or macro imports; all queries are executed at runtime.
pub async fn seed(db: &Pool<Sqlite>) -> Result<(), Error> {
    // === DEFAULT RECORDS ===
    let record_count: i64 = sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM record")
        .fetch_one(&*db)
        .await
        .map_err(Error::other)?;
    if record_count == 0 {
        sqlx::query(
            "INSERT INTO record(quantity, head, body)
                VALUES (1, 'Welcome', 'This is your first record')",
        )
        .execute(&*db)
        .await
        .map_err(Error::other)?;
        sqlx::query(
            "INSERT INTO record(quantity, head, body)
                VALUES (-1, 'a negative one', 'This record has negative quantity')",
        )
        .execute(&*db)
        .await
        .map_err(Error::other)?;
    }

    // === DEFAULT CONFIGURATION ===
    let configuration_with_style: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(1) FROM configuration WHERE style IS NOT NULL AND style <> ''",
    )
    .fetch_one(&*db)
    .await
    .map_err(Error::other)?;

    if configuration_with_style == 0 {
        sqlx::query(
            "INSERT INTO configuration(quantity, name, language, timezone, style)
                VALUES (1, 'Default', 'en', 0, 'catppuccin_macchiato')",
        )
        .execute(&*db)
        .await
        .map_err(Error::other)?;
    }

    // === DEFAULT COLLECTION + VIEWS ===
    let collection_views_count: i64 = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(1) FROM collection_view cv
         JOIN collection c ON cv.collection_id = c.id
         JOIN view v ON cv.view_id = v.id",
    )
    .fetch_one(&*db)
    .await
    .map_err(Error::other)?;

    if collection_views_count == 0 {
        // default collection
        sqlx::query("INSERT INTO collection(quantity, name) VALUES (1, 'Default Collection')")
            .execute(&*db)
            .await
            .map_err(Error::other)?;

        sqlx::query("INSERT INTO view(name, query) VALUES ('Record', 'SELECT * FROM record')")
            .execute(&*db)
            .await
            .map_err(Error::other)?;
        sqlx::query("INSERT INTO collection_view(quantity, collection_id, view_id) VALUES (1, 1, (SELECT id FROM view WHERE name='Record'))")
            .execute(&*db)
            .await
            .map_err(Error::other)?;

        sqlx::query(
            "INSERT INTO view(name, query) VALUES ('Configuration', 'SELECT * FROM configuration')",
        )
        .execute(&*db)
        .await
        .map_err(Error::other)?;
        sqlx::query("INSERT INTO collection_view(quantity, collection_id, view_id) VALUES (1, 1, (SELECT id FROM view WHERE name='Configuration'))")
            .execute(&*db)
            .await
            .map_err(Error::other)?;

        sqlx::query(
            "INSERT INTO view(name, query) VALUES ('Collection', 'SELECT * FROM collection')",
        )
        .execute(&*db)
        .await
        .map_err(Error::other)?;
        sqlx::query("INSERT INTO collection_view(quantity, collection_id, view_id) VALUES (1, 1, (SELECT id FROM view WHERE name='Collection'))")
            .execute(&*db)
            .await
            .map_err(Error::other)?;

        sqlx::query("INSERT INTO view(name, query) VALUES ('Collection_View', 'SELECT * FROM collection_view')")
            .execute(&*db)
            .await
            .map_err(Error::other)?;
        sqlx::query("INSERT INTO collection_view(quantity, collection_id, view_id) VALUES (1, 1, (SELECT id FROM view WHERE name='Collection_View'))")
            .execute(&*db)
            .await
            .map_err(Error::other)?;

        sqlx::query("INSERT INTO view(name, query) VALUES ('View', 'SELECT * FROM view')")
            .execute(&*db)
            .await
            .map_err(Error::other)?;
        sqlx::query("INSERT INTO collection_view(quantity, collection_id, view_id) VALUES (1, 1, (SELECT id FROM view WHERE name='View'))")
            .execute(&*db)
            .await
            .map_err(Error::other)?;

        sqlx::query("INSERT INTO view(name, query) VALUES ('Negative Records', 'SELECT * FROM record WHERE quantity < 0')")
            .execute(&*db)
            .await
            .map_err(Error::other)?;
        sqlx::query("INSERT INTO collection(quantity, name) VALUES (0, 'Negative Records')")
            .execute(&*db)
            .await
            .map_err(Error::other)?;
        sqlx::query("INSERT INTO collection_view(quantity, collection_id, view_id) VALUES (1, 2, (SELECT id FROM view WHERE name='Negative Records'))")
            .execute(&*db)
            .await
            .map_err(Error::other)?;

        sqlx::query("INSERT INTO view(name, query) VALUES ('Command', 'SELECT * FROM command')")
            .execute(&*db)
            .await
            .map_err(Error::other)?;
        sqlx::query("INSERT INTO collection_view(quantity, collection_id, view_id) VALUES (1, 1, (SELECT id FROM view WHERE name='Command'))")
            .execute(&*db)
            .await
            .map_err(Error::other)?;

        sqlx::query(
            "INSERT INTO view(name, query) VALUES ('Frequency', 'SELECT * FROM frequency')",
        )
        .execute(&*db)
        .await
        .map_err(Error::other)?;
        sqlx::query("INSERT INTO collection_view(quantity, collection_id, view_id) VALUES (1, 1, (SELECT id FROM view WHERE name='Frequency'))")
            .execute(&*db)
            .await
            .map_err(Error::other)?;
    }

    Ok(())
}
