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
-- Data for Name: command; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.command VALUES (1, -1, 'touch test.rs');
INSERT INTO public.command VALUES (2, 1, 'touch test.py');
INSERT INTO public.command VALUES (3, 1, 'pdismc');
INSERT INTO public.command VALUES (4, 1, 'python test.py');


--
-- Data for Name: configuration; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.configuration VALUES (1, 1, 'Automatic', 'SELECT * FROM record WHERE quantity < 0 ORDER BY quantity ASC, text ASC, id ASC', 'verbose', '{}', '{"text": 150, "view": 100}', '{"record": "SELECT * FROM RECORD ORDER BY quantity ASC, text ASC, id ASC", "frequency": "SELECT * FROM frequency ORDER BY record_id ASC"}');


--
-- Data for Name: frequency; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.frequency VALUES (10, 1, NULL, 0, 1, 0, '2024-03-22 00:00:00+00', NULL);


--
-- Data for Name: record; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.record VALUES (63, 0, 'v0.5.0 Transfer proposal and connection, i.e "A proposal of transfering a quantity from A to B, in return (or not) C receives some from D" "If you transfer an amount of apples to my apple registration, I will transfer so much money from my registration to yours. Contribution and Retribution (optional if it is a donation)."', NULL);
INSERT INTO public.record VALUES (64, 0, 'v0.5.0 Transferência múltipla, i.e "Entregar diversos itens por um só. Para comprar essa bota eu ofereço 20 reais e um candelabro".', NULL);
INSERT INTO public.record VALUES (65, 0, 'v0.5.0 Eventually consistent', NULL);
INSERT INTO public.record VALUES (66, 0, 'v0.5.0 Decentralization', NULL);
INSERT INTO public.record VALUES (67, 0, 'v0.5.0 Eventually consistent databases', NULL);
INSERT INTO public.record VALUES (68, 0, 'v0.5.0 pub/sub protocols, i.e. "Pessoas podem se inscrever com pub/sub com cada cadastro, referente ao assunto escolhido."', NULL);
INSERT INTO public.record VALUES (69, 0, 'v0.5.0 Merkle-CRDTs implementation', NULL);
INSERT INTO public.record VALUES (70, 0, 'v0.5.0 Authentication. # Check gajim for login inspiration.', NULL);
INSERT INTO public.record VALUES (71, 0, 'v0.5.0 É possível utilizar IPFS e libp2p, diversos outras ferramentas pra auxiliar no processo de compartilhamento de cadastros, condições e transferências', NULL);
INSERT INTO public.record VALUES (72, 0, 'v0.5.0 Fazer com que as pessoas possam utilizar máquinas que tem controle para aliviar o tráfego em certos pontos da rede e permitir mais em geral. Não necessariamente as que elas possuem.', NULL);
INSERT INTO public.record VALUES (73, 0, 'v0.6.0 Browser version, with dropdowns and buttons and pages and erethang', NULL);
INSERT INTO public.record VALUES (74, 0, 'v0.6.0 The registration can be done by typing, voice, photo or video, making it accessible and easy to use. For those without access to technology, it is possible to add their needs and contributions through any device or party.', NULL);
INSERT INTO public.record VALUES (75, 0, 'v0.7.0 Sugestão de Enriquecimento de cards, a pessoa coloca um prompt/foto do NI/CE e um MMLLM adiciona metadata, dps a pessoa confirma.', NULL);
INSERT INTO public.record VALUES (76, 0, 'v0.7.0 Algorithm and/or ML for optimization of transfer quotas and cost-efficient connections. By digitizing the information, Lince enables the use of optimization algorithms and machine learning for more effective planning of contributions. However, it is necessary to consider the human biases present in the algorithms and ensure transparency, consent and democracy when implementing any decision-making artificial intelligence. Lince Modelo/Template.', NULL);
INSERT INTO public.record VALUES (86, 0, 'v0.4.0 Feature | When creating checkpoint, remove when_done from frequency, it is a checkpoint.', NULL);
INSERT INTO public.record VALUES (78, 1, 'v0.3.0 Bug: when query select * from table <table> not returning rows', NULL);
INSERT INTO public.record VALUES (60, 1, 'v0.3.0 config filter on what tables', NULL);
INSERT INTO public.record VALUES (55, 1, 'v0.3.0 config save mode', NULL);
INSERT INTO public.record VALUES (59, 1, 'v0.3.0 config sort', NULL);
INSERT INTO public.record VALUES (58, 1, 'v0.3.0 config menu options shown and their disposition on the screen and columns..', NULL);
INSERT INTO public.record VALUES (56, 1, 'v0.3.0 config tables show at main view', NULL);
INSERT INTO public.record VALUES (54, 1, 'v0.3.0 Criar table config, vai conter as configurações do aplicativo, quando o aplicativo começar ele vai olhar todas as colunas e se comportar conforme as informações, cada linha dessa table é um perfil, sempre há o perfil atual, com quantidade tal, e outros com outra quantidade. essa table sempre tem uma linha com as configs padrão.', NULL);
INSERT INTO public.record VALUES (80, 1, 'v0.3.0 | Fix order of main functions so startup isnt janky, things look different after you go to the while loop, menu & stuff', NULL);
INSERT INTO public.record VALUES (52, 1, 'v0.3.0 | Templates: prov uma manifestação de uso. Pessoas vão modelar sua cadeia produtiva e mostrar ou não, as que mostrarem vão deixar pessoas copiar tudo. Depois o que sobra são trabalhadores se colocando nas tarefas pra levantar tal empreitada.', NULL);
INSERT INTO public.record VALUES (79, 1, 'v0.3.0 | Bug: capital F not activating SQL File option', NULL);
INSERT INTO public.record VALUES (62, 1, 'v0.3.0 | config change architecture main. reads from config table and passes info as argument to functions', NULL);
INSERT INTO public.record VALUES (61, 1, 'v0.3.0 | config view selected config options', NULL);
INSERT INTO public.record VALUES (57, 1, 'v0.3.0 | config views', NULL);
INSERT INTO public.record VALUES (50, 1, 'v0.3.0 | make quantity zero with minimum types of keyboard', NULL);
INSERT INTO public.record VALUES (81, 1, 'v0.3.0 | Enhancement | Make all tables that should have quantity inherit a id and quantity table.', NULL);
INSERT INTO public.record VALUES (82, 1, 'v0.3.0 | Feature | Make all consequences have quantity, inheriting id+quantity table. Make a pack of consequences have multiple possibilities for retiring. Different bottlenecks, more limitations for something to happen, more precision on modeling Ns & Cs.', NULL);
INSERT INTO public.record VALUES (51, 1, 'v0.3.0 | Feature | Add help in the format of a complete text as an operation', NULL);
INSERT INTO public.record VALUES (83, 1, 'v0.3.0 | Feature | Configuration: add information in different quantities, may be nothing at all, to columns when filling them. Read from configuration table active row. Also to operations when performing them in any way.', NULL);
INSERT INTO public.record VALUES (77, 1, 'v0.3.0 | Feature | Configuration: truncate certain columns in certain tables in accordance with truncate column in configuration table.', NULL);
INSERT INTO public.record VALUES (42, 0, 'v0.4.0 | Checkpoint, i.e. "When a quantity reaches 4"', NULL);
INSERT INTO public.record VALUES (48, 0, 'v0.4.0 | Transform frequency into a building block, to be used just like any condition/consequence', NULL);
INSERT INTO public.record VALUES (44, 0, 'v0.4.0 | Proportion, i.e. "When a quantity changes a certain number"', NULL);
INSERT INTO public.record VALUES (43, 0, 'v0.4.0 | Rate, i.e. "When a quantity changes in a certain rate (change/time)"', NULL);
INSERT INTO public.record VALUES (46, 0, 'v0.4.0 | Delta, i.e. "Set a quantity to more or less a number, -1, +4, etc."', NULL);
INSERT INTO public.record VALUES (47, 0, 'v0.4.0 | Arquitetura de condições e consequências', NULL);
INSERT INTO public.record VALUES (87, 0, 'v0.4.0 | Enhancement | Remove title column if possible, leave only description, put stuff of the title in the description column. Have an easy way of changing view, maybe by sql searches, but could be a better one.', NULL);
INSERT INTO public.record VALUES (41, 0, 'v0.4.0 | Conditions (The objective is to have generalized conditions and consequences, so anything can trigger anything else.\n A periodicity can run a script, a checkpoint can change a quantity through a proportion, etc.)', NULL);
INSERT INTO public.record VALUES (84, -2, 'Backlog | Bug | When starting two different instances of lince the server has problems, no pg_dump, weird messages.', NULL);
INSERT INTO public.record VALUES (92, -1, 'Bug | Frequency gets the hours, minutes and so on, erased, always at 00:00, that is not ideal because of the seconds feature', NULL);
INSERT INTO public.record VALUES (45, 0, 'v0.4.0 | Command, i.e. "Shell command, being able to trigger any script in any language, easy to do with nix-shells for dev envs"', NULL);
INSERT INTO public.record VALUES (89, 0, '	v0.4.0 | Feature | Make record text have in it a function, it takes ids and tables in a code Record Quantity with id 20 is rq20, so rq20 = rq40 equals the two quantities. rq10 = rq20.f30 if frequency with id 30 returns a 1, it makes rq10 be rq20. c32 = f324, every how many times, a command is run. rq43 = f45.c56. in that frequency, make rq43 have the value returned by the command.', NULL);
INSERT INTO public.record VALUES (95, -1, 'v0.4.0 | Enhancement | Make help more helpful, explain the whole deal.', NULL);
INSERT INTO public.record VALUES (49, 0, 'v0.5.0 | Feature | Wheel of Time: simulate passage of time, to see the quantities changing. Be able see the result of a transfer. Tip: study Operational Research.', NULL);


--
-- Data for Name: history; Type: TABLE DATA; Schema: public; Owner: -
--



--
-- Data for Name: karma; Type: TABLE DATA; Schema: public; Owner: -
--



--
-- Data for Name: transfer; Type: TABLE DATA; Schema: public; Owner: -
--



--
-- Name: command_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.command_id_seq', 4, true);


--
-- Name: configuration_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.configuration_id_seq', 1, false);


--
-- Name: frequency_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.frequency_id_seq', 10, true);


--
-- Name: history_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.history_id_seq', 1, false);


--
-- Name: karma_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.karma_id_seq', 18, true);


--
-- Name: record_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.record_id_seq', 95, true);


--
-- Name: transfer_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.transfer_id_seq', 1, false);


--
-- PostgreSQL database dump complete
--

