-- The identity floor (Ontology §11 "Key compromise: the root/operational
-- split"). These three tables are what make a published key survive device
-- changes, rotations and theft without every contact having to re-pair.
--
-- Landing them EARLY is the whole point: every one is cheap now and brutal to
-- retrofit once keys are in other people's hands.

-- One Organ's current signed Cell roster — ours and every contact's.
--
-- The roster is what a root key signs, and being listed in the current one IS
-- what certifies a Cell's operational key. There is no separate certificate
-- object: one signed blob carries membership, certification, versioning and
-- expiry together.
CREATE TABLE organ_roster (
    organ_uid  TEXT PRIMARY KEY,
    -- The ROOT public key that signed this roster. Not the operational keys,
    -- which are inside the payload.
    root_key   TEXT NOT NULL,
    -- Monotonic. A contact accepts only a roster NEWER than the one it holds,
    -- which is what makes removing a stolen device stick: an old roster
    -- cannot be replayed to re-add it.
    version    INTEGER NOT NULL,
    -- Self-limiting credentials beat remembering to revoke: a Cell that stops
    -- syncing fresh rosters loses authority on its own.
    not_after  TEXT NOT NULL,
    payload    TEXT NOT NULL,
    signature  TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Key succession: the OLD root key signs a statement endorsing the NEW one, so
-- an Organ can rotate without every contact re-pairing.
--
-- A succession is accepted ONLY if it chains from a key already held. Anything
-- else is a loud, blocking warning that needs a human decision — never a
-- silent update. That is the cheap approximation of key transparency, and it
-- is what converts a silent takeover into a visible alarm.
CREATE TABLE identity_succession (
    organ_uid  TEXT NOT NULL,
    old_key    TEXT NOT NULL,
    new_key    TEXT NOT NULL,
    signature  TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (organ_uid, old_key, new_key)
);

-- Pre-signed revocation certificates, generated at key creation and stored
-- offline beside the root. Publishing one does not prove a new key is genuine
-- — it kills the old one immediately, which is damage limitation that works
-- even when identity cannot yet be re-established. PGP has done this for
-- decades and it costs nothing.
CREATE TABLE identity_revocation (
    organ_uid   TEXT NOT NULL,
    revoked_key TEXT NOT NULL,
    signature   TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    PRIMARY KEY (organ_uid, revoked_key)
);
