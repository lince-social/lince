-- Recurrence: a standing declaration that a quantity change is expected again.
--
-- A rent, a salary, a weekly stock count, a monthly backup review. The rule
-- says what is expected, how much, how often, and what it counts as. It is
-- nothing but a declaration: no Fact exists until somebody (or a grant) applies
-- an occurrence, and the Ledger stays the only quantity truth.
--
-- Nothing here is domain-specific. A surface is one caller; the rule does not
-- know what that surface calls the thing it is counting.
--
-- **There is deliberately no occurrence table.** Due dates are a pure function
-- of the cadence and the anchor, so materializing them would create a second
-- copy of a derivable fact and a cursor to keep in sync with it. The two things
-- that are *not* derivable — "this one was applied" and "this one was skipped"
-- — are the only ones stored, and the first is not even stored here: applying
-- an occurrence writes an ordinary entry whose `request_id` is
-- `<recurrence_uid>:<due_at>`, so `entry_revision`'s existing UNIQUE(request_id)
-- is what makes applying twice impossible. A read derives the dates and marks
-- each one by looking for those two signals.
CREATE TABLE recurrence (
    uid             TEXT PRIMARY KEY,
    -- The Record this rule is about. One rule, one target: letting each
    -- consequence name its own would make "what does this rule touch?"
    -- unanswerable without evaluating it, which is the question a person
    -- scanning a list of rules is actually asking.
    record_uid      TEXT NOT NULL REFERENCES record(uid),
    -- A serialized `nucleus::karma::Consequences`: the ordered, non-empty list
    -- of typed changes this rule makes when one of its dates is applied.
    --
    -- This replaced a single exact `amount` plus a `concept_uid`. That pair was
    -- the first caller's shape leaking into the model: it could only ever say
    -- "add this number", so it could not say "this task is due again" or "move
    -- this card from @wip to @done". Every variant reduces to a typed Action a
    -- person could have performed by hand, which is what keeps a rule-applied
    -- change auditable by exactly the same means as a manual one.
    --
    -- A list rather than one, because the useful cases are pairs: removing
    -- @wip and adding @done is one intention and must be one rule, or a reader
    -- has to know that two rules are secretly joined.
    consequences_json TEXT NOT NULL CHECK (json_valid(consequences_json)),
    -- The *if* half of "when, if, then". NULL means unconditional: the date
    -- arriving is the whole reason to act.
    --
    -- `condition_src` is the text a person wrote; `gate` decides whether the
    -- number it computes means "fire"; `carry` decides what number the
    -- consequences receive. Gate and carry are separate because "fire when
    -- stock drops below three" and "then order one" are two decisions, and a
    -- pipeline that fuses them can only hand over the number it happened to
    -- test.
    condition_src   TEXT,
    gate            TEXT,
    carry           TEXT,
    note            TEXT,
    -- A serialized `nucleus::karma::Cadence`: the step, the weekday landing, the
    -- short-month policy, and the bound. One column because the shapes differ per
    -- rule and a column-per-field table would be mostly NULL and would let an
    -- impossible combination be written.
    --
    -- There is deliberately no `ends_at` column beside it. When a rule stops is
    -- part of the rule, and a second place to say it is a second thing to keep in
    -- sync. `bound` inside this JSON is the only answer, which is also what makes
    -- "once, on the 14th" the same object as "monthly forever" rather than a
    -- special case with an end date bolted on.
    cadence_json    TEXT NOT NULL CHECK (json_valid(cadence_json)),
    -- Sets both the phase and the time of day. Dates before it are never
    -- produced: a rule does not apply to before it was declared.
    anchor_at       TEXT NOT NULL,
    -- 'active' | 'paused'. Pausing stops future dates being offered without
    -- disowning anything already applied.
    state           TEXT NOT NULL,
    revision        INTEGER NOT NULL,
    actor_uid       TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);
CREATE INDEX idx_recurrence_record ON recurrence(record_uid, state);

-- Every change to a rule, append-only. `request_id` is UNIQUE, which is what
-- makes create/revise/pause idempotent under retry.
--
-- Editing a rule is not editing history: occurrences already applied are
-- ordinary entries and keep the amount they were applied with. A revision
-- changes what is expected *from here on*, which is why the old values stay
-- readable here rather than being overwritten in place.
CREATE TABLE recurrence_revision (
    uid             TEXT PRIMARY KEY,
    recurrence_uid  TEXT NOT NULL REFERENCES recurrence(uid),
    revision        INTEGER NOT NULL,
    -- 'created' | 'revised' | 'paused' | 'resumed'
    kind            TEXT NOT NULL,
    consequences_json TEXT NOT NULL CHECK (json_valid(consequences_json)),
    -- The *if* half of "when, if, then". NULL means unconditional: the date
    -- arriving is the whole reason to act.
    --
    -- `condition_src` is the text a person wrote; `gate` decides whether the
    -- number it computes means "fire"; `carry` decides what number the
    -- consequences receive. Gate and carry are separate because "fire when
    -- stock drops below three" and "then order one" are two decisions, and a
    -- pipeline that fuses them can only hand over the number it happened to
    -- test.
    condition_src   TEXT,
    gate            TEXT,
    carry           TEXT,
    note            TEXT,
    cadence_json    TEXT NOT NULL CHECK (json_valid(cadence_json)),
    anchor_at       TEXT NOT NULL,
    state           TEXT NOT NULL,
    request_id      TEXT NOT NULL,
    actor_uid       TEXT,
    at              TEXT NOT NULL,
    UNIQUE(recurrence_uid, revision)
);
CREATE UNIQUE INDEX idx_recurrence_revision_request
    ON recurrence_revision(request_id);

-- "Not this one." A skip is a real decision — this month's rent was not paid,
-- this week's count was not taken — and it has to survive, because a derived
-- date that is merely absent is indistinguishable from one nobody looked at yet.
--
-- Keyed by the due date rather than by an occurrence id, since the due date is
-- the occurrence's whole identity.
CREATE TABLE recurrence_skip (
    recurrence_uid TEXT NOT NULL REFERENCES recurrence(uid),
    due_at         TEXT NOT NULL,
    note           TEXT,
    actor_uid      TEXT,
    at             TEXT NOT NULL,
    PRIMARY KEY (recurrence_uid, due_at)
);
