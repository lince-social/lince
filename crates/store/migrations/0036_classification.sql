-- Classification: what a thing is, and what a change to it was.
--
-- Two levels that must not be confused:
--
--   * A RECORD's concepts say what the thing IS and what it COUNTS AS. A
--     toothbrush is a toothbrush, and also a cost and a health item. These
--     drive views like "everything I use for hygiene".
--   * A FACT's concept says what a particular CHANGE was. Buying the
--     toothbrush is one -10 classified as a hygiene purchase; a stock count
--     correction is a different -10 on the same Record meaning something else
--     entirely. These drive every aggregation over the Ledger.
--
-- Nothing here is domain-specific. A balance, stock, hours, and reps are the same
-- shape: a signed quantity change on a Record, classified by a concept. A sand
-- supplies the vocabulary; the schema supplies the classification.
--
-- Direction is never stored: it is the SIGN of the delta. A refunded purchase
-- is classified @cost with a +10 delta and must REDUCE the total. A separate
-- gain/loss column would get that backwards, and two axes that can disagree is
-- a bug frozen into the schema.
--
-- No parallel flat "tags" field exists on purpose. The concept DAG already is
-- the tag system: it is multilingual through concept_name and hierarchical
-- through concept_parent, so classifying a Fact @ice-cream makes it answer a
-- query for @food and for @cost with no list to maintain.

-- A Record's ADDITIONAL concepts. `record.concept_uid` stays as the identity
-- concept -- what the thing is -- because Transfer matching and sync resolve
-- through it; this table carries everything it also counts as. Concept queries
-- read the union of the two.
CREATE TABLE record_concept (
    record_uid  TEXT NOT NULL REFERENCES record(uid),
    concept_uid TEXT NOT NULL REFERENCES concept(uid),
    at          TEXT NOT NULL,
    actor_uid   TEXT,
    UNIQUE(record_uid, concept_uid)
);
CREATE INDEX idx_record_concept_concept ON record_concept(concept_uid);

-- Classification of a MOVEMENT, as an append-only log of assertions.
--
-- This is a sidecar keyed on fact_uid rather than a column on `fact`, and that
-- is forced: `fact` is hash-chained and signed. A concept inside the preimage
-- would make every already-sealed Fact permanently unclassifiable; one outside
-- the preimage would be unsigned mutable data masquerading as ledger truth.
--
-- So a classification is an ASSERTION ABOUT a Fact, never part of it.
-- Correcting a mistagged expense is a new assertion with an audit trail, not a
-- compensating Fact over a typo -- the quantity never moved, only our account of
-- what the movement meant.
--
-- A NULL concept_uid is a deliberate "unclassify" assertion, distinct from
-- never having classified the Fact at all.
CREATE TABLE fact_concept_event (
    uid         TEXT PRIMARY KEY,
    fact_uid    TEXT NOT NULL REFERENCES fact(uid),
    concept_uid TEXT REFERENCES concept(uid),
    actor_uid   TEXT,
    note        TEXT,
    at          TEXT NOT NULL
);
-- Ordering within a Fact's assertions is rowid (insertion) order, which SQLite
-- gives for free and cannot be named in an index.
CREATE INDEX idx_fact_concept_event_fact ON fact_concept_event(fact_uid);

-- The current projection of that log: one row per classified Fact, rebuildable
-- by replaying the events in rowid order. Reads go here; corrections go to the
-- log and update this in the same transaction.
CREATE TABLE fact_concept (
    fact_uid    TEXT PRIMARY KEY REFERENCES fact(uid),
    concept_uid TEXT REFERENCES concept(uid),
    event_uid   TEXT NOT NULL REFERENCES fact_concept_event(uid),
    at          TEXT NOT NULL
);
CREATE INDEX idx_fact_concept_concept ON fact_concept(concept_uid);
