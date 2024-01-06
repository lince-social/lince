CREATE TABLE conta (
  id SERIAL PRIMARY KEY,
  horario_criacao_conta TIMESTAMPZ DEFAULT CURRENT_TIMESTAMP,
  usuario VARCHAR(30) NOT NULL,
  senha VARCHAR(255) NOT NULL
);

CREATE TABLE cadastro (
  id SERIAL PRIMARY KEY,
  conta_id INT REFERENCES conta(id) ON DELETE CASCADE,
  horario_criacao_cadastro TIMESTAMPZ DEFAULT CURRENT_TIMESTAMP,
  titulo VARCHAR(50) NOT NULL,
  descricao TEXT NOT NULL,
  localizacao VARCHAR(255), 
  quantidade INT NOT NULL
);

CREATE TABLE condicao_transferencia (
  cadastro_enviante_id INT REFERENCES cadastro(id) ON DELETE CASCADE,
  custo_float REAL NOT NULL,
  cadastro_id_a_transferir INT REFERENCES cadastro(id) ON DELETE CASCADE,
  PRIMARY KEY (cadastro_id, cadastro_id_transferido)
);

CREATE TABLE transferencia (
  id SERIAL PRIMARY KEY,

  cadastro_enviante_id REFERENCES cadastro(id) ON DELETE CASCADE,
  valor_transferido REAL NOT NULL,
  oferta_cadastro_receptor_id REFERENCES cadastro(id) ON DELETE CASCADE

  cadastro_receptor_id REFERENCES cadastro(id) ON DELETE CASCADE
);

CREATE TABLE periodicidade (
  id SERIAL PRIMARY KEY,
  cadastro_id INT REFERENCES cadastro(id) ON DELETE CASCADE,
  periodicidade SMALLINT DEFAULT 0 NOT NULL CHECK (periodicidade >= 0), 
  periodos_desde_criacao SMALLINT DEFAULT 0 NOT NULL,
  tipo_periodicidade VARCHAR(6) NOT NULL CHECK (tipo_periodicidade IN ('Dia', 'Semana', 'Mês')),
  data_inicio TIMESTAMPZ NOT NULL,
  cadastro_quantidade_mudanca INT DEFAULT 1 NOT NULL
);

INSERT INTO conta (usuario, senha) VALUES
('fulano', '1234'),
('beutrano', '4321');

INSERT INTO cadastro (conta_id, necessidade_ou_contribuicao, titulo, descricao, quantidade) VALUES
(1, 'C', 'par de chinelo', 'par de chinelos tamanho 34 branco seminovo', 1),
(2, 'N', 'chinelo 34/35', 'olá, busco por um chinelo tamanho 34/35 pra cima, mas não muito mior que isso', 1);

--INSERT INTO periodicidade()

