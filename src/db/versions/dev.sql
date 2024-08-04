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

INSERT INTO public.configuration VALUES (1, 1, 'Automatic', 'SELECT * FROM record WHERE quantity < 0 ORDER BY quantity ASC, text ASC, id ASC', 'verbose', '{}', '{"text": 150, "view": 100}', '{"record": "SELECT * FROM RECORD ORDER BY quantity ASC, text ASC, id ASC", "frequency": "SELECT * FROM frequency ORDER BY id ASC"}', 'en-US', '-3', 'default', 'default');


--
-- Data for Name: frequency; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.frequency VALUES (18, 1, NULL, 0, 0, 2, '2024-08-01 04:06:18.374205+00', NULL);
INSERT INTO public.frequency VALUES (17, 1, NULL, 0, 1, 0, '2024-08-04 19:56:56.08274+00', NULL);


--
-- Data for Name: record; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.record VALUES (73, 0, 'v0.6.0 | Feature | Browser Version: 1. Dropdowns to choose. 2. Buttons. 3. Pages...', NULL);
INSERT INTO public.record VALUES (74, 0, 'v0.7.0 | Feature | Maximization of Value Architecture: Create an architecture that allows for any amount of receptables of models and rules that alter the functioning of the app.', NULL);
INSERT INTO public.record VALUES (75, 0, 'v0.8.0 | Feature | Acessibility: Get information through any medium and turn them into any operation (Any language, verbal, sign, whistled..).', NULL);
INSERT INTO public.record VALUES (76, 0, 'v0.9.0 | Feature | Optimization: get recommendations or automatically optimize all tables and execute actions. Make the text pretty and formated. The transfer proposals in accordance with social quotas, and cost/benefit, automatic transfer with highest one, Lince agent. Correct frequencies for records and commands. Script correction...', NULL);
INSERT INTO public.record VALUES (49, -1, 'v0.5.0 | Feature | Wheel of Time: simulate passage of time, to see the quantities changing. Be able see the result of a transfer. Tip: study Operational Research.', NULL);
INSERT INTO public.record VALUES (63, -1, 'v0.5.0 | Feature | Transfer: 1. Multiple parties make their proposal. Each part can receive and contribute many things to any party. 2. Every proposal is accepted. 3. Every party marks deal as uphold. Tip: study smart contracts.', NULL);
INSERT INTO public.record VALUES (65, -1, 'v0.5.0 | Feature | Eventually Consistent Databases.', NULL);
INSERT INTO public.record VALUES (66, -1, 'v0.5.0 | Feature | Decentralization: have different nodes of Lince that can communicate.', NULL);
INSERT INTO public.record VALUES (68, -1, 'v0.5.0 | Feature | Communication: between nodes, maybe use pub/sub protocol, IPFS, libp2p, Merkle-CRDTs.', NULL);
INSERT INTO public.record VALUES (72, -1, 'v0.5.0 | Feature | Computing Donation: Give the agent the option to make the machine use its resources for network traffic optimization. Whatever that means.', NULL);
INSERT INTO public.record VALUES (70, -1, 'v0.5.0 | Feature | Authentication. Tip: Check gajim for possible login inspiration.', NULL);
INSERT INTO public.record VALUES (102, -1, 'v0.4.1 | Enhancement| DB Versions: have different DBs, in a dir inside db, i.e. db/versions/, change postgre.sql to schema.sql, have an option to change loaded db or load default.sql', NULL);
INSERT INTO public.record VALUES (103, -1, 'v0.4.1 | Enhancement | Be able to change quantity of a record with a simple command, like 1q70 makes rq70 = 1, typying on the Your Choice:', NULL);


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
INSERT INTO public.history VALUES (146, 103, '2024-08-03 23:03:04.010398+00', -1, -2);
INSERT INTO public.history VALUES (147, 103, '2024-08-03 23:03:04.759165+00', -2, -3);
INSERT INTO public.history VALUES (148, 103, '2024-08-03 23:03:05.032706+00', -3, -4);
INSERT INTO public.history VALUES (149, 103, '2024-08-03 23:03:05.222668+00', -4, -5);
INSERT INTO public.history VALUES (150, 103, '2024-08-03 23:03:05.405022+00', -5, -6);
INSERT INTO public.history VALUES (151, 103, '2024-08-03 23:03:05.615221+00', -6, -7);
INSERT INTO public.history VALUES (152, 103, '2024-08-03 23:03:05.814881+00', -7, -8);
INSERT INTO public.history VALUES (153, 103, '2024-08-03 23:03:06.02742+00', -8, -9);
INSERT INTO public.history VALUES (154, 103, '2024-08-03 23:03:06.236871+00', -9, -10);
INSERT INTO public.history VALUES (155, 103, '2024-08-03 23:03:06.430141+00', -10, -11);
INSERT INTO public.history VALUES (156, 103, '2024-08-03 23:03:06.658251+00', -11, -12);
INSERT INTO public.history VALUES (157, 103, '2024-08-03 23:03:06.872197+00', -12, -13);
INSERT INTO public.history VALUES (158, 103, '2024-08-03 23:03:07.072434+00', -13, -14);
INSERT INTO public.history VALUES (159, 103, '2024-08-03 23:03:07.226994+00', -14, -15);
INSERT INTO public.history VALUES (160, 103, '2024-08-03 23:03:07.383101+00', -15, -16);
INSERT INTO public.history VALUES (161, 103, '2024-08-03 23:03:09.788822+00', -16, -17);
INSERT INTO public.history VALUES (162, 103, '2024-08-03 23:03:10.140642+00', -17, -18);
INSERT INTO public.history VALUES (163, 103, '2024-08-03 23:03:10.55868+00', -18, -19);
INSERT INTO public.history VALUES (164, 103, '2024-08-03 23:03:41.732943+00', -19, -20);
INSERT INTO public.history VALUES (165, 103, '2024-08-03 23:04:25.517682+00', -20, -21);
INSERT INTO public.history VALUES (166, 103, '2024-08-03 23:04:28.618904+00', -21, -22);
INSERT INTO public.history VALUES (167, 103, '2024-08-03 23:04:28.864718+00', -22, -23);
INSERT INTO public.history VALUES (168, 103, '2024-08-03 23:04:29.102374+00', -23, -1);


--
-- Data for Name: karma; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.karma VALUES (32, 1, 'rq103 = f17*rq103 -1');


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

SELECT pg_catalog.setval('public.configuration_id_seq', 1, false);


--
-- Name: frequency_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.frequency_id_seq', 18, true);


--
-- Name: history_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.history_id_seq', 168, true);


--
-- Name: karma_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.karma_id_seq', 32, true);


--
-- Name: record_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.record_id_seq', 103, true);


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

