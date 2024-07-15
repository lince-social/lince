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
	periods_since_alteration SMALLINT DEFAULT 0 NOT NULL CHECK (periods_since_alteration >= 0),
	periods SMALLINT DEFAULT 1 NOT NULL CHECK (periods > 0),
	days REAL DEFAULT 0 NOT NULL CHECK (days > 0),
	months REAL DEFAULT 0 NOT NULL CHECK (months > 0),
	starting_date_with_timezone TIMESTAMP WITH TIME ZONE NOT NULL
);

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
