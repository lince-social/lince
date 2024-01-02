CREATE TABLE cadastro (
    id 
    titulo VARCHAR(255) NOT NULL,
    descricao TEXT,
    tipoCadastro VARCHAR(12) CHECK (tipoCadastro IN ('Necessidade', 'Contribuicao'),
    horarioCriacao TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    estado BOOLEAN NOT NULL
  );
