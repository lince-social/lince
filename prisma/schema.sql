CREATE TABLE dna (
id SERIAL PRIMARY KEY,
quantity INTEGER NOT NULL DEFAULT 0,
origin TEXT NOT NULL DEFAULT "lince.sql"
);

CREATE TABLE view (
	id SERIAL PRIMARY KEY,
	name TEXT
 	query TEXT NOT NULL DEFAULT 'SELECT * FROM record',
);

CREATE TABLE configuration (
	id SERIAL PRIMARY KEY,
	configurationName VARCHAR(50),
	quantity REAL NOT NULL DEFAULT 1,
 	views jsonb NOT NULL DEFAULT '{}',
	language VARCHAR(20),
	timezone VARCHAR(3) NOT NULL DEFAULT '0',
	style VARCHAR(50)
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
	consequence TEXT
);

CREATE TABLE frequency (
	id SERIAL PRIMARY KEY,
	quantity INTEGER NOT NULL DEFAULT 1,
	day_week REAL,
	months REAL DEFAULT 0 NOT NULL,
	days REAL DEFAULT 0 NOT NULL,
	seconds REAL DEFAULT 0 NOT NULL,
	next_date TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
	finish_date DATE,
	catch_up_sum INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE sum (
	id SERIAL PRIMARY KEY,
	quantity INTEGER NOT NULL DEFAULT 1,
	record_id INTEGER NOT NULL,

	sum_mode INTEGER NOT NULL DEFAULT 0 CHECK (sum_mode in (-1,0,1)),
        interval_length INTERVAL NOT NULL,
        interval_relative BOOL NOT NULL DEFAULT TRUE,

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

	records_received JSON,
	records_contributed JSON,

	agreement JSON,
	agreement_time TIMESTAMP WITH TIME ZONE,

	transfer_confirmation JSON,
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
