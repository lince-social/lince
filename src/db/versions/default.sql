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



--
-- Data for Name: configuration; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.configuration VALUES (1, 1, 'Automatic', 'SELECT * FROM record WHERE quantity < 0 ORDER BY quantity ASC, head ASC, body ASC, id ASC', 'verbose', '{}', '{"body": 150, "view": 100}', '{"record": "SELECT * FROM RECORD ORDER BY quantity ASC, head ASC, body ASC, id ASC", "frequency": "SELECT * FROM frequency ORDER BY id ASC"}', 'en-US', '-3', 'default', 'default');


--
-- Data for Name: frequency; Type: TABLE DATA; Schema: public; Owner: -
--



--
-- Data for Name: record; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.record VALUES (106, -1, 'v0.4.1', 'Enhancement | LIKE Expression: alter the quantity of any record that has body or header LIKE some part of the string, regex? postgres LIKE?', NULL);
INSERT INTO public.record VALUES (73, 0, '', 'Feature | Browser Version: 1. Dropdowns to choose. 2. Buttons. 3. Pages...', NULL);
INSERT INTO public.record VALUES (74, 0, '', 'Feature | Maximization of Value Architecture: Create an architecture that allows for any amount of receptables of models and rules that alter the functioning of the app.', NULL);
INSERT INTO public.record VALUES (75, 0, '', 'Feature | Acessibility: Get information through any medium and turn them into any operation (Any language, verbal, sign, whistled..).', NULL);
INSERT INTO public.record VALUES (76, 0, '', 'Feature | Optimization: get recommendations or automatically optimize all tables and execute actions. Make the text pretty and formated. The transfer proposals in accordance with social quotas, and cost/benefit, automatic transfer with highest one, Lince agent. Correct frequencies for records and commands. Script correction...', NULL);
INSERT INTO public.record VALUES (49, 0, '', 'Feature | Wheel of Time: simulate passage of time, to see the quantities changing. Be able see the result of a transfer. Tip: study Operational Research.', NULL);
INSERT INTO public.record VALUES (63, 0, '', 'Feature | Transfer: 1. Multiple parties make their proposal. Each part can receive and contribute many things to any party. 2. Every proposal is accepted. 3. Every party marks deal as uphold. Tip: study smart contracts.', NULL);
INSERT INTO public.record VALUES (65, 0, '', 'Feature | Eventually Consistent Databases.', NULL);
INSERT INTO public.record VALUES (66, 0, '', 'Feature | Decentralization: have different nodes of Lince that can communicate.', NULL);
INSERT INTO public.record VALUES (68, 0, '', 'Feature | Communication: between nodes, maybe use pub/sub protocol, IPFS, libp2p, Merkle-CRDTs.', NULL);
INSERT INTO public.record VALUES (72, 0, '', 'Feature | Computing Donation: Give the agent the option to make the machine use its resources for network traffic optimization. Whatever that means.', NULL);
INSERT INTO public.record VALUES (70, 0, '', 'Feature | Authentication. Tip: Check gajim for possible login inspiration.', NULL);
INSERT INTO public.record VALUES (102, 0, '', 'Enhancement| DB Versions: have different DBs, in a dir inside db, i.e. db/versions/, change postgre.sql to schema.sql, have an option to change loaded db or load default.sql', NULL);
INSERT INTO public.record VALUES (104, 0, '', 'Feature | Streaming: be able to stream video and/or audio through a p2p connection. Be part of a karma expression.', NULL);
INSERT INTO public.record VALUES (107, 0, '', 'Feature | Have the app know where your machine is located on the world', NULL);
INSERT INTO public.record VALUES (108, 0, '', 'Feature | Default Location: When doing transfers the default location is where the machine is at the moment, only not when the location is filled with a coordinate.', NULL);
INSERT INTO public.record VALUES (109, 1, '', 'Feature | Graph View: See dependent records and their triggers through karma, with its dependencies like commands and frequencies. Also view dependencies with other records from other nodes, see the chain, sypply chain.', NULL);
INSERT INTO public.record VALUES (110, -1, 'v0.4.1', 'Feature | Views: select one or multiple views to see in one view', NULL);
INSERT INTO public.record VALUES (105, -1, 'v0.4.1', 'Enhancement | Easy CRUD: be able to perform crud operations on any table through karma expressions and menu typying.', NULL);


--
-- Data for Name: history; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.history VALUES (118, 49, '2024-08-01 04:34:54.530641+00', 0, -1);
INSERT INTO public.history VALUES (119, 63, '2024-08-01 04:34:54.530641+00', 0, -1);
INSERT INTO public.history VALUES (120, 65, '2024-08-01 04:34:54.530641+00', 0, -1);
INSERT INTO public.history VALUES (121, 66, '2024-08-01 04:34:54.530641+00', 0, -1);
INSERT INTO public.history VALUES (122, 68, '2024-08-01 04:34:54.530641+00', 0, -1);
INSERT INTO public.history VALUES (123, 70, '2024-08-01 04:34:54.530641+00', 0, -1);
INSERT INTO public.history VALUES (124, 72, '2024-08-01 04:34:54.530641+00', 0, -1);
INSERT INTO public.history VALUES (126, 70, '2024-08-02 13:34:04.757129+00', -1, -2);
INSERT INTO public.history VALUES (128, 70, '2024-08-02 13:34:08.206153+00', -2, -3);
INSERT INTO public.history VALUES (130, 70, '2024-08-02 13:34:08.703126+00', -3, -4);
INSERT INTO public.history VALUES (132, 70, '2024-08-02 13:34:08.935131+00', -4, -5);
INSERT INTO public.history VALUES (134, 70, '2024-08-02 13:39:32.880783+00', -5, -6);
INSERT INTO public.history VALUES (136, 70, '2024-08-02 13:39:35.80497+00', -6, -7);
INSERT INTO public.history VALUES (138, 70, '2024-08-02 13:39:35.990641+00', -7, -8);
INSERT INTO public.history VALUES (140, 70, '2024-08-02 13:39:36.16883+00', -8, -9);
INSERT INTO public.history VALUES (142, 70, '2024-08-02 13:40:12.332949+00', -9, -10);
INSERT INTO public.history VALUES (144, 70, '2024-08-02 13:40:13.380425+00', -10, -11);
INSERT INTO public.history VALUES (145, 70, '2024-08-02 13:40:45.751341+00', -11, -1);
INSERT INTO public.history VALUES (146, 105, '2024-08-06 23:41:27.063145+00', 1, -1);
INSERT INTO public.history VALUES (147, 49, '2024-08-09 02:10:20.704595+00', -1, 0);
INSERT INTO public.history VALUES (148, 63, '2024-08-09 02:10:20.704595+00', -1, 0);
INSERT INTO public.history VALUES (149, 65, '2024-08-09 02:10:20.704595+00', -1, 0);
INSERT INTO public.history VALUES (150, 66, '2024-08-09 02:10:20.704595+00', -1, 0);
INSERT INTO public.history VALUES (151, 68, '2024-08-09 02:10:20.704595+00', -1, 0);
INSERT INTO public.history VALUES (152, 72, '2024-08-09 02:10:20.704595+00', -1, 0);
INSERT INTO public.history VALUES (153, 70, '2024-08-09 02:10:20.704595+00', -1, 0);
INSERT INTO public.history VALUES (154, 102, '2024-08-09 02:10:20.704595+00', -1, 0);
INSERT INTO public.history VALUES (155, 104, '2024-08-09 02:10:20.704595+00', 1, 0);
INSERT INTO public.history VALUES (156, 107, '2024-08-09 02:10:20.704595+00', 1, 0);
INSERT INTO public.history VALUES (157, 108, '2024-08-09 02:10:20.704595+00', 1, 0);
INSERT INTO public.history VALUES (159, 105, '2024-08-09 02:10:20.704595+00', -1, 0);
INSERT INTO public.history VALUES (160, 106, '2024-08-09 02:10:20.704595+00', -1, 0);
INSERT INTO public.history VALUES (162, 105, '2024-08-09 02:10:34.914652+00', 0, -1);
INSERT INTO public.history VALUES (163, 106, '2024-08-09 02:10:34.914652+00', 0, -1);


--
-- Data for Name: karma; Type: TABLE DATA; Schema: public; Owner: -
--



--
-- Data for Name: sum; Type: TABLE DATA; Schema: public; Owner: -
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

SELECT pg_catalog.setval('public.configuration_id_seq', 3, true);


--
-- Name: frequency_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.frequency_id_seq', 18, true);


--
-- Name: history_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.history_id_seq', 163, true);


--
-- Name: karma_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.karma_id_seq', 30, true);


--
-- Name: record_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.record_id_seq', 110, true);


--
-- Name: sum_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.sum_id_seq', 2, true);


--
-- Name: transfer_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.transfer_id_seq', 1, false);


--
-- PostgreSQL database dump complete
--

