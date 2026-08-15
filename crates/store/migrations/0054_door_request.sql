-- The front door's inbox (Ontology §11, "Front-door mechanics", C3).
--
-- A stranger's "add me in Lince" arrives at the always-on Cell, whose owner may
-- be on a phone that is not in the public record and may be offline. The front
-- door HOLDS the request until a Cell that can decide sees it.
--
-- LOCAL-ONLY, and that is the whole point. A front door holds
-- `relay_capabilities()` — no write, no karma, no represent — so it cannot log
-- an op, cannot bind a contact, and cannot accept on the owner's behalf. If
-- this queue were in the op log it would have to, and "the front door holds no
-- signing material" would stop being a structural fact. Instead a personal
-- Cell PULLS this over `FetchDoorRequests` and makes the decision itself, with
-- its own capabilities.
CREATE TABLE door_request (
    uid         TEXT PRIMARY KEY,
    -- The NodeId that knocked, authenticated by QUIC. Identity, unlike
    -- anything inside `intro`.
    node_id     TEXT NOT NULL,
    -- What they CLAIM to be: their Organ uid, as declared. Untrusted until a
    -- Cell that can decide adopts it.
    organ_uid   TEXT NOT NULL,
    -- The whole Introduction, verbatim, so the deciding Cell sees exactly what
    -- arrived rather than a summary the door chose.
    intro       TEXT NOT NULL,
    received_at TEXT NOT NULL
);
-- One live request per knocking NodeId: a stranger retrying every minute must
-- not grow the queue without bound, and the newest attempt is the one worth
-- keeping.
CREATE UNIQUE INDEX idx_door_request_node ON door_request(node_id);
