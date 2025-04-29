use super::lib::connection;

pub async fn execute_migration() {
    let pool = connection().await.unwrap();
    let dna = sqlx::query("PRAGMA foreign_keys = OFF;
        -- record
        CREATE TABLE new_record (
            id INTEGER PRIMARY KEY,
            quantity REAL NOT NULL DEFAULT 1,
            head TEXT,
            body TEXT
        );
        INSERT INTO new_record (id, quantity, head, body)
        SELECT id, quantity, head, body FROM record;
        DROP TABLE record;
        ALTER TABLE new_record RENAME TO record;

        -- view
        DROP TABLE IF EXISTS new_view;
        CREATE TABLE new_view (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL DEFAULT 'View',
            query TEXT NOT NULL DEFAULT 'SELECT * FROM record'
        );
        INSERT INTO new_view (id, name, query)
        SELECT id, name, query FROM view;
        DROP TABLE view;
        ALTER TABLE new_view RENAME TO view;

        -- configuration
        CREATE TABLE new_configuration (
            id INTEGER PRIMARY KEY,
            quantity INTEGER,
            name TEXT NOT NULL,
            language TEXT,
            timezone INTEGER,
            style TEXT
        );
        INSERT INTO new_configuration (id, quantity, name, language, timezone, style)
        SELECT id, quantity, name, language, timezone, style FROM configuration;
        DROP TABLE configuration;
        ALTER TABLE new_configuration RENAME TO configuration;

        -- configuration_view
        CREATE TABLE new_configuration_view (
            id INTEGER PRIMARY KEY,
            quantity INTEGER NOT NULL DEFAULT 1,
            configuration_id INTEGER REFERENCES configuration(id),
            view_id INTEGER REFERENCES view(id)
        );
        INSERT INTO new_configuration_view (id, quantity, configuration_id, view_id)
        SELECT id, quantity, configuration_id, view_id FROM configuration_view;
        DROP TABLE configuration_view;
        ALTER TABLE new_configuration_view RENAME TO configuration_view;

        -- karma_condition
        DROP TABLE IF EXISTS new_karma_condition;
        CREATE TABLE new_karma_condition (
            id INTEGER PRIMARY KEY,
            quantity INTEGER NOT NULL DEFAULT 1,
            name TEXT,
            condition TEXT NOT NULL
        );
        INSERT INTO new_karma_condition (id, quantity, condition)
        SELECT id, quantity, condition FROM karma_condition;
        DROP TABLE karma_condition;
        ALTER TABLE new_karma_condition RENAME TO karma_condition;

        -- karma_consequence
        CREATE TABLE new_karma_consequence (
            id INTEGER PRIMARY KEY,
            quantity INTEGER NOT NULL DEFAULT 1,
            name TEXT,
            consequence TEXT NOT NULL
        );
        INSERT INTO new_karma_consequence (id, quantity, consequence)
        SELECT id, quantity, consequence FROM karma_consequence;
        DROP TABLE karma_consequence;
        ALTER TABLE new_karma_consequence RENAME TO karma_consequence;

        -- karma
        DROP TABLE IF EXISTS new_karma;
        CREATE TABLE new_karma (
            id INTEGER PRIMARY KEY,
            quantity INTEGER NOT NULL DEFAULT 1,
            name TEXT NOT NULL DEFAULT 'Karma',
            condition_id INTEGER NOT NULL,
            operator TEXT NOT NULL,
            consequence_id INTEGER NOT NULL
        );
        INSERT INTO new_karma (id, quantity, condition_id, operator, consequence_id)
        SELECT id, quantity, condition_id, operator, consequence_id FROM karma;
        DROP TABLE karma;
        ALTER TABLE new_karma RENAME TO karma;

        -- frequency
        DROP TABLE IF EXISTS new_frequency;
        CREATE TABLE new_frequency (
            id INTEGER PRIMARY KEY,
            quantity REAL NOT NULL DEFAULT 1,
            name TEXT,
            day_week REAL,
            months REAL DEFAULT 0 NOT NULL,
            days REAL DEFAULT 0 NOT NULL,
            seconds REAL DEFAULT 0 NOT NULL,
            next_date TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL,
            finish_date DATETIME,
            catch_up_sum INTEGER NOT NULL DEFAULT 0
        );
        INSERT INTO new_frequency (
            id, quantity, day_week, months, days, seconds, next_date, finish_date, catch_up_sum
        )
        SELECT id, quantity, day_week, months, days, seconds, next_date, finish_date, catch_up_sum FROM frequency;
        DROP TABLE frequency;
        ALTER TABLE new_frequency RENAME TO frequency;

        -- command
        DROP TABLE IF EXISTS new_command;
        CREATE TABLE new_command (
            id INTEGER PRIMARY KEY,
            quantity REAL NOT NULL DEFAULT 1,
            name TEXT,
            command TEXT NOT NULL
        );
        INSERT INTO new_command (id, quantity, command)
        SELECT id, quantity, command FROM command;
        DROP TABLE command;
        ALTER TABLE new_command RENAME TO command;

        -- transfer
        CREATE TABLE new_transfer (
            id INTEGER PRIMARY KEY,
            quantity REAL NOT NULL DEFAULT 1
        );
        INSERT INTO new_transfer (id, quantity)
        SELECT id, quantity FROM transfer;
        DROP TABLE transfer;
        ALTER TABLE new_transfer RENAME TO transfer;

        -- sum
        CREATE TABLE new_sum (
            id INTEGER PRIMARY KEY,
            quantity REAL NOT NULL DEFAULT 1
        );
        INSERT INTO new_sum (id, quantity)
        SELECT id, quantity FROM sum;
        DROP TABLE sum;
        ALTER TABLE new_sum RENAME TO sum;

        -- history
        CREATE TABLE new_history (
            id INTEGER PRIMARY KEY,
            record_id INTEGER NOT NULL,
            change_time TEXT DEFAULT CURRENT_TIMESTAMP,
            old_quantity REAL NOT NULL,
            new_quantity REAL NOT NULL
        );
        INSERT INTO new_history (id, record_id, change_time, old_quantity, new_quantity)
        SELECT id, record_id, change_time, old_quantity, new_quantity FROM history;
        DROP TABLE history;
        ALTER TABLE new_history RENAME TO history;

        -- dna
        DROP TABLE IF EXISTS new_dna;
        CREATE TABLE new_dna (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            origin TEXT NOT NULL,
            quantity INTEGER NOT NULL DEFAULT 0
        );
        INSERT INTO new_dna (id, origin, quantity)
        SELECT id, origin, quantity FROM dna;
        DROP TABLE dna;
        ALTER TABLE new_dna RENAME TO dna;

        PRAGMA foreign_keys = ON;
        ",)
    .execute(&pool)
    .await;
    if dna.is_err() {
        println!("{}", dna.unwrap_err());
    }
}
