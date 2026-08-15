-- Read receipts for live references (Ontology §11 "Individual replica and
-- threads", C6).
--
-- A reference is read LIVE from its owner's Cell, which means the owner's Cell
-- observes the read. That is unavoidable — it is what "live" costs — so it is
-- recorded and shown rather than left as an invisible side effect. Between two
-- Organs already in a conversation the disclosure is likely acceptable, but it
-- is the same class of disclosure the Facade design spends real effort
-- avoiding, so it is stated to BOTH sides instead of discovered by one.
--
-- LOCAL ONLY, and it must stay that way. This table is not in `sync_op`'s
-- world and no op kind carries it: who read what and when is exactly the kind
-- of behavioural trail that must not become someone else's data. It is the
-- owner's own record of reads against their own Cell.
CREATE TABLE reference_read (
    -- The Organ that did the reading. Never a Cell: which of their devices
    -- opened it is theirs to know, not ours, and recording it would make this
    -- a device-tracking table.
    reader_organ TEXT NOT NULL,
    record_uid   TEXT NOT NULL,
    -- The conversation the reference was posted in, so a reader can be told
    -- WHERE they were seen reading rather than only that they were.
    root_record  TEXT NOT NULL,
    -- Last read and how many, rather than one row per read. A conversation
    -- open in a tab would otherwise write a row a minute and turn a receipt
    -- into a surveillance log — and "when did they last look" is the question
    -- anyone actually has.
    reads        INTEGER NOT NULL DEFAULT 1,
    last_read_at TEXT NOT NULL,
    PRIMARY KEY (reader_organ, record_uid, root_record)
);
CREATE INDEX reference_read_by_record ON reference_read(record_uid, last_read_at DESC);
