-- A login granted to a contact Organ (Ontology §11 "live mode").
--
-- Deliberately NOT an `app_user` row with a password. A password answers "who
-- is at this keyboard" for a browser reaching a Cell over HTTPS; here the
-- iroh handshake has already proved which ORGAN is on the connection, and it
-- proved it with a key rather than a secret anyone could retype. Adding a
-- password would be a second, weaker way in to the same session.
--
-- What this table adds is the other half: WHICH PERSON that Organ acts as once
-- it is in. Every read they make is gated by `visible_targets` on that Person,
-- so granting a login is granting a named identity on this Cell, not a
-- bypass — and revoking is deleting one row.
CREATE TABLE organ_login (
    organ_uid  TEXT PRIMARY KEY REFERENCES record(uid),
    -- The Person record they act as. UNIQUE: two Organs sharing one Person
    -- would make their reads and writes indistinguishable in the Ledger.
    person_uid TEXT NOT NULL UNIQUE REFERENCES record(uid),
    created_at TEXT NOT NULL
);
