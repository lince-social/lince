-- DROP DATABASE lince;
-- CREATE DATABASE lince;

-- \c lince;

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE cadastro (
	id SERIAL PRIMARY KEY,
	titulo VARCHAR(50) NOT NULL,
	descricao TEXT,
	localizacao VARCHAR(255), 
	quantidade REAL DEFAULT 0 NOT NULL
);

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

-- CREATE TABLE periodicidade (
-- 	periodos_desde_alteracao SMALLINT DEFAULT 0 NOT NULL CHECK (periodos_desde_alteracao >= 0),
-- 	periodicidade SMALLINT DEFAULT 1 NOT NULL CHECK (periodicidade > 0),
-- 	tipo_periodicidade_dia_true_mes_false BOOLEAN NOT NULL,
-- 	data_inicio TIMESTAMP WITH TIME ZONE NOT NULL,
-- ); INHERITS (uuid_table);

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
