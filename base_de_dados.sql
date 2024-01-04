CREATE TABLE conta (
  id SERIAL PRIMARY KEY
  usuario VARCHAR(255) NOT NULL,
  senha VARCHAR(255) NOT NULL
);

CREATE TABLE cadastro (
  id SERIAL PRIMARY KEY,
  horario_criacao TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  
  conta_id VARCHAR(255) REFERENCES conta(id) ON DELETE CASCADE,

  tipo_cadastro VARCHAR(12) CHECK (tipo_cadastro IN ('Necessidade', 'Contribuicao')),

  titulo VARCHAR(50) NOT NULL,
  descricao TEXT NOT NULL,
  quantidade INTEGER NOT NULL
  );

CREATE TABLE periodicidade (
  id SERIAL PRIMARY KEY,

  cadastro_id INTEGER REFERENCES cadastro(id) ON DELETE CASCADE,
  
  periodicidade SMALLINT NOT NULL,
  periodos_desde_criacao SMALLINT DEFAULT 0 NOT NULL,

  tipo_periodicidade VARCHAR(6) CHECK (tipo_periodicidade IN ('Dia, Semana, Mês')),
  
  data_inicio TIMESTAMPZ NOT NULL,

);

INSERT INTO conta (usuario, senha) VALUES ('fulano', '1234');
INSERT INTO conta (usuario, senha) VALUES ('beutrano', '4321');

INSERT INTO cadastro (conta_id, tipo_cadastro, titulo, descrição, quantidade) VALUES (1, 'Contribuicao', 'par de chinelo', 'par de chinelos tamanho 34 branco seminovo', 1);
INSERT INTO cadastro (2, 'Necessidade', 'chinelo 34/35', 'olá, busco por um chinelo tamanho 34/35 pra cima, mas não muito mior que isso', 1);

--INSERT INTO periodicidade()
