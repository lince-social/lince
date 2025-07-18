use sqlx::{Pool, Sqlite};
use std::{
    io::{Error, ErrorKind},
    sync::Arc,
};

pub async fn schema(db: Arc<Pool<Sqlite>>) -> Result<(), Error> {
    let record = sqlx::query(
        "CREATE TABLE IF NOT EXISTS record(
             id INTEGER PRIMARY KEY,
             quantity REAL NOT NULL DEFAULT 1,
             head TEXT,
             body TEXT
         )",
    )
    .execute(&*db)
    .await;
    if record.is_err() {
        println!("{}", record.unwrap_err());
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table record",
        ));
    }

    let view = sqlx::query(
        "CREATE TABLE IF NOT EXISTS view(
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            query TEXT NOT NULL DEFAULT 'SELECT * FROM record'
        )",
    )
    .execute(&*db)
    .await;
    if view.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table view",
        ));
    }

    let collection = sqlx::query(
        "CREATE TABLE IF NOT EXISTS collection(
            id INTEGER PRIMARY KEY,
            quantity INTEGER,
            name TEXT NOT NULL
        )",
    )
    .execute(&*db)
    .await;
    if collection.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table collection",
        ));
    }

    let configuration = sqlx::query(
        "CREATE TABLE IF NOT EXISTS configuration(
            id INTEGER PRIMARY KEY,
            quantity INTEGER,
            name TEXT NOT NULL,
            language TEXT,
            timezone INTEGER,
            style TEXT
        )",
    )
    .execute(&*db)
    .await;
    if configuration.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table configuration",
        ));
    }

    let collection_view = sqlx::query(
        "CREATE TABLE IF NOT EXISTS collection_view(
                id INTEGER PRIMARY KEY,
                quantity INTEGER NOT NULL DEFAULT 1,
                collection_id INTEGER REFERENCES collection(id),
                view_id INTEGER REFERENCES view(id)
         )",
    )
    .execute(&*db)
    .await;
    if collection_view.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table collection_view",
        ));
    }

    let karma_condition = sqlx::query(
        "CREATE TABLE IF NOT EXISTS karma_condition(
                id INTEGER PRIMARY KEY,
                quantity INTEGER NOT NULL DEFAULT 1,
                name TEXT NOT NULL DEFAULT 'Condition',
                condition TEXT NOT NULL
            )",
    )
    .execute(&*db)
    .await;
    if karma_condition.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table karma_condition",
        ));
    }

    let karma_consequence = sqlx::query(
        "CREATE TABLE IF NOT EXISTS karma_consequence(
             id INTEGER PRIMARY KEY,
             quantity INTEGER NOT NULL DEFAULT 1,
             name TEXT NOT NULL DEFAULT 'Consequence',
             consequence TEXT NOT NULL
         )",
    )
    .execute(&*db)
    .await;
    if karma_consequence.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table karma_consequence",
        ));
    }

    let karma = sqlx::query(
        "CREATE TABLE IF NOT EXISTS karma(
            id INTEGER PRIMARY KEY,
            quantity INTEGER NOT NULL DEFAULT 1,
            name TEXT NOT NULL DEFAULT 'Karma',
            condition_id INTEGER NOT NULL,
            operator TEXT NOT NULL,
            consequence_id INTEGER NOT NULL
         )",
    )
    .execute(&*db)
    .await;
    if karma.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table karma",
        ));
    }

    let frequency = sqlx::query(
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
    )
    .execute(&*db)
    .await;
    if frequency.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table frequency",
        ));
    }

    let command = sqlx::query(
        "CREATE TABLE IF NOT EXISTS command(
             id INTEGER PRIMARY KEY,
             quantity REAL NOT NULL DEFAULT 1,
             name TEXT NOT NULL DEFAULT 'Command',
             command TEXT NOT NULL
         )",
    )
    .execute(&*db)
    .await;
    if command.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table command",
        ));
    }

    let transfer = sqlx::query(
        "CREATE TABLE IF NOT EXISTS transfer(
             id INTEGER PRIMARY KEY,
             quantity REAL NOT NULL DEFAULT 1

         )",
    )
    .execute(&*db)
    .await;
    if transfer.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table transfer",
        ));
    }

    let sum = sqlx::query(
        "CREATE TABLE IF NOT EXISTS sum(
             id INTEGER PRIMARY KEY,
             quantity REAL NOT NULL DEFAULT 1
         )",
    )
    .execute(&*db)
    .await;
    if sum.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table sum",
        ));
    }

    let history = sqlx::query(
        "CREATE TABLE IF NOT EXISTS history(
                id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL,
    change_time TEXT DEFAULT CURRENT_TIMESTAMP,
    old_quantity REAL NOT NULL,
    new_quantity REAL NOT NULL
            )",
    )
    .execute(&*db)
    .await;
    if history.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            format!(
                "Error when creating table history: {}",
                history.unwrap_err()
            ),
        ));
    }

    let dna = sqlx::query(
        "CREATE TABLE IF NOT EXISTS dna (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                origin TEXT NOT NULL,
                quantity INTEGER NOT NULL DEFAULT 0
            )",
    )
    .execute(&*db)
    .await;
    if dna.is_err() {
        println!("{}", dna.unwrap_err());
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table dna",
        ));
    }

    let query = sqlx::query(
        "CREATE TABLE IF NOT EXISTS query (
                id INTEGER PRIMARY KEY,
                name TEXT,
                query TEXT NOT NULL
            )",
    )
    .execute(&*db)
    .await;
    if query.is_err() {
        println!("{}", query.unwrap_err());
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table query",
        ));
    }

    Ok(())
}
