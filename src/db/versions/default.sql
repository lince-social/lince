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

INSERT INTO public.command VALUES (5, 1, 'python aaa.py');
INSERT INTO public.command VALUES (6, 1, 'touch teste.md');


--
-- Data for Name: configuration; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.configuration VALUES (4, 0, 'Automatic', 1, 'verbose', '{}', '{"body": 150, "view": 100}', '{"record": "SELECT * FROM RECORD ORDER BY quantity ASC, head ASC, body ASC, id ASC", "frequency": "SELECT * FROM frequency ORDER BY id ASC"}', 'en-US', '-3', 'default', 'default');
INSERT INTO public.configuration VALUES (5, 1, 'Automatic', 2, 'verbose', '{}', '{"body": 150, "view": 100}', '{"record": "SELECT * FROM RECORD ORDER BY quantity ASC, head ASC, body ASC, id ASC", "frequency": "SELECT * FROM frequency ORDER BY id ASC"}', 'en-US', '-3', 'default', 'default');


--
-- Data for Name: frequency; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.frequency VALUES (19, 1, NULL, 0, 1, 0, '2024-08-15 00:00:00+00', NULL);


--
-- Data for Name: history; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.history VALUES (166, 109, '2024-08-11 23:32:08.231529+00', 1, 0);
INSERT INTO public.history VALUES (167, 65, '2024-08-11 23:46:09.80722+00', 0, -1);
INSERT INTO public.history VALUES (168, 66, '2024-08-11 23:46:09.80722+00', 0, -1);
INSERT INTO public.history VALUES (169, 70, '2024-08-11 23:46:09.80722+00', 0, -1);
INSERT INTO public.history VALUES (170, 68, '2024-08-11 23:46:27.061796+00', 0, -1);
INSERT INTO public.history VALUES (171, 70, '2024-08-12 23:29:47.623516+00', -1, -3);
INSERT INTO public.history VALUES (172, 70, '2024-08-14 17:01:33.76144+00', -3, -4);
INSERT INTO public.history VALUES (173, 70, '2024-08-14 17:01:34.460294+00', -4, -5);
INSERT INTO public.history VALUES (174, 70, '2024-08-14 17:01:34.673296+00', -5, -6);
INSERT INTO public.history VALUES (175, 70, '2024-08-14 17:01:34.867951+00', -6, -7);
INSERT INTO public.history VALUES (176, 70, '2024-08-14 17:01:35.067403+00', -7, -8);
INSERT INTO public.history VALUES (177, 70, '2024-08-14 17:01:35.264995+00', -8, -9);
INSERT INTO public.history VALUES (178, 70, '2024-08-14 17:01:35.460727+00', -9, -10);
INSERT INTO public.history VALUES (179, 70, '2024-08-14 17:01:35.656238+00', -10, -11);
INSERT INTO public.history VALUES (180, 70, '2024-08-14 17:01:35.852752+00', -11, -12);
INSERT INTO public.history VALUES (181, 70, '2024-08-14 17:01:36.046601+00', -12, -13);
INSERT INTO public.history VALUES (182, 70, '2024-08-14 17:01:36.243952+00', -13, -14);
INSERT INTO public.history VALUES (183, 70, '2024-08-14 17:01:36.442558+00', -14, -15);
INSERT INTO public.history VALUES (184, 70, '2024-08-14 17:01:36.640776+00', -15, -16);
INSERT INTO public.history VALUES (185, 70, '2024-08-14 17:01:36.83691+00', -16, -17);
INSERT INTO public.history VALUES (186, 70, '2024-08-14 17:01:38.178574+00', -17, -18);
INSERT INTO public.history VALUES (187, 70, '2024-08-14 17:02:00.676812+00', -18, -19);
INSERT INTO public.history VALUES (188, 70, '2024-08-14 17:02:23.420752+00', -19, -20);
INSERT INTO public.history VALUES (189, 70, '2024-08-14 17:02:26.950421+00', -20, -21);
INSERT INTO public.history VALUES (190, 70, '2024-08-14 17:02:36.495295+00', -21, -22);
INSERT INTO public.history VALUES (191, 70, '2024-08-14 17:02:44.463986+00', -22, -23);
INSERT INTO public.history VALUES (192, 70, '2024-08-14 17:02:49.057927+00', -23, -24);
INSERT INTO public.history VALUES (193, 70, '2024-08-14 17:03:01.942929+00', -24, -25);
INSERT INTO public.history VALUES (194, 70, '2024-08-14 17:03:23.153572+00', -25, -26);
INSERT INTO public.history VALUES (195, 70, '2024-08-14 17:03:24.015683+00', -26, -27);
INSERT INTO public.history VALUES (196, 70, '2024-08-14 17:03:24.574725+00', -27, -28);
INSERT INTO public.history VALUES (197, 70, '2024-08-14 17:03:25.070789+00', -28, -29);
INSERT INTO public.history VALUES (198, 70, '2024-08-14 17:03:45.289471+00', -29, -30);
INSERT INTO public.history VALUES (199, 70, '2024-08-14 17:04:19.192237+00', -30, -31);
INSERT INTO public.history VALUES (200, 70, '2024-08-14 17:04:30.78703+00', -31, -32);
INSERT INTO public.history VALUES (201, 70, '2024-08-14 17:04:39.399748+00', -32, -33);
INSERT INTO public.history VALUES (202, 70, '2024-08-14 17:04:42.639026+00', -33, -34);
INSERT INTO public.history VALUES (203, 70, '2024-08-14 17:05:13.81196+00', -34, -35);
INSERT INTO public.history VALUES (204, 70, '2024-08-14 17:05:14.510132+00', -35, -36);
INSERT INTO public.history VALUES (205, 70, '2024-08-14 17:05:15.223673+00', -36, -37);
INSERT INTO public.history VALUES (206, 70, '2024-08-14 17:05:29.909423+00', -37, -38);
INSERT INTO public.history VALUES (207, 70, '2024-08-14 17:06:00.74671+00', -38, -39);
INSERT INTO public.history VALUES (208, 70, '2024-08-14 17:06:15.017588+00', -39, -3);
INSERT INTO public.history VALUES (209, 70, '2024-08-14 17:06:15.032524+00', -3, -4);
INSERT INTO public.history VALUES (210, 65, '2024-08-14 17:06:15.07914+00', -1, -58);
INSERT INTO public.history VALUES (211, 70, '2024-08-14 17:06:26.37959+00', -4, -5);
INSERT INTO public.history VALUES (212, 70, '2024-08-14 17:06:26.945987+00', -5, -6);
INSERT INTO public.history VALUES (213, 70, '2024-08-14 17:06:27.214265+00', -6, -7);
INSERT INTO public.history VALUES (214, 70, '2024-08-14 17:06:27.460134+00', -7, -8);
INSERT INTO public.history VALUES (215, 70, '2024-08-14 17:06:27.701579+00', -8, -9);
INSERT INTO public.history VALUES (216, 70, '2024-08-14 17:06:27.914302+00', -9, -10);
INSERT INTO public.history VALUES (217, 70, '2024-08-14 17:07:36.473243+00', -10, -11);
INSERT INTO public.history VALUES (218, 70, '2024-08-14 17:07:45.028077+00', -11, -12);
INSERT INTO public.history VALUES (219, 70, '2024-08-14 17:07:51.246072+00', -12, -13);


--
-- Data for Name: karma; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.karma VALUES (31, 1, 'rq70 = f19 * -3');
INSERT INTO public.karma VALUES (32, 1, 'rq70 = rq70 -1');
INSERT INTO public.karma VALUES (33, 1, 'rq68 = c5');
INSERT INTO public.karma VALUES (34, 1, 'rq65 = f19 * -58');
INSERT INTO public.karma VALUES (35, 1, 'c6 = 1');


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
INSERT INTO public.record VALUES (70, -13, '', 'Feature | Authentication. Tip: Check gajim for possible login inspiration.', NULL);


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

SELECT pg_catalog.setval('public.command_id_seq', 6, true);


--
-- Name: configuration_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.configuration_id_seq', 5, true);


--
-- Name: frequency_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.frequency_id_seq', 19, true);


--
-- Name: history_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.history_id_seq', 219, true);


--
-- Name: karma_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.karma_id_seq', 35, true);


--
-- Name: record_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.record_id_seq', 113, true);


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

