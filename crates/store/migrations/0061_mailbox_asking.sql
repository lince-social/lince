-- Being ASKED to carry, and inviting someone to ask (Ontology C4).
--
-- What existed was the operator's half: `MailboxCarryFor`, where the person
-- running the box names an Organ and registers it. That is the wrong way round
-- for the motivating case. Carrying someone's mail is a favour between people
-- who know each other, and a favour starts with the asking — otherwise the
-- only way to get a pickup point is for the operator to think of you first.
--
-- A RELAY is the opposite and stays the opposite: it accepts as its standing
-- job, which is the difference between a favour and a service.

-- An Organ that has asked, and not yet been answered.
--
-- One row per Organ by primary key, and only from an Organ whose root key this
-- Cell already holds (enforced in the engine) — so the table is bounded by the
-- contact list rather than by whoever can reach the door, and no rate limit is
-- needed to make that true. Somebody with no prior relationship gets in the
-- other way, by redeeming an invite the operator handed them, which is consent
-- given in advance instead of consent asked for after.
CREATE TABLE mailbox_request (
    organ_uid TEXT PRIMARY KEY,
    -- Taken from the roster they presented, and stored now rather than looked
    -- up at acceptance: registration is keyed on the root, and the operator
    -- may answer days later from a different device.
    root_key  TEXT NOT NULL,
    -- What they call themselves. A LABEL and never identity — it is displayed
    -- beside the uid, never instead of it.
    label     TEXT NOT NULL DEFAULT '',
    asked_at  TEXT NOT NULL
);

-- A single-use code that lets its holder register themselves.
--
-- The same object as the enrolment token, with a different verb: not "this
-- device joins my identity" but "this Organ may leave mail here". So it is
-- stored the same way — only the HASH, because the plaintext living in a row
-- would be a second, quieter way in — and claimed the same way, by an UPDATE
-- whose `rows_affected` settles a race between two redeemers.
--
-- It differs from enrolment in ONE respect, deliberately: the lifetime. An
-- enrolment code is read off a screen in the next ten minutes and grants
-- membership in an identity. This one is sent to somebody in a message and
-- grants the right to leave sealed bytes on their own quota, which is a much
-- smaller thing that has to survive the other person reading their mail
-- tomorrow.
CREATE TABLE mailbox_invite (
    token_hash   TEXT PRIMARY KEY,
    label        TEXT NOT NULL DEFAULT '',
    quota_bytes  INTEGER NOT NULL,
    expires_at   TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    used_at      TEXT,
    -- Who spent it. Kept so the operator's panel can say what a code turned
    -- into rather than only that it is gone.
    used_by      TEXT
);
