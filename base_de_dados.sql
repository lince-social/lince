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
  quantidade INT DEFAULT 0 NOT NULL,
);

CREATE TABLE condicao (
  id SERIAL PRIMARY KEY,
  cadastro_id INT REFERENCES cadastro(id) ON DELETE CASCADE,
  condicao_quantidade INT NOT NULL,
  condicao_quantidade_mudanca INT NOT NULL
);

CREATE TABLE periodicidade (
  id SERIAL PRIMARY KEY,
  cadastro_id INT REFERENCES cadastro(id) ON DELETE CASCADE,
  periodicidade SMALLINT DEFAULT 0 NOT NULL CHECK (periodicidade >= 0), 
  periodos_desde_criacao SMALLINT DEFAULT 0 NOT NULL,
  tipo_periodicidade VARCHAR(6) NOT NULL CHECK (tipo_periodicidade IN ('Dia', 'Semana', 'Mês')),
  data_inicio TIMESTAMP WITH TIME ZONE NOT NULL,
  periodicidade_quantidade_mudanca INT DEFAULT 1 NOT NULL
);

CREATE TABLE proposta_transferencia (
  cadastro_enviante_quantidade REAL NOT NULL,
  cadastro_enviante_id UUID REFERENCES cadastro(id) ON DELETE CASCADE,
  
  cadastro_receptor_quantidade REAL NOT NULL,
  cadastro_receptor_id UUID REFERENCES cadastro(id) ON DELETE CASCADE,
  
  PRIMARY KEY (cadastro_enviante_id, cadastro_receptor_id)
);

