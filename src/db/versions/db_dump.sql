--
-- PostgreSQL database dump
--

-- Dumped from database version 15.7
-- Dumped by pg_dump version 15.7

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: uuid-ossp; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS "uuid-ossp" WITH SCHEMA public;


--
-- Name: EXTENSION "uuid-ossp"; Type: COMMENT; Schema: -; Owner: 
--

COMMENT ON EXTENSION "uuid-ossp" IS 'generate universally unique identifiers (UUIDs)';


SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: cadastro; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.cadastro (
    id integer NOT NULL,
    titulo character varying(50) NOT NULL,
    descricao text,
    localizacao character varying(255),
    quantidade real DEFAULT 0 NOT NULL
);


ALTER TABLE public.cadastro OWNER TO postgres;

--
-- Name: cadastro_id_seq; Type: SEQUENCE; Schema: public; Owner: postgres
--

CREATE SEQUENCE public.cadastro_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER TABLE public.cadastro_id_seq OWNER TO postgres;

--
-- Name: cadastro_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: postgres
--

ALTER SEQUENCE public.cadastro_id_seq OWNED BY public.cadastro.id;


--
-- Name: cadastro id; Type: DEFAULT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.cadastro ALTER COLUMN id SET DEFAULT nextval('public.cadastro_id_seq'::regclass);


--
-- Data for Name: cadastro; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.cadastro (id, titulo, descricao, localizacao, quantidade) FROM stdin;
\.


--
-- Name: cadastro_id_seq; Type: SEQUENCE SET; Schema: public; Owner: postgres
--

SELECT pg_catalog.setval('public.cadastro_id_seq', 1, false);


--
-- Name: cadastro cadastro_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.cadastro
    ADD CONSTRAINT cadastro_pkey PRIMARY KEY (id);


--
-- PostgreSQL database dump complete
--

