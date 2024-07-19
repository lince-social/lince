CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE record (
	id SERIAL PRIMARY KEY,
	title VARCHAR(50) NOT NULL,
	description TEXT,
	location VARCHAR(255), 
	quantity REAL DEFAULT 0 NOT NULL
);

CREATE TABLE frequency (
	id SERIAL PRIMARY KEY,
	day_week INTEGER,
	months REAL DEFAULT 0 NOT NULL,
	days REAL DEFAULT 0 NOT NULL,
	seconds REAL DEFAULT 0 NOT NULL,
	next_date TIMESTAMP WITH TIME ZONE DEFAULT now() NOT NULL,
	record_id INTEGER REFERENCES record(id) ON DELETE CASCADE,
	delta REAL DEFAULT 0 NOT NULL,
	times INTEGER DEFAULT 1,
	finish_date DATE,
	when_done BOOLEAN DEFAULT false -- this is actually a checkpoint, when becomes 0 update the days counting from today, not the next_period
);














-- CREATE TABLE consequence (
-- 	id SERIAL PRIMARY KEY,
-- 	record_id INTEGER REFERENCES record(id) ON DELETE CASCADE	
-- );

-- CREATE TABLE delta ( delta REAL DEFAULT 0 NOT NULL ) INHERITS consequence;

-- CREATE TABLE checkpoint ( 
-- inferior_limit_is_open BOOLEAN NOT NULL DEFAULT true
-- inferior_limit REAL DEFAULT 0 NOT NULL 
-- upper_limit_is_open BOOLEAN NOT NULL DEFAULT true
-- upper_limit REAL DEFAULT 0 NOT NULL
-- ) INHERITS consequence;

-- create history on each table

-- CREATE TABLE app_mode ( make every app configuration through an sql table, to be sent like any old data, to be copied like any data. reproducibility. customization. the user can change a frontend link variable that uses another frontend, through a self made window. and its integrated on the app
-- create diferent modes for saving autosave etc
-- table view, side by side or on top of menu, on the bottom, just menu, just table rows, what tables, how many rows from each table
-- 	menu_expansion BOOLEAN
-- );
---
-- when condition_id
-- do consequence_id to table(id)
---
-- CREATE TABLE condition (
	-- id SERIAL PRIMARY KEY,

	-- quantity REAL NOT NULL DEFAULT 0,

-- )






-- CREATE TABLE uuid_table (
-- 	id UUID DEFAULT uuid_generate_v4() PRIMARY KEY,
-- );

-- CREATE TABLE conta (
-- usuario VARCHAR(30) NOT NULL,
-- senha VARCHAR(255) NOT NULL
-- ); INHERITS (uuid_table);

-- CREATE TABLE uuid_e_conta (
-- 	id_conta UUID REFERENCES conta(id) ON DELETE CASCADE
-- ); INHERITS (uuid_table);

-- CREATE TABLE script (
-- 	path_script_disparado VARCHAR(255) NOT NULL
-- ); INHERITS (uuid_table);

-- CREATE TABLE uuid_e_cadastro_foco (
-- 	id_cadastro_foco UUID REFERENCES cadastro(id) ON DELETE CASCADE
-- ); INHERITS (uuid_table);

-- CREATE TABLE cadastro_foco_e_alterado (
-- 	id_cadastro_alterado UUID REFERENCES cadastro(id) ON DELETE CASCADE
-- ); INHERITS (uuid_e_cadastro_foco);


-- CREATE TABLE ponto (
-- 	quantidade_ponto UUID REFERENCES cadastro(id) ON DELETE CASCADE,
-- ); INHERITS (cadastro_foco_e_alterado);

-- CREATE TABLE proporcao (
-- 	mudanca_quantidade_cadastro_observado REAL NOT NULL,
-- 	mudanca_quantidade_cadastro_alterado REAL NOT NULL
-- );

-- CREATE TABLE velocidade (
-- 	velocidade_quantidade_cadastro_observado REAL NOT NULL,
-- 	mudanca_quantidade_cadastro_alterado REAL NOT NULL
-- );
-- CREATE TABLE movimento (
-- 	quantidade_contribuida REAL NOT NULL,
-- 	id_cadastro_contribuido UUID REFERENCES cadastro(id) ON DELETE CASCADE,

-- 	id_cadastro_retribuicao UUID references cadastro(id) ON DELETE CASCADE,
-- 	quantidade_retribuida REAL NOT NULL,
-- 	id_cadastro_retribuido UUID references cadastro(id) ON DELETE CASCADE,

-- 	momento_movimento TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now()
-- ); INHERITS (uuid_e_cadastro_foco);


-- CREATE TABLE consequencia (

-- ); INHERITS 
