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

INSERT INTO public.configuration VALUES (1, 1, 'Automatic', 'SELECT * FROM record WHERE quantity < 0 ORDER BY quantity ASC, text ASC, id ASC', 'verbose', '{}', '{"text": 150, "view": 100}', '{"record": "SELECT * FROM RECORD ORDER BY quantity ASC, text ASC, id ASC", "frequency": "SELECT * FROM frequency ORDER BY id ASC"}', 'en-US', '-3');


--
-- Data for Name: frequency; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.frequency VALUES (17, 1, NULL, 0, 1, 0, '2024-08-01 19:56:56.08274+00', NULL);


--
-- Data for Name: record; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.record VALUES (49, 0, 'v0.5.0 | Feature | Wheel of Time: simulate passage of time, to see the quantities changing. Be able see the result of a transfer. Tip: study Operational Research.', NULL);
INSERT INTO public.record VALUES (63, 0, 'v0.5.0 | Feature | Transfer: 1. Multiple parties make their proposal. Each part can receive and contribute many things to any party. 2. Every proposal is accepted. 3. Every party marks deal as uphold. Tip: study smart contracts.', NULL);
INSERT INTO public.record VALUES (65, 0, 'v0.5.0 | Feature | Eventually Consistent Databases.', NULL);
INSERT INTO public.record VALUES (66, 0, 'v0.5.0 | Feature | Decentralization: have different nodes of Lince that can communicate.', NULL);
INSERT INTO public.record VALUES (68, 0, 'v0.5.0 | Feature | Communication: between nodes, maybe use pub/sub protocol, IPFS, libp2p, Merkle-CRDTs.', NULL);
INSERT INTO public.record VALUES (70, 0, 'v0.5.0 | Feature | Authentication. Tip: Check gajim for possible login inspiration.', NULL);
INSERT INTO public.record VALUES (72, 0, 'v0.5.0 | Feature | Computing Donation: Give the agent the option to make the machine use its resources for network traffic optimization. Whatever that means.', NULL);
INSERT INTO public.record VALUES (73, 0, 'v0.6.0 | Feature | Browser Version: 1. Dropdowns to choose. 2. Buttons. 3. Pages...', NULL);
INSERT INTO public.record VALUES (74, 0, 'v0.7.0 | Feature | Maximization of Value Architecture: Create an architecture that allows for any amount of receptables of models and rules that alter the functioning of the app.', NULL);
INSERT INTO public.record VALUES (75, 0, 'v0.8.0 | Feature | Acessibility: Get information through any medium and turn them into any operation (Any language, verbal, sign, whistled..).', NULL);
INSERT INTO public.record VALUES (76, 0, 'v0.9.0 | Feature | Optimization: get recommendations or automatically optimize all tables and execute actions. Make the text pretty and formated. The transfer proposals in accordance with social quotas, and cost/benefit, automatic transfer with highest one, Lince agent. Correct frequencies for records and commands. Script correction...', NULL);
INSERT INTO public.record VALUES (98, -1, 'v0.4.0 | Enhancement | Rate: check record history and give back 1 or 0 if meets the rate of change. 1. Sum positive changes. 2. Sum negative changes. 3. Sum all changes, delta.', NULL);
INSERT INTO public.record VALUES (95, 0, 'v0.4.0 | Enhancement | Make help more helpful, explain the whole deal.', NULL);
INSERT INTO public.record VALUES (99, 0, 'Bugfix | Fix the but where i change a column datatype completely and the code doesnt follow it, weird, in this case its the view_mode of the tables in config', NULL);
INSERT INTO public.record VALUES (92, 0, 'v0.4.0 | Bugfix | Frequency gets the hours, minutes and so on, erased, always at 00:00, that is not ideal because of the seconds feature.', NULL);


--
-- Data for Name: history; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.history VALUES (4, 0, 95, '2024-07-31 13:52:03.363818+00');
INSERT INTO public.history VALUES (5, -2, 92, '2024-07-31 13:56:15.923303+00');
INSERT INTO public.history VALUES (6, 0, 99, '2024-07-31 15:49:03.91218+00');
INSERT INTO public.history VALUES (7, -3, 92, '2024-07-31 18:30:40.783208+00');
INSERT INTO public.history VALUES (8, -4, 92, '2024-07-31 19:24:01.856106+00');
INSERT INTO public.history VALUES (9, -5, 92, '2024-07-31 19:24:03.442374+00');
INSERT INTO public.history VALUES (10, -6, 92, '2024-07-31 19:24:08.059481+00');
INSERT INTO public.history VALUES (11, -7, 92, '2024-07-31 19:24:08.659857+00');
INSERT INTO public.history VALUES (12, -8, 92, '2024-07-31 19:24:08.815688+00');
INSERT INTO public.history VALUES (13, -9, 92, '2024-07-31 19:24:08.967507+00');
INSERT INTO public.history VALUES (14, -10, 92, '2024-07-31 19:24:09.117226+00');
INSERT INTO public.history VALUES (15, -11, 92, '2024-07-31 19:24:09.264599+00');
INSERT INTO public.history VALUES (16, -12, 92, '2024-07-31 19:24:09.416975+00');
INSERT INTO public.history VALUES (17, -13, 92, '2024-07-31 19:24:09.564048+00');
INSERT INTO public.history VALUES (18, -14, 92, '2024-07-31 19:24:09.712537+00');
INSERT INTO public.history VALUES (19, -15, 92, '2024-07-31 19:24:09.860227+00');
INSERT INTO public.history VALUES (20, -16, 92, '2024-07-31 19:24:10.009261+00');
INSERT INTO public.history VALUES (21, -17, 92, '2024-07-31 19:24:10.163454+00');
INSERT INTO public.history VALUES (22, -18, 92, '2024-07-31 19:24:10.311484+00');
INSERT INTO public.history VALUES (23, -19, 92, '2024-07-31 19:24:10.457397+00');
INSERT INTO public.history VALUES (24, -20, 92, '2024-07-31 19:24:10.606504+00');
INSERT INTO public.history VALUES (25, -21, 92, '2024-07-31 19:24:10.753538+00');
INSERT INTO public.history VALUES (26, -22, 92, '2024-07-31 19:24:10.89916+00');
INSERT INTO public.history VALUES (27, -23, 92, '2024-07-31 19:24:11.050576+00');
INSERT INTO public.history VALUES (28, -24, 92, '2024-07-31 19:24:11.198264+00');
INSERT INTO public.history VALUES (29, -25, 92, '2024-07-31 19:24:11.34583+00');
INSERT INTO public.history VALUES (30, -26, 92, '2024-07-31 19:24:11.493515+00');
INSERT INTO public.history VALUES (31, -27, 92, '2024-07-31 19:24:11.640459+00');
INSERT INTO public.history VALUES (32, -28, 92, '2024-07-31 19:24:11.786758+00');
INSERT INTO public.history VALUES (33, -29, 92, '2024-07-31 19:24:11.934511+00');
INSERT INTO public.history VALUES (34, -30, 92, '2024-07-31 19:24:12.09219+00');
INSERT INTO public.history VALUES (35, -31, 92, '2024-07-31 19:24:12.241135+00');
INSERT INTO public.history VALUES (36, -32, 92, '2024-07-31 19:24:12.388605+00');
INSERT INTO public.history VALUES (37, -33, 92, '2024-07-31 19:24:12.537343+00');
INSERT INTO public.history VALUES (38, -34, 92, '2024-07-31 19:24:12.687785+00');
INSERT INTO public.history VALUES (39, -35, 92, '2024-07-31 19:24:12.837661+00');
INSERT INTO public.history VALUES (40, -36, 92, '2024-07-31 19:24:12.987546+00');
INSERT INTO public.history VALUES (41, -37, 92, '2024-07-31 19:24:13.134896+00');
INSERT INTO public.history VALUES (42, -38, 92, '2024-07-31 19:24:13.282786+00');
INSERT INTO public.history VALUES (43, -39, 92, '2024-07-31 19:24:13.430284+00');
INSERT INTO public.history VALUES (44, -40, 92, '2024-07-31 19:24:13.577362+00');
INSERT INTO public.history VALUES (45, -41, 92, '2024-07-31 19:24:13.724838+00');
INSERT INTO public.history VALUES (46, -42, 92, '2024-07-31 19:24:13.871341+00');
INSERT INTO public.history VALUES (47, -43, 92, '2024-07-31 19:24:14.019413+00');
INSERT INTO public.history VALUES (48, -44, 92, '2024-07-31 19:24:14.165398+00');
INSERT INTO public.history VALUES (49, -45, 92, '2024-07-31 19:24:14.315253+00');
INSERT INTO public.history VALUES (50, -46, 92, '2024-07-31 19:24:14.468767+00');
INSERT INTO public.history VALUES (51, -47, 92, '2024-07-31 19:24:14.61526+00');
INSERT INTO public.history VALUES (52, -48, 92, '2024-07-31 19:24:14.764742+00');
INSERT INTO public.history VALUES (53, -49, 92, '2024-07-31 19:24:14.910365+00');
INSERT INTO public.history VALUES (54, -50, 92, '2024-07-31 19:24:15.057308+00');
INSERT INTO public.history VALUES (55, -51, 92, '2024-07-31 19:24:15.205113+00');
INSERT INTO public.history VALUES (56, -52, 92, '2024-07-31 19:24:15.353241+00');
INSERT INTO public.history VALUES (57, -53, 92, '2024-07-31 19:24:15.505027+00');
INSERT INTO public.history VALUES (58, -54, 92, '2024-07-31 19:24:15.654068+00');
INSERT INTO public.history VALUES (59, -55, 92, '2024-07-31 19:24:15.806467+00');
INSERT INTO public.history VALUES (60, -56, 92, '2024-07-31 19:24:15.95485+00');
INSERT INTO public.history VALUES (61, -57, 92, '2024-07-31 19:24:16.10159+00');
INSERT INTO public.history VALUES (62, -58, 92, '2024-07-31 19:24:16.258102+00');
INSERT INTO public.history VALUES (63, -59, 92, '2024-07-31 19:24:16.410455+00');
INSERT INTO public.history VALUES (64, -60, 92, '2024-07-31 19:24:16.557318+00');
INSERT INTO public.history VALUES (65, -61, 92, '2024-07-31 19:24:16.712427+00');
INSERT INTO public.history VALUES (66, -62, 92, '2024-07-31 19:24:16.864109+00');
INSERT INTO public.history VALUES (67, -63, 92, '2024-07-31 19:24:17.01604+00');
INSERT INTO public.history VALUES (68, -64, 92, '2024-07-31 19:24:17.871943+00');
INSERT INTO public.history VALUES (69, -65, 92, '2024-07-31 19:24:18.880262+00');
INSERT INTO public.history VALUES (70, -66, 92, '2024-07-31 19:24:19.871199+00');
INSERT INTO public.history VALUES (71, -67, 92, '2024-07-31 19:24:20.820555+00');
INSERT INTO public.history VALUES (72, -68, 92, '2024-07-31 19:24:21.781664+00');
INSERT INTO public.history VALUES (73, -69, 92, '2024-07-31 19:24:22.832833+00');
INSERT INTO public.history VALUES (74, -70, 92, '2024-07-31 19:24:23.800039+00');
INSERT INTO public.history VALUES (75, -71, 92, '2024-07-31 19:24:24.844024+00');
INSERT INTO public.history VALUES (76, -72, 92, '2024-07-31 19:24:25.797547+00');
INSERT INTO public.history VALUES (77, -73, 92, '2024-07-31 19:24:26.768264+00');
INSERT INTO public.history VALUES (78, -74, 92, '2024-07-31 19:24:27.834461+00');
INSERT INTO public.history VALUES (79, -75, 92, '2024-07-31 19:24:28.790983+00');
INSERT INTO public.history VALUES (80, -76, 92, '2024-07-31 19:24:29.829558+00');
INSERT INTO public.history VALUES (81, -77, 92, '2024-07-31 19:24:30.788224+00');
INSERT INTO public.history VALUES (82, -78, 92, '2024-07-31 19:24:31.843635+00');
INSERT INTO public.history VALUES (83, -79, 92, '2024-07-31 19:24:32.805486+00');
INSERT INTO public.history VALUES (84, -80, 92, '2024-07-31 19:24:34.861685+00');
INSERT INTO public.history VALUES (85, -81, 92, '2024-07-31 19:24:35.359031+00');
INSERT INTO public.history VALUES (86, -82, 92, '2024-07-31 19:24:35.804645+00');
INSERT INTO public.history VALUES (87, -83, 92, '2024-07-31 19:24:36.756436+00');
INSERT INTO public.history VALUES (88, -84, 92, '2024-07-31 19:24:37.921379+00');
INSERT INTO public.history VALUES (89, -85, 92, '2024-07-31 19:24:45.351147+00');
INSERT INTO public.history VALUES (90, 0, 92, '2024-07-31 19:24:56.499273+00');
INSERT INTO public.history VALUES (91, -1, 92, '2024-07-31 19:24:56.709142+00');
INSERT INTO public.history VALUES (92, 0, 92, '2024-07-31 19:55:56.299183+00');
INSERT INTO public.history VALUES (93, -1, 92, '2024-07-31 19:57:13.158437+00');
INSERT INTO public.history VALUES (94, 0, 92, '2024-07-31 19:57:26.051781+00');


--
-- Data for Name: karma; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.karma VALUES (27, 1, 'rq98=s2');


--
-- Data for Name: sum; Type: TABLE DATA; Schema: public; Owner: -
--

INSERT INTO public.sum VALUES (2, 1, 98, 0, 'relative', '2 days', NULL, '2024-08-01 02:05:39.143753+00');


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

SELECT pg_catalog.setval('public.frequency_id_seq', 17, true);


--
-- Name: history_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.history_id_seq', 94, true);


--
-- Name: karma_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.karma_id_seq', 27, true);


--
-- Name: record_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.record_id_seq', 99, true);


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

