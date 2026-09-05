-- Ontology, cluster 4: per-contact rate limiting on the REJECT path.
--
-- The quarantine ring bounds how much a hostile contact can make us STORE.
-- It does nothing about how much it can make us DO: refusing an op costs a
-- parse, an admissibility check and a quarantine write, every pass, for free.
-- And an empty version vector legitimately means "send me everything", so a
-- peer that sends one every pass makes us serve the whole log repeatedly.
--
-- `store::budget` already holds the shape for a named budget kind, but only in
-- bytes. This is the RATE dimension, and it backs off the CONTACT rather than
-- the queue: past its allowance the contact is answered less often, and the
-- contact panel is told so with the reason.
--
-- One row per (from_organ, kind). `window_start` is the rolling window's
-- opening; `count` is what has been spent inside it. `backoff_until` is set
-- when the allowance runs out and is what the serve/import paths read: a
-- contact inside its backoff is refused BEFORE the expensive work, which is
-- the whole point.
CREATE TABLE contact_rate (
    from_organ    TEXT NOT NULL,
    kind          TEXT NOT NULL,
    window_start  TEXT NOT NULL,
    count         INTEGER NOT NULL DEFAULT 0,
    backoff_until TEXT,
    reason        TEXT,
    PRIMARY KEY (from_organ, kind)
);

-- `quarantine_tally` was created by 0065 and seeded once, then never
-- maintained by any code — a lifetime count frozen at the moment of that
-- migration. It is the same question this table answers, so it goes rather
-- than drifting further from the truth.
DROP TABLE IF EXISTS quarantine_tally;
