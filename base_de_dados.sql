CREATE TABLE conta (
  id SERIAL PRIMARY KEY,
  horario_criacao_conta TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
  usuario VARCHAR(30) NOT NULL,
  senha VARCHAR(255) NOT NULL
);

CREATE TABLE cadastro (
  id SERIAL PRIMARY KEY,
  conta_id INT REFERENCES conta(id) ON DELETE CASCADE,
  horario_criacao_cadastro TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
  titulo VARCHAR(50) NOT NULL,
  descricao TEXT,
  localizacao VARCHAR(255), 
  quantidade INT DEFAULT 0 NOT NULL
);

CREATE TABLE proposta_transferencia (
  cadastro_enviante_id INT REFERENCES cadastro(id) ON DELETE CASCADE,
  valor_proposto_transferencia REAL NOT NULL,
  cadastro_id_a_transferir INT REFERENCES cadastro(id) ON DELETE CASCADE,
  PRIMARY KEY (cadastro_enviante_id, cadastro_id_a_transferir)
);

CREATE TABLE transferencia (
  horario_transferencia TIMESTAMP WITH TIME ZONE DEFAULT CURRENT_TIMESTAMP,
  id SERIAL,
  cadastro_enviante_id INT REFERENCES cadastro(id) ON DELETE CASCADE,
  valor_transferido REAL NOT NULL,
  cadastro_receptor_id INT REFERENCES cadastro(id) ON DELETE CASCADE,
  PRIMARY KEY (id, cadastro_enviante_id)
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
