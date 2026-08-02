-- Frequency: a named beat, declared once and read by any condition.
--
-- A Frequency is a slug and a step. That is the whole object. It replaces the
-- three-table apparatus in `0027_karma_frequencies.sql` — revision hashes,
-- activation epochs, effective-parameter hashes, immutability triggers — which
-- was that much machinery for a thing whose content is "every 1 day".
--
-- **There is deliberately no beat table.** Dates are a pure function of the
-- step and the anchor, so `Cadence::between(anchor, from, to)` answers both
-- "when does this fire next" and "what does the next year look like" without
-- storing either. Materialising beats would create a second copy of a
-- derivable fact and a cursor to keep in sync with it, which is the mistake the
-- schedule-cursor tables already made.
--
-- The schedule does not live on the rule any more. A rule that wants a beat
-- reads `freq(@daily)`; every rule naming it shares this one definition rather
-- than each restating it and drifting apart the first time one is edited.
CREATE TABLE frequency (
    uid        TEXT PRIMARY KEY,
    -- What a condition calls it: the `daily` in `freq(@daily)`. Unique, because
    -- a reading that could mean two beats is not a reading.
    slug       TEXT NOT NULL UNIQUE,
    -- What a person calls it. Free text; the slug is the identity.
    head       TEXT NOT NULL,
    -- A serialized `nucleus::karma::CadenceStep`: the compound step this beat
    -- advances by. One column because the shapes differ per frequency and a
    -- column-per-unit table would be mostly zero.
    every_json TEXT NOT NULL CHECK (json_valid(every_json)),
    -- Sets both the phase and the time of day. Beats before it never happen: a
    -- frequency does not apply to before it was declared.
    anchor_at  TEXT NOT NULL,
    actor_uid  TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_frequency_slug ON frequency(slug);

-- Every change to a frequency, append-only. `request_id` is UNIQUE, which is
-- what makes declaring one idempotent under retry.
CREATE TABLE frequency_revision (
    uid           TEXT PRIMARY KEY,
    frequency_uid TEXT NOT NULL REFERENCES frequency(uid),
    -- 'created' | 'revised'
    kind          TEXT NOT NULL,
    head          TEXT NOT NULL,
    every_json    TEXT NOT NULL CHECK (json_valid(every_json)),
    anchor_at     TEXT NOT NULL,
    request_id    TEXT NOT NULL,
    actor_uid     TEXT,
    at            TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_frequency_revision_request
    ON frequency_revision(request_id);
