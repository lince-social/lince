DROP DATABASE lince;
CREATE DATABASE lince;

\c lince;

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE conta (
  id UUID DEFAULT uuid_generate_v4() PRIMARY KEY,
  
  usuario VARCHAR(30) NOT NULL,
  senha VARCHAR(255) NOT NULL
);

CREATE TABLE circulo (
  id UUID DEFAULT uuid_generate_v4(),
  id_conta UUID REFERENCES conta(id) ON DELETE CASCADE,
  
  PRIMARY KEY (id, id_conta)
);

CREATE TABLE cadastro (
  id UUID DEFAULT uuid_generate_v4() PRIMARY KEY,
  id_conta UUID REFERENCES conta(id) ON DELETE CASCADE,

  titulo VARCHAR(50) NOT NULL,
  descricao TEXT,
  localizacao VARCHAR(255), 
  quantidade REAL DEFAULT 0 NOT NULL
);

CREATE TABLE transferencia (
  id UUID DEFAULT uuid_generate_v4() PRIMARY KEY,
  
  id_cadastro_necessidade UUID REFERENCES cadastro(id) ON DELETE CASCADE,
  quantidade_contribuida REAL NOT NULL,
  id_cadastro_contribuicao UUID REFERENCES cadastro(id) ON DELETE CASCADE,
  
  id_cadastro_retribuicao UUID references cadastro(id) ON DELETE CASCADE,
  quantidade_retribuida REAL NOT NULL,
  id_cadastro_retribuido UUID references cadastro(id) ON DELETE CASCADE,

  momento_acordo TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT now()
);

CREATE TABLE observacao_ponto (
  id_cadastro_observado UUID REFERENCES cadastro(id) ON DELETE CASCADE,
  certa_quantidade_cadastro REAL NOT NULL,

  id_cadastro_alterado UUID REFERENCES cadastro(id) ON DELETE CASCADE,
  alteracao_quantidade_cadastro REAL NOT NULL,

  PRIMARY KEY (id_cadastro_observado, certa_quantidade_cadastro)
);

CREATE TABLE observacao_anicca (
  id UUID DEFAULT uuid_generate_v4() PRIMARY KEY,
  
  id_cadastro_observado UUID REFERENCES cadastro(id) ON DELETE CASCADE,
  mudanca_quantidade_cadastro_observado REAL NOT NULL,

  id_cadastro_alterado UUID REFERENCES cadastro(id) ON DELETE CASCADE,
  mudanca_quantidade_cadastro_alterado REAL NOT NULL
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

CREATE TABLE disparador_script_ponto (
  id UUID DEFAULT uuid_generate_v4() PRIMARY KEY,
  cadastro_id UUID REFERENCES cadastro(id) ON DELETE CASCADE, 
  certa_quantidade_cadastro REAL NOT NULL,
  script_name VARCHAR(255) NOT NULL
);
