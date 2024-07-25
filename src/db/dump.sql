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
-- Name: configuration; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.configuration (
    id integer NOT NULL,
    name text NOT NULL,
    quantity real DEFAULT 1 NOT NULL,
    save_mode text DEFAULT 'Automatic'::text NOT NULL,
    view text DEFAULT 'SELECT * FROM record WHERE quantity < 0 ORDER BY quantity ASC, title ASC, description ASC | SELECT * FROM configuration'::text NOT NULL,
    column_information text DEFAULT 'Verbose'::text NOT NULL,
    keymap jsonb DEFAULT '{}'::jsonb NOT NULL,
    truncation jsonb DEFAULT '{"record": {"description": 150}}'::jsonb NOT NULL,
    CONSTRAINT configuration_column_information_check CHECK ((column_information = ANY (ARRAY['Verbose'::text, 'Short'::text, 'Silent'::text]))),
    CONSTRAINT configuration_save_mode_check CHECK ((save_mode = ANY (ARRAY['Automatic'::text, 'Manual'::text])))
);


ALTER TABLE public.configuration OWNER TO postgres;

--
-- Name: configuration_id_seq; Type: SEQUENCE; Schema: public; Owner: postgres
--

CREATE SEQUENCE public.configuration_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER TABLE public.configuration_id_seq OWNER TO postgres;

--
-- Name: configuration_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: postgres
--

ALTER SEQUENCE public.configuration_id_seq OWNED BY public.configuration.id;


--
-- Name: record; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.record (
    id integer NOT NULL,
    title character varying(50) NOT NULL,
    description text,
    location character varying(255),
    quantity real DEFAULT 0 NOT NULL
);


ALTER TABLE public.record OWNER TO postgres;

--
-- Name: default_view; Type: VIEW; Schema: public; Owner: postgres
--

CREATE VIEW public.default_view AS
 SELECT record.id,
    record.title,
    record.description,
    record.location,
    record.quantity
   FROM public.record
  WHERE (record.quantity < (0)::double precision)
  ORDER BY record.quantity, record.title, record.description;


ALTER TABLE public.default_view OWNER TO postgres;

--
-- Name: frequency; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.frequency (
    id integer NOT NULL,
    quantity integer DEFAULT 1,
    day_week integer,
    months real DEFAULT 0 NOT NULL,
    days real DEFAULT 0 NOT NULL,
    seconds real DEFAULT 0 NOT NULL,
    next_date timestamp with time zone DEFAULT now() NOT NULL,
    record_id integer NOT NULL,
    delta real DEFAULT 0 NOT NULL,
    finish_date date,
    when_done boolean DEFAULT false
);


ALTER TABLE public.frequency OWNER TO postgres;

--
-- Name: frequency_id_seq; Type: SEQUENCE; Schema: public; Owner: postgres
--

CREATE SEQUENCE public.frequency_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER TABLE public.frequency_id_seq OWNER TO postgres;

--
-- Name: frequency_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: postgres
--

ALTER SEQUENCE public.frequency_id_seq OWNED BY public.frequency.id;


--
-- Name: record_id_seq; Type: SEQUENCE; Schema: public; Owner: postgres
--

CREATE SEQUENCE public.record_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER TABLE public.record_id_seq OWNER TO postgres;

--
-- Name: record_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: postgres
--

ALTER SEQUENCE public.record_id_seq OWNED BY public.record.id;


--
-- Name: configuration id; Type: DEFAULT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.configuration ALTER COLUMN id SET DEFAULT nextval('public.configuration_id_seq'::regclass);


--
-- Name: frequency id; Type: DEFAULT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.frequency ALTER COLUMN id SET DEFAULT nextval('public.frequency_id_seq'::regclass);


--
-- Name: record id; Type: DEFAULT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.record ALTER COLUMN id SET DEFAULT nextval('public.record_id_seq'::regclass);


--
-- Data for Name: configuration; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.configuration (id, name, quantity, save_mode, view, column_information, keymap, truncation) FROM stdin;
1	Default	1	Automatic	SELECT * FROM record WHERE quantity < 0 ORDER BY quantity ASC, title ASC, description ASC	Verbose	{}	{"record": {"description": 150}}
\.


--
-- Data for Name: frequency; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.frequency (id, quantity, day_week, months, days, seconds, next_date, record_id, delta, finish_date, when_done) FROM stdin;
\.


--
-- Data for Name: record; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.record (id, title, description, location, quantity) FROM stdin;
42	v0.4.0	Checkpoint, i.e. "When a quantity reaches 4"	\N	0
44	v0.4.0	Proportion, i.e. "When a quantity changes a certain number"	\N	0
63	v0.5.0	Transfer proposal and connection, i.e "A proposal of transfering a quantity from A to B, in return (or not) C receives some from D" "If you transfer an amount of apples to my apple registration, I will transfer so much money from my registration to yours. Contribution and Retribution (optional if it is a donation)."	\N	0
64	v0.5.0	Transferência múltipla, i.e "Entregar diversos itens por um só. Para comprar essa bota eu ofereço 20 reais e um candelabro".	\N	0
65	v0.5.0	Eventually consistent	\N	0
66	v0.5.0	Decentralization	\N	0
67	v0.5.0	Eventually consistent databases	\N	0
68	v0.5.0	pub/sub protocols, i.e. "Pessoas podem se inscrever com pub/sub com cada cadastro, referente ao assunto escolhido."	\N	0
69	v0.5.0	Merkle-CRDTs implementation	\N	0
70	v0.5.0	Authentication. # Check gajim for login inspiration.	\N	0
71	v0.5.0	É possível utilizar IPFS e libp2p, diversos outras ferramentas pra auxiliar no processo de compartilhamento de cadastros, condições e transferências	\N	0
72	v0.5.0	Fazer com que as pessoas possam utilizar máquinas que tem controle para aliviar o tráfego em certos pontos da rede e permitir mais em geral. Não necessariamente as que elas possuem.	\N	0
73	v0.6.0	Browser version, with dropdowns and buttons and pages and erethang	\N	0
74	v0.6.0	The registration can be done by typing, voice, photo or video, making it accessible and easy to use. For those without access to technology, it is possible to add their needs and contributions through any device or party.	\N	0
75	v0.7.0	Sugestão de Enriquecimento de cards, a pessoa coloca um prompt/foto do NI/CE e um MMLLM adiciona metadata, dps a pessoa confirma.	\N	0
76	v0.7.0	Algorithm and/or ML for optimization of transfer quotas and cost-efficient connections. By digitizing the information, Lince enables the use of optimization algorithms and machine learning for more effective planning of contributions. However, it is necessary to consider the human biases present in the algorithms and ensure transparency, consent and democracy when implementing any decision-making artificial intelligence. Lince Modelo/Template.	\N	0
41	v0.4.0	Conditions (The objective is to have generalized conditions and consequences, so anything can trigger anything else.\\n A periodicity can run a script, a checkpoint can change a quantity through a proportion, etc.)	\N	0
43	v0.4.0	Rate, i.e. "When a quantity changes in a certain rate (change/time)"	\N	0
45	v0.4.0	Command, i.e. "Shell command, being able to trigger any script in any language, easy to do with nix-shells for dev envs"	\N	0
46	v0.4.0	Delta, i.e. "Set a quantity to more or less a number, -1, +4, etc."	\N	0
47	v0.4.0	Arquitetura de condições e consequências	\N	0
48	v0.4.0	Transform frequency into a building block, to be used just like any condition/consequence	\N	0
49	v0.4.0	Simulção do efeito do tempo nos cadastros, se possível a partir dos cadastros de todos, coletar esses dados e rodar uma função de pesquisa operacional de simulação, também funciona quando uma transferência foi confirmada mas não efetivada no mundo material, só prometida, aí vai ter o valor atual da quantidade -1 e o previsto caso tudo continue como setado.	\N	0
78	v0.3.0	Bug: when query select * from table <table> not returning rows	\N	0
60	v0.3.0	config filter on what tables	\N	0
55	v0.3.0	config save mode	\N	0
59	v0.3.0	config sort	\N	0
58	v0.3.0	config menu options shown and their disposition on the screen and columns..	\N	0
56	v0.3.0	config tables show at main view	\N	0
54	v0.3.0	Criar table config, vai conter as configurações do aplicativo, quando o aplicativo começar ele vai olhar todas as colunas e se comportar conforme as informações, cada linha dessa table é um perfil, sempre há o perfil atual, com quantidade tal, e outros com outra quantidade. essa table sempre tem uma linha com as configs padrão.	\N	0
80	v0.3.0	Fix order of main functions so startup isnt janky, things look different after you go to the while loop, menu & stuff	\N	0
52	v0.3.0	Templates: prov uma manifestação de uso. Pessoas vão modelar sua cadeia produtiva e mostrar ou não, as que mostrarem vão deixar pessoas copiar tudo. Depois o que sobra são trabalhadores se colocando nas tarefas pra levantar tal empreitada.	\N	0
79	v0.3.0	Bug: capital F not activating SQL File option	\N	0
62	v0.3.0	config change architecture main. reads from config table and passes info as argument to functions	\N	0
61	v0.3.0	config view selected config options	\N	0
57	v0.3.0	config views	\N	0
50	v0.3.0	make quantity zero with minimum types of keyboard	\N	0
53	v0.3.0	Feature | Copy easily any row or group of rows. Be able to edit row per row, in bulk or not at all.	\N	-1
77	v0.3.0	Feature | Configuration: truncate certain columns in certain tables in accordance with truncate column in configuration table.	\N	-1
51	v0.3.0	Feature | Add help in the format of a complete text as an operation	\N	-1
82	v0.3.0	Feature | Make all consequences have quantity, inheriting id+quantity table. Make a pack of consequences have multiple possibilities for retiring. Different bottlenecks, more limitations for something to happen, more precision on modeling Ns & Cs.	\N	-1
83	v0.3.0	Feature | Configuration: add information in different quantities, may be nothing at all, to columns when filling them. Read from configuration table active row. Also to operations when performing them in any way.	\N	-1
84	Backlog	Bug | When starting two different instances of lince the server has problems, no pg_dump, weird messages.	\N	0
81	v0.3.0	Enhancement | Make all tables that should have quantity inherit a id and quantity table.	\N	0
\.


--
-- Name: configuration_id_seq; Type: SEQUENCE SET; Schema: public; Owner: postgres
--

SELECT pg_catalog.setval('public.configuration_id_seq', 1, false);


--
-- Name: frequency_id_seq; Type: SEQUENCE SET; Schema: public; Owner: postgres
--

SELECT pg_catalog.setval('public.frequency_id_seq', 9, true);


--
-- Name: record_id_seq; Type: SEQUENCE SET; Schema: public; Owner: postgres
--

SELECT pg_catalog.setval('public.record_id_seq', 84, true);


--
-- Name: configuration configuration_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.configuration
    ADD CONSTRAINT configuration_pkey PRIMARY KEY (id);


--
-- Name: frequency frequency_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.frequency
    ADD CONSTRAINT frequency_pkey PRIMARY KEY (id);


--
-- Name: record record_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.record
    ADD CONSTRAINT record_pkey PRIMARY KEY (id);


--
-- Name: frequency frequency_record_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.frequency
    ADD CONSTRAINT frequency_record_id_fkey FOREIGN KEY (record_id) REFERENCES public.record(id) ON DELETE CASCADE;


--
-- PostgreSQL database dump complete
--

