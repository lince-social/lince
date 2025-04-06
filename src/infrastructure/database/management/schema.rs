use sqlx::sqlite;
use std::io::{Error, ErrorKind};

pub async fn schema() -> Result<(), Error> {
    let config_dir = dirs::config_dir().unwrap();
    let db_path = String::from(config_dir.to_str().unwrap()) + "/lince/lince.db";
    let opt = sqlite::SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true);
    let pool = sqlite::SqlitePool::connect_with(opt).await;
    if pool.is_err() {
        return Err(Error::new(ErrorKind::Other, "Pool error"));
    }
    let pool = pool.unwrap();

    let record = sqlx::query(
        "CREATE TABLE IF NOT EXISTS record(
             id INTEGER PRIMARY KEY,
             quantity REAL NOT NULL DEFAULT 1,
             head TEXT,
             body TEXT
         )",
    )
    .execute(&pool)
    .await;
    if record.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table record",
        ));
    }

    let dna = sqlx::query(
        "CREATE TABLE IF NOT EXISTS dna(
             id INTEGER PRIMARY KEY,
             quantity INTEGER NOT NULL DEFAULT 1,
             origin TEXT NOT NULL DEFAULT 'lince.db'
         )",
    )
    .execute(&pool)
    .await;
    if dna.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table dna",
        ));
    }

    let view = sqlx::query(
        "CREATE TABLE IF NOT EXISTS view(
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            query TEXT NOT NULL DEFAULT 'SELECT * FROM record'
        )",
    )
    .execute(&pool)
    .await;
    if view.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table view",
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
    .execute(&pool)
    .await;
    if configuration.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table configuration",
        ));
    }

    let _created_configuration = sqlx::query(
        "INSERT INTO configuration(name, quantity)
        SELECT ?1, ?2
        WHERE NOT EXISTS (SELECT * FROM configuration)",
    )
    .execute(&pool)
    .await;

    let configuration_view = sqlx::query(
        "CREATE TABLE IF NOT EXISTS configuration_view(
            configuration_id INTEGER REFERENCES configuration(id),
            view_id INTEGER REFERENCES view(id),
            quantity INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (configuration_id, view_id)
         )",
    )
    .execute(&pool)
    .await;
    if configuration_view.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table configuration_view",
        ));
    }

    let configuration_view = sqlx::query(
        "CREATE TABLE IF NOT EXISTS configuration_view(
            configuration_id INTEGER REFERENCES configuration(id),
            view_id INTEGER REFERENCES view(id),
            quantity INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (configuration_id, view_id)
         )",
    )
    .execute(&pool)
    .await;
    if configuration_view.is_err() {
        return Err(Error::new(
            ErrorKind::ConnectionAborted,
            "Error when creating table configuration_view",
        ));
    }

    // let karma_condition = conn.execute(
    //     "CREATE TABLE IF NOT EXISTS karma_condition(
    //             id INTEGER PRIMARY KEY,
    //             quantity INTEGER NOT NULL DEFAULT 0,
    //             condition TEXT NOT NULL
    //         )",
    //     [],
    // );
    // if karma_condition.is_err() {
    //     return Err(Error::new(
    //         ErrorKind::ConnectionAborted,
    //         "Error when creating table karma_condition",
    //     ));
    // }

    // let karma_consequence = conn.execute(
    //     "CREATE TABLE IF NOT EXISTS karma_consequence(
    //          id INTEGER PRIMARY KEY,
    //          quantity INTEGER NOT NULL DEFAULT 0,
    //          consequence TEXT NOT NULL
    //      )",
    //     [],
    // );
    // if karma_consequence.is_err() {
    //     return Err(Error::new(
    //         ErrorKind::ConnectionAborted,
    //         "Error when creating table karma_consequence",
    //     ));
    // }

    // let karma = conn.execute(
    //     "CREATE TABLE IF NOT EXISTS karma(
    //         id INTEGER PRIMARY KEY,
    //         quantity REAL NOT NULL DEFAULT 1,
    //         condition_id INTEGER NOT NULL,
    //         operator TEXT NOT NULL,
    //         consequence_id INTEGER NOT NULL
    //      )",
    //     [],
    // );
    // if karma.is_err() {
    //     return Err(Error::new(
    //         ErrorKind::ConnectionAborted,
    //         "Error when creating table karma",
    //     ));
    // }

    // let frequency = conn.execute(
    //     "CREATE TABLE IF NOT EXISTS frequency(
    //         id INTEGER PRIMARY KEY,
    //         quantity REAL NOT NULL DEFAULT 1,
    //         day_week REAL,
    //         months REAL DEFAULT 0 NOT NULL,
    //         days REAL DEFAULT 0 NOT NULL,
    //         seconds REAL DEFAULT 0 NOT NULL,
    //         next_date DATETIME NOT NULL,
    //         finish_date DATETIME,
    //         catch_up_sum INTEGER NOT NULL DEFAULT 0
    //      )",
    //     [],
    // );
    // if frequency.is_err() {
    //     return Err(Error::new(
    //         ErrorKind::ConnectionAborted,
    //         "Error when creating table frequency",
    //     ));
    // }

    // let command = conn.execute(
    //     "CREATE TABLE IF NOT EXISTS command(
    //          id INTEGER PRIMARY KEY,
    //          quantity REAL NOT NULL DEFAULT 1,
    //          command TEXT NOT NULL
    //      )",
    //     [],
    // );
    // if command.is_err() {
    //     return Err(Error::new(
    //         ErrorKind::ConnectionAborted,
    //         "Error when creating table command",
    //     ));
    // }

    // conn.execute(
    //     "CREATE TABLE IF NOT EXISTS transfer(
    //          id INTEGER PRIMARY KEY,
    //          quantity REAL NOT NULL DEFAULT 1,

    //      )",
    //     [],
    // )?;
    // conn.execute(
    //     "CREATE TABLE IF NOT EXISTS sum(
    //          id INTEGER PRIMARY KEY,
    //          quantity REAL NOT NULL DEFAULT 1,
    //      )",
    //     [],
    // )?;
    //    conn.execute(
    //        "CREATE TABLE IF NOT EXISTS history(
    //             id INTEGER PRIMARY KEY,
    // record_id INTEGER NOT NULL,
    // change_time TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    // old_quantity REAL NOT NULL,
    // new_quantity REAL NOT NULL
    //         )",
    //        [],
    //    )?;

    Ok(())
}
// use sqlx::{sqlite, Executor};

// #[tokio::main]
// pub async fn schema_database() {
//     let config_dir = dirs::config_dir().unwrap();
//     let db_path = String::from(config_dir.to_str().unwrap()) + "/lince/lince.db";
//     let opt = sqlite::SqliteConnectOptions::new()
//         .filename(db_path)
//         .create_if_missing(true);
//     let conn = sqlite::SqlitePool::connect_with(opt).await.unwrap();

//     conn.execute(
//         "CREATE TABLE IF NOT EXISTS record(
//                  id INTEGER PRIMARY KEY,
//                  quantity REAL NOT NULL DEFAULT 1,
//                  head TEXT,
//                  body TEXT
//              )",
//     )
//     .await
//     .expect("Failed to create record table");

//     conn.execute(
//         "CREATE TABLE IF NOT EXISTS dna(
//                  id INTEGER PRIMARY KEY,
//                  quantity INTEGER NOT NULL DEFAULT 1,
//                  origin TEXT NOT NULL DEFAULT 'lince.db'
//              )",
//     )
//     .await
//     .unwrap();

//     conn.execute(
//         "CREATE TABLE IF NOT EXISTS view(
//                 id INTEGER PRIMARY KEY,
//                 name TEXT NOT NULL,
//                 query TEXT NOT NULL DEFAULT 'SELECT * FROM record'
//              )",
//     )
//     .await
//     .unwrap();

//     conn.execute(
//         "CREATE TABLE IF NOT EXISTS configuration(
//                 id INTEGER PRIMARY KEY,
//                 quantity INTEGER,
//                 name TEXT NOT NULL,
//                 language TEXT,
//                 timezone INTEGER,
//                 style TEXT
//              )",
//     )
//     .await
//     .unwrap();

//     conn.execute(
//         "CREATE TABLE IF NOT EXISTS configuration_view(
//                 configuration_id INTEGER REFERENCES configuration(id),
//                 view_id INTEGER REFERENCES view(id),
//                 quantity INTEGER NOT NULL DEFAULT 0,
//                 PRIMARY KEY (configuration_id, view_id)
//              )",
//     )
//     .await
//     .unwrap();

//     conn.execute(
//         "CREATE TABLE IF NOT EXISTS karma_condition(
//                  id INTEGER PRIMARY KEY,
//                  quantity INTEGER NOT NULL DEFAULT 0,
//                  condition TEXT NOT NULL
//              )",
//     )
//     .await
//     .unwrap();

//     conn.execute(
//         "CREATE TABLE IF NOT EXISTS karma_consequence(
//                  id INTEGER PRIMARY KEY,
//                  quantity INTEGER NOT NULL DEFAULT 0,
//                  consequence TEXT NOT NULL
//              )",
//     )
//     .await
//     .unwrap();

//     conn.execute(
//         "CREATE TABLE IF NOT EXISTS karma(
//                 id INTEGER PRIMARY KEY,
//                 quantity REAL NOT NULL DEFAULT 1,
//                 condition_id INTEGER NOT NULL,
//                 operator TEXT NOT NULL,
//                 consequence_id INTEGER NOT NULL
//              )",
//     )
//     .await
//     .unwrap();

//     conn.execute(
//         "CREATE TABLE IF NOT EXISTS frequency(
//                 id INTEGER PRIMARY KEY,
//                 quantity REAL NOT NULL DEFAULT 1,
//                 day_week REAL,
//                 months REAL DEFAULT 0 NOT NULL,
//                 days REAL DEFAULT 0 NOT NULL,
//                 seconds REAL DEFAULT 0 NOT NULL,
//                 next_date DATETIME NOT NULL,
//                 finish_date DATETIME,
//                 catch_up_sum INTEGER NOT NULL DEFAULT 0
//              )",
//     )
//     .await
//     .unwrap();

//     conn.execute(
//         "CREATE TABLE IF NOT EXISTS command(
//                  id INTEGER PRIMARY KEY,
//                  quantity REAL NOT NULL DEFAULT 1,
//                  command TEXT NOT NULL
//              )",
//     )
//     .await
//     .unwrap();

//     conn.execute(
//         "CREATE TABLE IF NOT EXISTS transfer(
//                  id INTEGER PRIMARY KEY,
//                  quantity REAL NOT NULL DEFAULT 1,

//              )",
//     )
//     .await
//     .unwrap();

//     conn.execute(
//         "CREATE TABLE IF NOT EXISTS sum(
//                  id INTEGER PRIMARY KEY,
//                  quantity REAL NOT NULL DEFAULT 1,
//              )",
//     )
//     .await
//     .unwrap();

//     conn.execute(
//         "CREATE TABLE IF NOT EXISTS history(
//                     id INTEGER PRIMARY KEY,
//         record_id INTEGER NOT NULL,
//         change_time TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
//         old_quantity REAL NOT NULL,
//         new_quantity REAL NOT NULL
//                 )",
//     )
//     .await
//     .unwrap();
// }
