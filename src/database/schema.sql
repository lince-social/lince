CREATE TABLE dna (
id SERIAL PRIMARY KEY,
quantity INTEGER NOT NULL DEFAULT 0,
origin TEXT NOT NULL DEFAULT 'lince.sql'
);

CREATE TABLE view (
	id SERIAL PRIMARY KEY,
	view_name TEXT NOT NULL,
	query TEXT NOT NULL DEFAULT 'SELECT * FROM record'
);

CREATE TABLE configuration (
	id SERIAL PRIMARY KEY,
	configuration_name VARCHAR(50) NOT NULL,
	quantity REAL NOT NULL DEFAULT 0,
	language VARCHAR(20),
	timezone VARCHAR(3) NOT NULL DEFAULT '0',
	style VARCHAR(50)
);

CREATE TABLE configuration_view (
	configuration_id INT REFERENCES configuration(id) ON DELETE CASCADE,
	view_id INT REFERENCES view(id) ON DELETE CASCADE,
	is_active BOOLEAN NOT NULL DEFAULT false,
	PRIMARY KEY (configuration_id, view_id)
);

CREATE TABLE record (
	id SERIAL PRIMARY KEY,
	quantity REAL NOT NULL DEFAULT 1,
	head TEXT,
	body TEXT,
	location POINT,
	save_history BOOLEAN
);

CREATE TABLE history (
	id SERIAL PRIMARY KEY,
	record_id INTEGER NOT NULL,
	change_time TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
	old_quantity REAL NOT NULL,
	new_quantity REAL NOT NULL
);

CREATE TABLE karma_condition (
	id SERIAL PRIMARY KEY,
	quantity INTEGER,
	condition TEXT NOT NULL
);

CREATE TABLE karma_consequence (
	id SERIAL PRIMARY KEY,
	quantity INTEGER,
	consequence TEXT NOT NULL
);

CREATE TABLE karma(
	id SERIAL PRIMARY KEY,
	quantity INTEGER,
	condition_id INT NOT NULL,
	operator TEXT NOT NULL,
	consequence_id INT NOT NULL
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
