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

COPY public.command (id, quantity, command) FROM stdin;
\.


--
-- Data for Name: configuration; Type: TABLE DATA; Schema: public; Owner: -
--

COPY public.configuration (id, quantity, save_mode, view_id, column_information_mode, keymap, truncation, table_query, language, timezone) FROM stdin;
1	1	Automatic	1	verbose	{}	{"body": 150, "head": 150, "view": 100, "command": 150}	{"record": "SELECT * FROM RECORD ORDER BY quantity ASC, head ASC, body ASC, id ASC", "command": "SELECT * FROM command ORDER BY id ASC", "frequency": "SELECT * FROM frequency ORDER BY id ASC"}	en-US	-3
\.


--
-- Data for Name: frequency; Type: TABLE DATA; Schema: public; Owner: -
--

COPY public.frequency (id, quantity, day_week, months, days, seconds, next_date, finish_date, catch_up_sum) FROM stdin;
\.


--
-- Data for Name: history; Type: TABLE DATA; Schema: public; Owner: -
--

COPY public.history (id, record_id, change_time, old_quantity, new_quantity) FROM stdin;
\.


--
-- Data for Name: karma; Type: TABLE DATA; Schema: public; Owner: -
--

COPY public.karma (id, quantity, expression) FROM stdin;
\.


--
-- Data for Name: record; Type: TABLE DATA; Schema: public; Owner: -
--

COPY public.record (id, quantity, head, body, location) FROM stdin;
\.


--
-- Data for Name: sum; Type: TABLE DATA; Schema: public; Owner: -
--

COPY public.sum (id, quantity, record_id, sum_mode, interval_mode, interval_length, end_lag, end_date) FROM stdin;
\.


--
-- Data for Name: transfer; Type: TABLE DATA; Schema: public; Owner: -
--

COPY public.transfer (id, records_received, records_contributed, receiving_agreement, contributing_agreement, agreement_time, receiving_transfer_confirmation, contributing_transfer_confirmation, transfer_time) FROM stdin;
\.


--
-- Data for Name: views; Type: TABLE DATA; Schema: public; Owner: -
--

COPY public.views (id, view, view_name) FROM stdin;
1	SELECT * FROM record	\N
\.


--
-- Name: command_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.command_id_seq', 1, false);


--
-- Name: configuration_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.configuration_id_seq', 1, true);


--
-- Name: frequency_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.frequency_id_seq', 1, false);


--
-- Name: history_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.history_id_seq', 1, false);


--
-- Name: karma_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.karma_id_seq', 1, false);


--
-- Name: record_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.record_id_seq', 1, false);


--
-- Name: sum_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.sum_id_seq', 1, false);


--
-- Name: transfer_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.transfer_id_seq', 1, false);


--
-- Name: views_id_seq; Type: SEQUENCE SET; Schema: public; Owner: -
--

SELECT pg_catalog.setval('public.views_id_seq', 1, true);


--
-- PostgreSQL database dump complete
--

