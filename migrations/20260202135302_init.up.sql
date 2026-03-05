-- === TABLES ===

CREATE TABLE record (
    id INTEGER PRIMARY KEY,
    quantity REAL NOT NULL DEFAULT 1,
    head TEXT,
    body TEXT
);

CREATE TABLE view (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    query TEXT NOT NULL DEFAULT 'SELECT * FROM record'
);

CREATE TABLE collection (
    id INTEGER PRIMARY KEY,
    quantity INTEGER,
    name TEXT NOT NULL
);

CREATE TABLE configuration (
    id INTEGER PRIMARY KEY,
    quantity INTEGER,
    name TEXT NOT NULL,
    language TEXT,
    timezone INTEGER,
    style TEXT
);

CREATE TABLE collection_view (
    id INTEGER PRIMARY KEY,
    quantity INTEGER NOT NULL DEFAULT 1,
    collection_id INTEGER REFERENCES collection(id),
    view_id INTEGER REFERENCES view(id)
);

CREATE TABLE karma_condition (
    id INTEGER PRIMARY KEY,
    quantity INTEGER NOT NULL DEFAULT 1,
    name TEXT NOT NULL DEFAULT 'Condition',
    condition TEXT NOT NULL
);

CREATE TABLE karma_consequence (
    id INTEGER PRIMARY KEY,
    quantity INTEGER NOT NULL DEFAULT 1,
    name TEXT NOT NULL DEFAULT 'Consequence',
    consequence TEXT NOT NULL
);

CREATE TABLE karma (
    id INTEGER PRIMARY KEY,
    quantity INTEGER NOT NULL DEFAULT 1,
    name TEXT NOT NULL DEFAULT 'Karma',
    condition_id INTEGER NOT NULL,
    operator TEXT NOT NULL,
    consequence_id INTEGER NOT NULL
);

CREATE TABLE frequency (
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
);

CREATE TABLE command (
    id INTEGER PRIMARY KEY,
    quantity REAL NOT NULL DEFAULT 1,
    name TEXT NOT NULL DEFAULT 'Command',
    command TEXT NOT NULL
);

CREATE TABLE transfer (
    id INTEGER PRIMARY KEY,
    quantity REAL NOT NULL DEFAULT 1
);

CREATE TABLE sum (
    id INTEGER PRIMARY KEY,
    quantity REAL NOT NULL DEFAULT 1
);

CREATE TABLE history (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL,
    change_time TEXT DEFAULT CURRENT_TIMESTAMP,
    old_quantity REAL NOT NULL,
    new_quantity REAL NOT NULL
);

CREATE TABLE dna (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    origin TEXT NOT NULL,
    quantity INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE query (
    id INTEGER PRIMARY KEY,
    name TEXT,
    query TEXT NOT NULL
);
