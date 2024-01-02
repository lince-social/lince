CREATE TABLE conta (
    usuario VARCHAR(255) PRIMARY KEY,
    senha VARCHAR(255) NOT NULL
);

CREATE TABLE cadastro (
    id SERIAL PRIMARY KEY,
    titulo VARCHAR(255) NOT NULL,
    descricao TEXT,
    conta_usuario VARCHAR(255) REFERENCES conta(usuario) ON DELETE CASCADE
);

CREATE TABLE conexao_lince (
    id SERIAL PRIMARY KEY,
    id_cadastro_1 INTEGER REFERENCES cadastro(id) ON DELETE CASCADE,
    id_cadastro_2 INTEGER REFERENCES cadastro(id) ON DELETE CASCADE
);

CREATE TABLE chat (
    id SERIAL PRIMARY KEY,
    nome_usuario_conta_1 VARCHAR(255) REFERENCES conta(nome_usuario) ON DELETE CASCADE,
    nome_usuario_conta_2 VARCHAR(255) REFERENCES conta(nome_usuario) ON DELETE CASCADE
);

CREATE TABLE mensagem (
    id SERIAL PRIMARY KEY,
    conteudo TEXT NOT NULL,
    horario_criacao TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    conta_usuario VARCHAR(255) REFERENCES conta(usuario) ON DELETE CASCADE,
    chat_id INTEGER REFERENCES chat(id) ON DELETE CASCADE
);

CREATE TABLE cadastro_tags (
    cadastro_id INTEGER REFERENCES cadastro(id) ON DELETE CASCADE,
    tag VARCHAR(255),
    PRIMARY KEY (cadastro_id, tag)
);

CREATE TABLE circulo


