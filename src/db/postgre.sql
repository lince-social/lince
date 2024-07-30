CREATE TABLE configuration (
	id SERIAL PRIMARY KEY,
	quantity REAL NOT NULL DEFAULT 1,
	save_mode VARCHAR(9) NOT NULL DEFAULT 'Automatic' CHECK (save_mode in ('Automatic', 'Manual')),
 	view TEXT NOT NULL DEFAULT 'SELECT * FROM record WHERE quantity < 0 ORDER BY quantity ASC, text ASC, id ASC',
	column_information_mode VARCHAR(7) NOT NULL DEFAULT 'verbose' CHECK (column_information_mode in ('verbose', 'short', 'silent')),
	keymap jsonb NOT NULL DEFAULT '{}',
	truncation jsonb NOT NULL DEFAULT '{"text": 150, "view": 100}',
	table_query jsonb NOT NULL DEFAULT '{"record": "SELECT * FROM RECORD ORDER BY quantity ASC, text ASC, id ASC", "frequency": "SELECT * FROM frequency ORDER BY record_id ASC"}'
);

INSERT INTO configuration (save_mode) VALUES ('Automatic') ON CONFLICT (id) DO NOTHING;

CREATE TABLE record (
	id SERIAL PRIMARY KEY,
	quantity REAL NOT NULL DEFAULT 1,
	text TEXT,
	location POINT -- example DEFAULT '(59.880220, -43.732561)'
);

CREATE TABLE history (
	id SERIAL PRIMARY KEY,
	record_quantity REAL NOT NULL DEFAULT 1,
	record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE
);

CREATE TABLE karma (
	id SERIAL PRIMARY KEY,
	quantity INTEGER NOT NULL DEFAULT 1,
	karma TEXT
);

CREATE TABLE frequency (
	id SERIAL PRIMARY KEY,
	quantity INTEGER NOT NULL DEFAULT 1,
	day_week INTEGER,
	months REAL DEFAULT 0 NOT NULL,
	days REAL DEFAULT 0 NOT NULL,
	seconds REAL DEFAULT 0 NOT NULL,
	next_date TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
	finish_date DATE
);

CREATE TABLE command (
	id SERIAL PRIMARY KEY,
	quantity INTEGER NOT NULL DEFAULT 1,
	command TEXT NOT NULL
);

CREATE TABLE transfer (
	id SERIAL PRIMARY KEY,

	records_received json,
	records_contributed json,

	receiving_agreement BOOL,
	contributing_agreement BOOL,
	agreement_time TIMESTAMP WITH TIME ZONE,

	receiving_transfer_confirmation BOOL,
	contributing_transfer_confirmation BOOL,
	transfer_time TIMESTAMP WITH TIME ZONE
);
