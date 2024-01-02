CREATE TABLE conta (
    servidor VARCHAR(255),
    usuario VARCHAR(255),
    senha VARCHAR(255) NOT NULL,
    PRIMARY KEY (servidor, usuario)
);





CREATE TABLE cadastro (
    id 
    titulo VARCHAR(255) NOT NULL,
    descricao TEXT,
    tipoCadastro VARCHAR(12) CHECK (tipoCadastro IN ('Necessidade', 'Contribuicao'),
    horarioCriacao TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    estado BOOLEAN NOT NULL
  );

  CREATE TABLE servidor (
  endereco ?
  )

  CREATE TABLE periodicidade (
    id SERIAL PRIMARY KEY,
    cadastro_id INTEGER REFERENCES cadastro(id) ON DELETE CASCADE,
    periodicidade SMALLINT NOT NULL,
    tipo_periodicidade VARCHAR(6) CHECK (tipo_periodicidade IN ('Dia, Semana, Mês')),

  )
