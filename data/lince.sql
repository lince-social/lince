CREATE TABLE conta (
  servidor VARCHAR(255), --(nao sei se é pra ser VARCHAR porque é um endereço ou se é melhor ser um número IPv4/IPv6, ou se deixo opcional pq a pessoa quer ter uma instância lince sozinha, vai que né..)
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

  )
