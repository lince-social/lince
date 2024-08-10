CREATE TABLE views (
	id SERIAL PRIMARY KEY,
 	view TEXT NOT NULL DEFAULT 'SELECT * FROM record'
);

CREATE TABLE configuration (
	id SERIAL PRIMARY KEY,
	quantity REAL NOT NULL DEFAULT 1,
	save_mode VARCHAR(9) NOT NULL DEFAULT 'Automatic' CHECK (save_mode in ('Automatic', 'Manual')),
 	view_id INTEGER NOT NULL DEFAULT 1,
	column_information_mode VARCHAR(7) NOT NULL DEFAULT 'verbose' CHECK (column_information_mode in ('verbose', 'short', 'silent')),
	keymap jsonb NOT NULL DEFAULT '{}',
	truncation jsonb NOT NULL DEFAULT '{"body": 150, "view": 100}',
	table_query jsonb NOT NULL DEFAULT '{"record": "SELECT * FROM RECORD ORDER BY quantity ASC, head ASC, body ASC, id ASC", "frequency": "SELECT * FROM frequency ORDER BY id ASC"}',
	language VARCHAR(15) NOT NULL DEFAULT 'en-US',
	timezone VARCHAR(3) NOT NULL DEFAULT '-3',
	startup_db VARCHAR(50) DEFAULT 'default',
	last_db VARCHAR(50) NOT NULL DEFAULT 'default'
);

CREATE TABLE record (
	id SERIAL PRIMARY KEY,
	quantity REAL NOT NULL DEFAULT 1,
	head TEXT,
	body TEXT,
	location POINT
);

CREATE TABLE history (
    id SERIAL PRIMARY KEY,
    record_id INTEGER NOT NULL,
    change_time TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    old_quantity REAL NOT NULL,
    new_quantity REAL NOT NULL
);

CREATE TABLE karma (
	id SERIAL PRIMARY KEY,
	quantity INTEGER NOT NULL DEFAULT 1,
	expression TEXT
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

CREATE TABLE sum (
	id SERIAL PRIMARY KEY,
	quantity INTEGER NOT NULL DEFAULT 1,
	record_id INTEGER NOT NULL,

	sum_mode INTEGER NOT NULL DEFAULT 0 CHECK (sum_mode in (-1,0,1)),
    interval_mode VARCHAR(10) NOT NULL DEFAULT 'relative' CHECK (interval_mode IN ('fixed', 'relative')),

    interval_length INTERVAL NOT NULL,
	end_lag interval,
    end_date TIMESTAMP WITH TIME ZONE DEFAULT now()
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

CREATE OR REPLACE FUNCTION record_quantity_change()
RETURNS TRIGGER AS $$
BEGIN
    IF NEW.quantity IS DISTINCT FROM OLD.quantity THEN
        INSERT INTO history (record_id, old_quantity, new_quantity)
        VALUES (OLD.id, OLD.quantity, NEW.quantity);
    END IF;
    RETURN NEW;
END;
$$
 LANGUAGE plpgsql;

CREATE TRIGGER record_quantity_update
AFTER UPDATE OF quantity ON record
FOR EACH ROW
EXECUTE FUNCTION record_quantity_change();
