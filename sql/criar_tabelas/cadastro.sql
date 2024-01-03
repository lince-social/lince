CREATE TABLE cadastro (
  conta_usuario VARCHAR(255) REFERENCES conta(usuario) ON DELETE CASCADE,
  id SERIAL PRIMARY KEY,
  titulo VARCHAR(50) NOT NULL,
  descricao TEXT NOT NULL,
  quantidade INTEGER NOT NULL,
  tipo_cadastro VARCHAR(12) CHECK (tipoCadastro IN ('Necessidade', 'Contribuicao'),
  horario_criacao TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
  estado BOOLEAN NOT NULL
);
