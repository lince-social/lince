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
-- Name: frequency; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.frequency (
    id integer NOT NULL,
    periods_since_alteration smallint DEFAULT 0 NOT NULL,
    periods smallint DEFAULT 1 NOT NULL,
    days real DEFAULT 0 NOT NULL,
    months real DEFAULT 0 NOT NULL,
    starting_date_with_timezone timestamp with time zone NOT NULL,
    CONSTRAINT frequency_days_check CHECK ((days > (0)::double precision)),
    CONSTRAINT frequency_months_check CHECK ((months > (0)::double precision)),
    CONSTRAINT frequency_periods_check CHECK ((periods > 0)),
    CONSTRAINT frequency_periods_since_alteration_check CHECK ((periods_since_alteration >= 0))
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
-- Name: test; Type: TABLE; Schema: public; Owner: postgres
--

CREATE TABLE public.test (
    id integer NOT NULL
);


ALTER TABLE public.test OWNER TO postgres;

--
-- Name: test_id_seq; Type: SEQUENCE; Schema: public; Owner: postgres
--

CREATE SEQUENCE public.test_id_seq
    AS integer
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


ALTER TABLE public.test_id_seq OWNER TO postgres;

--
-- Name: test_id_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: postgres
--

ALTER SEQUENCE public.test_id_seq OWNED BY public.test.id;


--
-- Name: frequency id; Type: DEFAULT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.frequency ALTER COLUMN id SET DEFAULT nextval('public.frequency_id_seq'::regclass);


--
-- Name: record id; Type: DEFAULT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.record ALTER COLUMN id SET DEFAULT nextval('public.record_id_seq'::regclass);


--
-- Name: test id; Type: DEFAULT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.test ALTER COLUMN id SET DEFAULT nextval('public.test_id_seq'::regclass);


--
-- Data for Name: frequency; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.frequency (id, periods_since_alteration, periods, days, months, starting_date_with_timezone) FROM stdin;
\.


--
-- Data for Name: record; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.record (id, title, description, location, quantity) FROM stdin;
9	task	Eventually consistent	\N	-1
10	task	Decentralization	\N	-1
11	task	Eventually consistent databases	\N	-1
12	task	pub/sub protocols, i.e. "Pessoas podem se inscrever com pub/sub com cada cadastro, referente ao assunto escolhido."	\N	-1
13	task	Merkle-CRDTs implementation	\N	-1
14	task	Authentication. # Check gajim for login inspiration.	\N	-1
15	task	Fazer com que as pessoas possam utilizar máquinas que tem controle para aliviar o tráfego em certos pontos da rede e permitir mais em geral. Não necessariamente as que elas possuem.	\N	-1
16	task	É possível utilizar IPFS e libp2p, diversos outras ferramentas pra auxiliar no processo de compartilhamento de cadastros, condições e transferências	\N	-1
17	task	Conditions (The objective is to have generalized conditions and consequences, so anything can trigger anything else. A periodicity can run a script, a checkpoint can change a quantity through a proportion, etc.)	\N	-1
18	task	Periodicity, i.e. "Every two months and 4 weeks on a Thursday"	\N	-1
19	task	Alem de rodar pra sempre, ter a possibilidade de acabar depois de tantos períodos ou tempo.	\N	-1
20	task	Checkpoint, i.e. "When a quantity reaches 4"	\N	-1
21	task	Rate, i.e. "When a quantity changes in a certain rate (change/time)"	\N	-1
22	task	Proportion, i.e. "When a quantity changes a certain number"	\N	-1
23	task	Consequences	\N	-1
24	task	Checkpoint, i.e "Set a quantity to a specific number"	\N	-1
25	task	Delta, i.e. "Set a quantity to more or less a number, -1, +4, etc."	\N	-1
26	task	Command, i.e. "Shell command, being able to trigger any script in any language, easy to do with nix-shells for dev envs"	\N	-1
27	task	Add string concatenation on query answerage so only filled things are added on the query, so like select * from table + if answered where "where cause", else its just select * from table, with all operations	\N	-1
28	task	The registration can be done by typing, voice, photo or video, making it accessible and easy to use. For those without access to technology, it is possible to add their needs and contributions through any device or party.	\N	-1
29	task	Transfer proposal and connection, i.e "A proposal of transfering a quantity from A to B, in return (or not) C receives some from D" "If you transfer an amount of apples to my apple registration, I will transfer so much money from my registration to yours. Contribution and Retribution (optional if it is a donation)."	\N	-1
30	task	Transferência múltipla, i.e "Entregar diversos itens por um só. Para comprar essa bota eu ofereço 20 reais e um candelabro".	\N	-1
31	task	Algorithm and/or ML for optimization of transfer quotas and cost-efficient connections. By digitizing the information, Lince enables the use of optimization algorithms and machine learning for more effective planning of contributions. However, it is necessary to consider the human biases present in the algorithms and ensure transparency, consent and democracy when implementing any decision-making artificial intelligence. Lince Modelo/Template.	\N	-1
32	task	Sugestão de Enriquecimento de cards, a pessoa coloca um prompt/foto do NI/CE e um MMLLM adiciona metadata, dps a pessoa confirma.	\N	-1
33	task	Copiar cadastro, seja seu ou de outrem	\N	-1
34	task	Simulção do efeito do tempo nos cadastros, se possível a partir dos cadastros de todos, coletar esses dados e rodar uma função de pesquisa operacional de simulação	\N	-1
35	task	Pensar sobre templates, provavelmente seria uma manifestação de mecanismos disponíveis. E não uma feature em si, se da pra copiar cadastros, é possivel que existam blueprints pra produção de certas coisas, com todas as partes documentadas, com todas as partes com tarefas pra produzir aquilo, o resto é colocar a simulação no tempo depois de copiado, fazer alterações, salvar como bases de dados diferentes em arquivos diferentes. Criações de Ns e Cs automáticas, segundo certas boas práticas de grupos ou preferências. Se entende como produzir algo, ou algo sobre produção e se modela qual a melhor maneira de fazer isso. Quais recursos precisam estar em qual lugar. Quem precisa contribuir com o que. Esses templates podem ser disponibilizados como arquiteturas. Pessoas se voluntariam a preencher certos campos e pegam parte dessa arquitetura como responsabilidade. Quanto todas as partes tiverem sido pegas e feitas aquela Grande Necessidade estará completa. Algo que se precisa mas faz parte de um esforço coletivo para conseguí-la.	\N	-1
36	task	É preciso aprender como funcionam diversas engrenagens e variáveis de processos produtivos e fazer com que a Lince consiga melhor organizar essa produção de forma ótima, a melhor, não importa o escopo. O desenvolvimento é alavancado por ouvir certos trabalhadores de diversas áreas e adaptar o aplicativo pra ficar mais intuitivo	\N	-1
\.


--
-- Data for Name: test; Type: TABLE DATA; Schema: public; Owner: postgres
--

COPY public.test (id) FROM stdin;
\.


--
-- Name: frequency_id_seq; Type: SEQUENCE SET; Schema: public; Owner: postgres
--

SELECT pg_catalog.setval('public.frequency_id_seq', 1, false);


--
-- Name: record_id_seq; Type: SEQUENCE SET; Schema: public; Owner: postgres
--

SELECT pg_catalog.setval('public.record_id_seq', 36, true);


--
-- Name: test_id_seq; Type: SEQUENCE SET; Schema: public; Owner: postgres
--

SELECT pg_catalog.setval('public.test_id_seq', 1, false);


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
-- Name: test test_pkey; Type: CONSTRAINT; Schema: public; Owner: postgres
--

ALTER TABLE ONLY public.test
    ADD CONSTRAINT test_pkey PRIMARY KEY (id);


--
-- PostgreSQL database dump complete
--

