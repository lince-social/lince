-- Entries: the editable, voidable handle over an authored quantity change.
--
-- A captured change is a signed Fact plus a classification assertion, and
-- neither can be edited: the Fact is hash-chained, and the classification is an
-- assertion about it. So "I typed 15 instead of 150" needs somewhere to live
-- that is allowed to change, and that is this table.
--
-- Editing never rewrites history. A revision compensates the old Fact and
-- appends the replacement, so the Ledger keeps both and the chain stays intact;
-- this row just tracks which Fact is currently the live one.
--
-- Nothing here is domain-specific. A mistyped cost, a miscounted stock take,
-- and a wrongly logged training set are the same problem: an authored change
-- that turned out to be wrong. A surface is one caller, never the subject —
-- this table would look identical if no accounting surface existed.
--
-- What this deliberately is NOT: an account, a category, a posting, or a second
-- balance truth. It holds no total and no running sum. The Record's ordinary
-- Fact chain remains the only quantity truth, and every aggregate is still a
-- query over classified Facts.
CREATE TABLE entry (
    uid             TEXT PRIMARY KEY,
    -- The resource whose level moved: checking, cash, the flour jar.
    record_uid      TEXT NOT NULL REFERENCES record(uid),
    -- Exact, never a float: the same pair the Ledger stores. See 0001.
    amount_mantissa TEXT NOT NULL,
    amount_scale    INTEGER NOT NULL,
    note            TEXT,
    -- Occurred-at, not recorded-at, so backdating is ordinary.
    occurred_at     TEXT NOT NULL,
    -- 'applied' | 'void'. A voided event keeps its row and its history; the
    -- quantity is returned by a compensating Fact, not by deleting anything.
    state           TEXT NOT NULL,
    revision        INTEGER NOT NULL,
    -- The Fact currently carrying this movement's amount. A revision points it
    -- at the replacement; voiding leaves it pointing at the Fact that was
    -- compensated, because that is still the movement this event describes.
    fact_uid        TEXT REFERENCES fact(uid),
    actor_uid       TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
CREATE INDEX idx_entry_record ON entry(record_uid, occurred_at);
CREATE INDEX idx_entry_fact ON entry(fact_uid);

-- There is deliberately NO concept_uid column here.
--
-- A movement's classification already has exactly one owner: `fact_concept`
-- and its event log from 0036. Storing it a second time on the event would let
-- the two disagree the moment a Fact is re-tagged, which is the same
-- "two overlapping numbers" failure this design removes at the totals level,
-- one layer down. Readers join through `fact_uid` instead.

-- Every change to an event, append-only: what it was, what it became, which
-- Facts were involved, and which request asked for it.
--
-- `request_id` is UNIQUE, which is what makes create/revise/void idempotent. A
-- retried request finds its own prior row and returns that result rather than
-- appending a second compensating Fact — the failure mode that turns a network
-- hiccup into a quantity moving twice.
CREATE TABLE entry_revision (
    uid                  TEXT PRIMARY KEY,
    entry_uid            TEXT NOT NULL REFERENCES entry(uid),
    revision             INTEGER NOT NULL,
    -- 'created' | 'revised' | 'voided'
    kind                 TEXT NOT NULL,
    amount_mantissa      TEXT NOT NULL,
    amount_scale         INTEGER NOT NULL,
    note                 TEXT,
    occurred_at          TEXT NOT NULL,
    -- The Fact this revision appended, if any.
    fact_uid             TEXT REFERENCES fact(uid),
    -- The Fact it reversed, if any. Both are recorded so the audit trail can
    -- show the correction as a pair rather than two unrelated changes.
    compensated_fact_uid TEXT REFERENCES fact(uid),
    request_id           TEXT NOT NULL,
    actor_uid            TEXT,
    at                   TEXT NOT NULL,
    UNIQUE(entry_uid, revision)
);
CREATE UNIQUE INDEX idx_entry_revision_request
    ON entry_revision(request_id);
