--
-- PostgreSQL database dump
--

-- Dumped from database version 16.4
-- Dumped by pg_dump version 16.4

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

INSERT INTO public.command VALUES (7, 1, 'xterm -e nmtui');


--
-- Data for Name: configuration; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.configuration VALUES (4, 0, 'Automatic', 1, 'verbose', '{}', '{"body": 150, "view": 100}', '{"record": "SELECT * FROM RECORD ORDER BY quantity ASC, head ASC, body ASC, id ASC", "frequency": "SELECT * FROM frequency ORDER BY id ASC"}', 'en-US', '-3', 'default', 'default');
INSERT INTO public.configuration VALUES (5, 1, 'Automatic', 2, 'verbose', '{}', '{"body": 100, "view": 100}', '{"record": "SELECT * FROM RECORD ORDER BY quantity ASC, head ASC, body ASC, id ASC", "frequency": "SELECT * FROM frequency ORDER BY id ASC"}', 'en-US', '-3', 'default', 'default');


--
-- Data for Name: frequency; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.frequency VALUES (21, 1, 1, 0, 0, 0, '2024-10-21 03:45:29.523624+00', NULL);


--
-- Data for Name: history; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.history VALUES (266, 119, '2024-10-19 14:36:08.683133+00', 1, 0);
INSERT INTO public.history VALUES (267, 119, '2024-10-19 14:36:21.916675+00', 0, 1);
INSERT INTO public.history VALUES (268, 119, '2024-10-19 14:37:57.449454+00', 1, 0);
INSERT INTO public.history VALUES (269, 119, '2024-10-19 14:41:51.23635+00', 0, 1);
INSERT INTO public.history VALUES (270, 119, '2024-10-19 14:42:02.476993+00', 1, 0);
INSERT INTO public.history VALUES (271, 119, '2024-10-19 14:42:02.687655+00', 0, 1);
INSERT INTO public.history VALUES (272, 119, '2024-10-19 14:42:25.808329+00', 1, 0);
INSERT INTO public.history VALUES (273, 119, '2024-10-19 14:42:31.170267+00', 0, 1);
INSERT INTO public.history VALUES (274, 119, '2024-10-19 14:42:49.207153+00', 1, 0);
INSERT INTO public.history VALUES (275, 119, '2024-10-19 14:43:32.606656+00', 0, 1);
INSERT INTO public.history VALUES (276, 119, '2024-10-19 14:43:36.706065+00', 1, 0);
INSERT INTO public.history VALUES (277, 119, '2024-10-19 14:43:52.511692+00', 0, 1);
INSERT INTO public.history VALUES (278, 119, '2024-10-19 14:44:04.274655+00', 1, 0);
INSERT INTO public.history VALUES (279, 119, '2024-10-19 14:44:59.567926+00', 0, 1);
INSERT INTO public.history VALUES (280, 119, '2024-10-19 14:57:02.942306+00', 1, 0);


--
-- Data for Name: karma; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.karma VALUES (37, 1, 'rq118 = f21 * -1');
INSERT INTO public.karma VALUES (38, 1, 'rq119,c7 = (rq119 == 0) * 1');


--
-- Data for Name: record; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.record VALUES (73, 0, '', 'Feature | Browser Version: 1. Dropdowns to choose. 2. Buttons. 3. Pages...', NULL);
INSERT INTO public.record VALUES (74, 0, '', 'Feature | Maximization of Value Architecture: Create an architecture that allows for any amount of receptables of models and rules that alter the functioning of the app.', NULL);
INSERT INTO public.record VALUES (75, 0, '', 'Feature | Acessibility: Get information through any medium and turn them into any operation (Any language, verbal, sign, whistled..).', NULL);
INSERT INTO public.record VALUES (76, 0, '', 'Feature | Optimization: get recommendations or automatically optimize all tables and execute actions. Make the text pretty and formated. The transfer proposals in accordance with social quotas, and cost/benefit, automatic transfer with highest one, Lince agent. Correct frequencies for records and commands. Script correction...', NULL);
INSERT INTO public.record VALUES (49, 0, '', 'Feature | Wheel of Time: simulate passage of time, to see the quantities changing. Be able see the result of a transfer. Tip: study Operational Research.', NULL);
INSERT INTO public.record VALUES (63, 0, '', 'Feature | Transfer: 1. Multiple parties make their proposal. Each part can receive and contribute many things to any party. 2. Every proposal is accepted. 3. Every party marks deal as uphold. Tip: study smart contracts.', NULL);
INSERT INTO public.record VALUES (72, 0, '', 'Feature | Computing Donation: Give the agent the option to make the machine use its resources for network traffic optimization. Whatever that means.', NULL);
INSERT INTO public.record VALUES (102, 0, '', 'Enhancement| DB Versions: have different DBs, in a dir inside db, i.e. db/versions/, change postgre.sql to schema.sql, have an option to change loaded db or load default.sql', NULL);
INSERT INTO public.record VALUES (104, 0, '', 'Feature | Streaming: be able to stream video and/or audio through a p2p connection. Be part of a karma expression.', NULL);
INSERT INTO public.record VALUES (107, 0, '', 'Feature | Have the app know where your machine is located on the world. Datatype: DEFAULT (59.880220, -43.732561)', NULL);
INSERT INTO public.record VALUES (108, 0, '', 'Feature | Default Location: When doing transfers the default location is where the machine is at the moment, only not when the location is filled with a coordinate.', NULL);
INSERT INTO public.record VALUES (109, 0, '', 'Feature | Graph View: See dependent records and their triggers through karma, with its dependencies like commands and frequencies. Also view dependencies with other records from other nodes, see the chain, sypply chain.', NULL);
INSERT INTO public.record VALUES (68, -1, '', 'Feature | Communication: between nodes, maybe use pub/sub protocol, IPFS, libp2p, Merkle-CRDTs.', NULL);
INSERT INTO public.record VALUES (65, -58, '', 'Feature | Eventually Consistent Databases.', NULL);
INSERT INTO public.record VALUES (116, -1, 'Stop command execution at startup', 'Bugfix | Steps: 1. Open Lince. Tip: bug seems to not happen if you do a change in the db that doesnt cause a code execution, like zeroing a commandless record', NULL);
INSERT INTO public.record VALUES (117, -1, 'Command be able to use both os.sys and Output', NULL, NULL);
INSERT INTO public.record VALUES (118, -1, 'fix timezone', NULL, NULL);
INSERT INTO public.record VALUES (119, 0, 'wifi', NULL, NULL);


--
-- Data for Name: sum; Type: TABLE DATA; Schema: public; Owner: -
--



--
-- Data for Name: transfer; Type: TABLE DATA; Schema: public; Owner: -
--



--
-- Data for Name: views; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.views VALUES (1, 'SELECT * FROM record');
INSERT INTO public.views VALUES (2, 'SELECT * FROM record WHERE quantity < 0 ORDER BY quantity ASC, head ASC, body ASC');


--
-- Name: command_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.command_id_seq', 7, true);


--
-- Name: configuration_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.configuration_id_seq', 5, true);


--
-- Name: frequency_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.frequency_id_seq', 21, true);


--
-- Name: history_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.history_id_seq', 280, true);


--
-- Name: karma_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.karma_id_seq', 38, true);


--
-- Name: record_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.record_id_seq', 119, true);


--
-- Name: sum_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.sum_id_seq', 2, true);


--
-- Name: transfer_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.transfer_id_seq', 1, false);


--
-- Name: views_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.views_id_seq', 2, true);


--
-- PostgreSQL database dump complete
--

