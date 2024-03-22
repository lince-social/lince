DROP DATABASE lince;
CREATE DATABASE lince;

\c lince;

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE uuid_table (
	id UUID DEFAULT uuid_generate_v4() PRIMARY KEY,
);

CREATE TABLE conta (
usuario VARCHAR(30) NOT NULL,
senha VARCHAR(255) NOT NULL
); INHERITS (uuid_table);

CREATE TABLE uuid_e_conta (
	id_conta UUID REFERENCES conta(id) ON DELETE CASCADE
); INHERITS (uuid_table);

CREATE TABLE cadastro (
	titulo VARCHAR(50) NOT NULL,
	descricao TEXT,
	localizacao VARCHAR(255), 
	quantidade REAL DEFAULT 0 NOT NULL
); INHERITS (uuid_e_conta);

CREATE TABLE uuid_e_cadastro_foco (
	id_cadastro_foco UUID REFERENCES cadastro(id) ON DELETE CASCADE
); INHERITS (uuid_table);


CREATE TABLE periodicidade (
	periodos_desde_alteracao SMALLINT DEFAULT 0 NOT NULL CHECK (periodos_desde_alteracao >= 0),
	periodicidade SMALLINT DEFAULT 1 NOT NULL CHECK (periodicidade > 0),
	tipo_periodicidade_dia_true_mes_false BOOLEAN NOT NULL,
	data_inicio TIMESTAMP WITH TIME ZONE NOT NULL,
); INHERITS (quantidade_mudanca_cadastro_focado);

CREATE TABLE cadastro_observado (
	id_cadastro_observado UUID REFERENCES cadastro(id) ON DELETE CASCADE,
); INHERITS uuid_e_cadastro_foco

CREATE TABLE script
script_disparado VARCHAR(255) NOT NULL

CREATE TABLE quantidade_mudanca_cadastro_focado (
	quantidade_mudanca_cadastro_foco REAL NOT NULL,
); INHERITS (uuid_e_cadastro_foco);

CREATE TABLE observacao_anicca (
	mudanca_quantidade_cadastro_observado REAL NOT NULL,
	mudanca_quantidade_cadastro_alterado REAL NOT NULL
);




CREATE TABLE movimento (
	quantidade_contribuida REAL NOT NULL,
	id_cadastro_contribuido UUID REFERENCES cadastro(id) ON DELETE CASCADE,

	id_cadastro_retribuicao UUID references cadastro(id) ON DELETE CASCADE,
	quantidade_retribuida REAL NOT NULL,
	id_cadastro_retribuido UUID references cadastro(id) ON DELETE CASCADE,

	momento_movimento TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now()
); INHERITS (uuid_e_cadastro_foco);
