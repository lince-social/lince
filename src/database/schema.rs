use rusqlite::{Connection, Result};

pub fn schema_database() -> Result<()> {
    let conn = Connection::open("/home/eduardo/.config/lince/lince.db")?;

    conn.execute(
        "CREATE TABLE IF NOT EXISTS record(
             id INTEGER PRIMARY KEY,
             quantity REAL NOT NULL DEFAULT 1,
             head TEXT,
             body TEXT
         )",
        [],
    )?;
    // conn.execute(
    //     "CREATE TABLE IF NOT EXISTS dna(
    //          id INTEGER PRIMARY KEY,
    //          quantity INTEGER NOT NULL DEFAULT 1,
    //          origin TEXT NOT NULL DEFAULT 'lince.db'
    //      )",
    //     [],
    // )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS view(
            id INTEGER PRIMARY KEY,
            view_name TEXT NOT NULL,
            query TEXT NOT NULL DEFAULT 'SELECT * FROM record'
         )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS configuration(
            id INTEGER PRIMARY KEY,
            quantity INTEGER,
            configuration_name TEXT NOT NULL,
            language TEXT,
            timezone INTEGER,
            style TEXT
         )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS configuration_view(
            quantity INTEGER NOT NULL DEFAULT 0,
            configuration_id INTEGER REFERENCES configuration(id),
            view_id INTEGER REFERENCES view(id),
            PRIMARY KEY (configuration_id, view_id)
         )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS karma_condition(
             id INTEGER PRIMARY KEY,
             quantity INTEGER NOT NULL DEFAULT 0,
             condition TEXT NOT NULL
         )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS karma_consequence(
             id INTEGER PRIMARY KEY,
             quantity INTEGER NOT NULL DEFAULT 0,
             consequence TEXT NOT NULL
         )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS karma(
            id INTEGER PRIMARY KEY,
            quantity REAL NOT NULL DEFAULT 1,
            condition_id INTEGER NOT NULL,
            operator TEXT NOT NULL,
            consequence_id INTEGER NOT NULL
         )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS frequency(
            id INTEGER PRIMARY KEY,
            quantity REAL NOT NULL DEFAULT 1,
            day_week REAL,
            months REAL DEFAULT 0 NOT NULL,
            days REAL DEFAULT 0 NOT NULL,
            seconds REAL DEFAULT 0 NOT NULL,
            next_date DATETIME NOT NULL,
            finish_date DATETIME,
            catch_up_sum INTEGER NOT NULL DEFAULT 0
         )",
        [],
    )?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS command(
             id INTEGER PRIMARY KEY,
             quantity REAL NOT NULL DEFAULT 1,
             command TEXT NOT NULL
         )",
        [],
    )?;
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
