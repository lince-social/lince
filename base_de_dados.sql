CREATE TABLE conta (
  id SERIAL PRIMARY KEY,
  horario_criacao_conta TIMESTAMPZ DEFAULT CURRENT_TIMESTAMP,

  usuario VARCHAR(255) NOT NULL,
  senha VARCHAR(255) NOT NULL
);

CREATE TABLE cadastro (
  id SERIAL PRIMARY KEY,
  conta_id INTEGER REFERENCES conta(id) ON DELETE CASCADE,
  horario_criacao_cadastro TIMESTAMPZ DEFAULT CURRENT_TIMESTAMP,

  necessidade_ou_contribuicao CHAR(1) CHECK (necessidade_ou_contribuicao IN ('N', 'C')),
  titulo VARCHAR(50) NOT NULL,
  descricao TEXT NOT NULL,
  quantidade INTEGER NOT NULL,
  custo
  localizacao VARCHAR(255)
);

CREATE TABLE periodicidade (
  id SERIAL PRIMARY KEY,
  cadastro_id INTEGER REFERENCES cadastro(id) ON DELETE CASCADE,
  
  periodicidade SMALLINT DEFAULT 0 NOT NULL CHECK (periodicidade >= 0), --backend por favor: se periocidicidade == 0 então periodicidade do cadastro ta desativada.
  periodos_desde_criacao SMALLINT DEFAULT 0 NOT NULL,
  tipo_periodicidade VARCHAR(6) CHECK (tipo_periodicidade IN ('Dia', 'Semana', 'Mês')),
  data_inicio TIMESTAMPZ NOT NULL,
  cadastro_aumento INT DEFAULT 1 NOT NULL CHECK (cadastro_aumento >= 1)
);

INSERT INTO conta (usuario, senha) VALUES
('fulano', '1234'),
('beutrano', '4321');

INSERT INTO cadastro (conta_id, necessidade_ou_contribuicao, titulo, descricao, quantidade) VALUES
(1, 'C', 'par de chinelo', 'par de chinelos tamanho 34 branco seminovo', 1),
(2, 'N', 'chinelo 34/35', 'olá, busco por um chinelo tamanho 34/35 pra cima, mas não muito mior que isso', 1);

--INSERT INTO periodicidade()

