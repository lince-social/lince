CREATE TABLE periodicidade (
  cadastro_id INTEGER REFERENCES cadastro(id) ON DELETE CASCADE,
  id SERIAL PRIMARY KEY,
  data_inicio TIMESTAMPZ NOT NULL,
  periodicidade SMALLINT NOT NULL,
  periodos_desde_criacao SMALLINT DEFAULT 0,
  tipo_periodicidade VARCHAR(6) CHECK (tipo_periodicidade IN ('Dia, Semana, Mês'))
);
