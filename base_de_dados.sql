CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE conta (
  id UUID DEFAULT uuid_generate_v4() PRIMARY KEY,

  usuario VARCHAR(30) NOT NULL,
  senha VARCHAR(255) NOT NULL
);

CREATE TABLE cadastro (
  id UUID DEFAULT uuid_generate_v4() PRIMARY KEY,
  id_conta UUID REFERENCES conta(id) ON DELETE CASCADE,

  titulo VARCHAR(50) NOT NULL,
  descricao TEXT,
  localizacao VARCHAR(255), 
  quantidade REAL DEFAULT 0 NOT NULL
);

CREATE TABLE proposta_transferencia (
  quantidade_cadastro_enviante REAL NOT NULL,
  id_cadastro_enviante UUID REFERENCES cadastro(id) ON DELETE CASCADE,

  quantidade_cadastro_receptor REAL NOT NULL,
  id_cadastro_receptor UUID REFERENCES cadastro(id) ON DELETE CASCADE,

  PRIMARY KEY (id_cadastro_enviante, id_cadastro_receptor)
);

CREATE TABLE sentinela (
  id_cadastro_observado UUID REFERENCES cadastro(id) ON DELETE CASCADE,
  certa_quantidade_cadastro REAL NOT NULL,

  id_cadastro_alterado UUID REFERENCES cadastro(id) ON DELETE CASCADE,
  alteracao_quantidade_cadastro REAL NOT NULL,

  PRIMARY KEY (id_cadastro_observado, certa_quantidade_cadastro)
);

CREATE TABLE periodicidade (
  id UUID DEFAULT uuid_generate_v4() PRIMARY KEY,

  periodos_desde_alteracao SMALLINT DEFAULT 0 NOT NULL CHECK (periodos_desde_alteracao >= 0),
  periodicidade SMALLINT DEFAULT 1 NOT NULL CHECK (periodicidade > 0),
  tipo_periodicidade_dia_true_mes_false BOOLEAN NOT NULL,
  data_inicio TIMESTAMP WITH TIME ZONE NOT NULL,

  id_cadastro_alterado UUID REFERENCES cadastro(id) ON DELETE CASCADE,
  alteracao_quantidade_cadastro REAL NOT NULL
);

