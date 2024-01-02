  CREATE TABLE periodicidade (
    id SERIAL PRIMARY KEY,
    cadastro_id INTEGER REFERENCES cadastro(id) ON DELETE CASCADE,
    periodicidade SMALLINT NOT NULL,
    tipo_periodicidade VARCHAR(6) CHECK (tipo_periodicidade IN ('Dia, Semana, Mês')),

  );
