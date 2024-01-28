CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE conta (
  id UUID DEFAULT uuid_generate_v4() PRIMARY KEY,
  
  usuario VARCHAR(30) NOT NULL,
  senha VARCHAR(255) NOT NULL
);

CREATE TABLE cadastro (
  id UUID DEFAULT uuid_generate_v4() PRIMARY KEY,
  conta_id UUID REFERENCES conta(id) ON DELETE CASCADE,
  
  titulo VARCHAR(50) NOT NULL,
  descricao TEXT,
  localizacao VARCHAR(255), 
  quantidade REAL DEFAULT 0 NOT NULL,
);

CREATE TABLE condicao (
  id SERIAL,
  cadastro_id UUID REFERENCES cadastro(id) ON DELETE CASCADE,

  certa_quantidade_cadastro REAL NOT NULL,

  delta_quantidade_cadastro REAL NOT NULL,

  PRIMARY KEY (cadastro_id, id)
);

CREATE TABLE periodicidade (
  id SERIAL,
  cadastro_id UUID REFERENCES cadastro(id) ON DELETE CASCADE,

  periodicidade SMALLINT DEFAULT 1 NOT NULL CHECK (periodicidade > 0), 
  periodos_desde_criacao SMALLINT DEFAULT 0 NOT NULL CHECK (periodos_desde_criacao >= 0),
  tipo_periodicidade VARCHAR(6) NOT NULL CHECK (tipo_periodicidade IN ('Dia', 'Semana', 'Mês')),
  data_inicio TIMESTAMP WITH TIME ZONE NOT NULL,

  delta_quantidade_cadastro REAL DEFAULT 1 NOT NULL,

  PRIMARY KEY (cadastro_id, id)
);

CREATE TABLE proposta_transferencia (
  cadastro_enviante_quantidade REAL NOT NULL,
  cadastro_enviante_id UUID REFERENCES cadastro(id) ON DELETE CASCADE,
  
  cadastro_receptor_quantidade REAL NOT NULL,
  cadastro_receptor_id UUID REFERENCES cadastro(id) ON DELETE CASCADE,
  
  PRIMARY KEY (cadastro_enviante_id, cadastro_receptor_id)
);

