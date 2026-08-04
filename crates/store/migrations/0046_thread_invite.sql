-- A request to open a conversation, held locally until the user answers it.
--
-- The grant row in `replica_grant` is the MECHANISM (it is what decides
-- whether ops are accepted); this is the SURFACE (it is what a person sees and
-- answers). They are kept separate because they answer different questions,
-- and collapsing them would mean a notification could not be shown without
-- already having decided something.
--
-- `from_organ` is UNIQUE, and that single constraint is the whole anti-spam
-- rule: one pending invite per Organ, so a declined conversation cannot be
-- reopened over and over. It lives in SQL rather than in a check-then-insert
-- because an Organ is several Cells — a contact's laptop and their VPS can
-- both hold a connection, and two concurrent offers would both pass a query
-- that asked "is there one already?".
CREATE TABLE thread_invite (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),
    from_organ TEXT NOT NULL UNIQUE,
    -- The conversation root being offered: what `accept` turns into a grant.
    root       TEXT NOT NULL,
    created_at TEXT NOT NULL
);
