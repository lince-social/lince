use crate::{ok, query};
use sqlx::{Pool, Sqlite};
use std::{io::Error, sync::Arc};

pub async fn schema(db: Arc<Pool<Sqlite>>) -> Result<(), Error> {
    // === TABLES ===
    query!(
        "CREATE TABLE IF NOT EXISTS record(
            id INTEGER PRIMARY KEY,
            quantity REAL NOT NULL DEFAULT 1,
            head TEXT,
            body TEXT
        )",
        db
    );

    query!(
        "CREATE TABLE IF NOT EXISTS view(
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            query TEXT NOT NULL DEFAULT 'SELECT * FROM record'
        )",
        db
    );

    query!(
        "CREATE TABLE IF NOT EXISTS collection(
            id INTEGER PRIMARY KEY,
            quantity INTEGER,
            name TEXT NOT NULL
        )",
        db
    );

    query!(
        "CREATE TABLE IF NOT EXISTS configuration(
            id INTEGER PRIMARY KEY,
            quantity INTEGER,
            name TEXT NOT NULL,
            language TEXT,
            timezone INTEGER,
            style TEXT
        )",
        db
    );

    query!(
        "CREATE TABLE IF NOT EXISTS collection_view(
            id INTEGER PRIMARY KEY,
            quantity INTEGER NOT NULL DEFAULT 1,
            collection_id INTEGER REFERENCES collection(id),
            view_id INTEGER REFERENCES view(id)
        )",
        db
    );

    query!(
        "CREATE TABLE IF NOT EXISTS karma_condition(
            id INTEGER PRIMARY KEY,
            quantity INTEGER NOT NULL DEFAULT 1,
            name TEXT NOT NULL DEFAULT 'Condition',
            condition TEXT NOT NULL
        )",
        db
    );

    query!(
        "CREATE TABLE IF NOT EXISTS karma_consequence(
            id INTEGER PRIMARY KEY,
            quantity INTEGER NOT NULL DEFAULT 1,
            name TEXT NOT NULL DEFAULT 'Consequence',
            consequence TEXT NOT NULL
        )",
        db
    );

    query!(
        "CREATE TABLE IF NOT EXISTS karma(
            id INTEGER PRIMARY KEY,
            quantity INTEGER NOT NULL DEFAULT 1,
            name TEXT NOT NULL DEFAULT 'Karma',
            condition_id INTEGER NOT NULL,
            operator TEXT NOT NULL,
            consequence_id INTEGER NOT NULL
        )",
        db
    );

    query!(
        "CREATE TABLE IF NOT EXISTS frequency(
            id INTEGER PRIMARY KEY,
            quantity REAL NOT NULL DEFAULT 1,
            name TEXT NOT NULL DEFAULT 'Frequency',
            day_week REAL,
            months REAL DEFAULT 0 NOT NULL,
            days REAL DEFAULT 0 NOT NULL,
            seconds REAL DEFAULT 0 NOT NULL,
            next_date TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL,
            finish_date DATETIME,
            catch_up_sum INTEGER NOT NULL DEFAULT 0
        )",
        db
    );

    query!(
        "CREATE TABLE IF NOT EXISTS command(
            id INTEGER PRIMARY KEY,
            quantity REAL NOT NULL DEFAULT 1,
            name TEXT NOT NULL DEFAULT 'Command',
            command TEXT NOT NULL
        )",
        db
    );

    query!(
        "CREATE TABLE IF NOT EXISTS transfer(
            id INTEGER PRIMARY KEY,
            quantity REAL NOT NULL DEFAULT 1
        )",
        db
    );

    query!(
        "CREATE TABLE IF NOT EXISTS sum(
            id INTEGER PRIMARY KEY,
            quantity REAL NOT NULL DEFAULT 1
        )",
        db
    );

    query!(
        "CREATE TABLE IF NOT EXISTS history(
            id INTEGER PRIMARY KEY,
            record_id INTEGER NOT NULL,
            change_time TEXT DEFAULT CURRENT_TIMESTAMP,
            old_quantity REAL NOT NULL,
            new_quantity REAL NOT NULL
        )",
        db
    );

    query!(
        "CREATE TABLE IF NOT EXISTS dna(
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            origin TEXT NOT NULL,
            quantity INTEGER NOT NULL DEFAULT 0
        )",
        db
    );

    query!(
        "CREATE TABLE IF NOT EXISTS query(
            id INTEGER PRIMARY KEY,
            name TEXT,
            query TEXT NOT NULL
        )",
        db
    );

    // === DEFAULT RECORDS ===
    if ok!(sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM record")
        .fetch_one(&*db)
        .await)
        == 0
    {
        query!(
            "INSERT INTO record(quantity, head, body)
                VALUES (1, 'Welcome', 'This is your first record')",
            db
        );
        query!(
            "INSERT INTO record(quantity, head, body)
                VALUES (-1, 'a negative one', 'This record has negative quantity')",
            db
        );
    }

    // === DEFAULT CONFIGURATION ===
    let configuration_with_style = ok!(sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(1) FROM configuration WHERE style IS NOT NULL AND style <> ''"
    )
    .fetch_one(&*db)
    .await);

    if configuration_with_style == 0 {
        query!(
            "INSERT INTO configuration(quantity, name, language, timezone, style)
                VALUES (1, 'Default', 'en', 0, 'catppuccin_macchiato')",
            db
        );
    }

    // === DEFAULT COLLECTION + VIEWS ===
    if ok!(sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(1) FROM collection_view cv
         JOIN collection c ON cv.collection_id = c.id
         JOIN view v ON cv.view_id = v.id"
    )
    .fetch_one(&*db)
    .await)
        == 0
    {
        // default collection
        query!(
            "INSERT INTO collection(quantity, name) VALUES (1, 'Default Collection')",
            db
        );

        query!(
            "INSERT INTO view(name, query) VALUES ('Record', 'SELECT * FROM record')",
            db
        );
        query!(
            "INSERT INTO collection_view(quantity, collection_id, view_id) VALUES (1, 1, (SELECT id FROM view WHERE name='Record'))",
            db
        );

        query!(
            "INSERT INTO view(name, query) VALUES ('Configuration', 'SELECT * FROM configuration')",
            db
        );
        query!(
            "INSERT INTO collection_view(quantity, collection_id, view_id) VALUES (1, 1, (SELECT id FROM view WHERE name='Configuration'))",
            db
        );

        query!(
            "INSERT INTO view(name, query) VALUES ('Collection', 'SELECT * FROM collection')",
            db
        );
        query!(
            "INSERT INTO collection_view(quantity, collection_id, view_id) VALUES (1, 1, (SELECT id FROM view WHERE name='Collection'))",
            db
        );

        query!(
            "INSERT INTO view(name, query) VALUES ('Collection_View', 'SELECT * FROM collection_view')",
            db
        );
        query!(
            "INSERT INTO collection_view(quantity, collection_id, view_id) VALUES (1, 1, (SELECT id FROM view WHERE name='Collection_View'))",
            db
        );

        query!(
            "INSERT INTO view(name, query) VALUES ('View', 'SELECT * FROM view')",
            db
        );
        query!(
            "INSERT INTO collection_view(quantity, collection_id, view_id) VALUES (1, 1, (SELECT id FROM view WHERE name='View'))",
            db
        );

        query!(
            "INSERT INTO view(name, query) VALUES ('Negative Records', 'SELECT * FROM record WHERE quantity < 0')",
            db
        );
        query!(
            "INSERT INTO collection(quantity, name) VALUES (0, 'Negative Records')",
            db
        );
        query!(
            "INSERT INTO collection_view(quantity, collection_id, view_id) VALUES (1, 2, (SELECT id FROM view WHERE name='Negative Records'))",
            db
        );
        query!(
            "INSERT INTO view(name, query) VALUES ('Command', 'SELECT * FROM command')",
            db
        );
        query!(
            "INSERT INTO collection_view(quantity, collection_id, view_id) VALUES (1, 1, (SELECT id FROM view WHERE name='Command'))",
            db
        );
        query!(
            "INSERT INTO view(name, query) VALUES ('Frequency', 'SELECT * FROM frequency')",
            db
        );
        query!(
            "INSERT INTO collection_view(quantity, collection_id, view_id) VALUES (1, 1, (SELECT id FROM view WHERE name='Frequency'))",
            db
        );
    }

    Ok(())
}
