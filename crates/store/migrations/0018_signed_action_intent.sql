-- Client-held Person keys authorize exact Action bytes. This evidence is
-- distinct from `fact.signature`, which remains exclusively a signature over
-- the Fact hash.
CREATE TABLE signed_action_intent (
    uid                 TEXT PRIMARY KEY,
    session_id          TEXT NOT NULL CHECK (length(trim(session_id)) > 0),
    session_challenge   TEXT NOT NULL CHECK (length(trim(session_challenge)) > 0),
    sequence            INTEGER NOT NULL CHECK (sequence > 0),
    message_id          TEXT NOT NULL CHECK (length(trim(message_id)) > 0),
    actor_person_uid    TEXT NOT NULL REFERENCES record(uid),
    key_id              TEXT NOT NULL CHECK (length(trim(key_id)) > 0),
    action_base64       TEXT NOT NULL CHECK (length(action_base64) > 0),
    signature           TEXT NOT NULL CHECK (length(signature) > 0),
    status              TEXT NOT NULL DEFAULT 'pending'
                        CHECK (status IN ('pending', 'committed', 'failed')),
    error_code          TEXT,
    error_message       TEXT,
    received_at         TEXT NOT NULL,
    finished_at         TEXT,
    UNIQUE (session_id, sequence),
    UNIQUE (session_id, message_id),
    FOREIGN KEY (actor_person_uid, key_id)
        REFERENCES identity_key(actor_uid, key_id),
    CHECK ((status = 'pending' AND finished_at IS NULL
            AND error_code IS NULL AND error_message IS NULL)
        OR (status = 'committed' AND finished_at IS NOT NULL
            AND error_code IS NULL AND error_message IS NULL)
        OR (status = 'failed' AND finished_at IS NOT NULL
            AND error_message IS NOT NULL))
) STRICT;

CREATE TABLE fact_action_intent (
    fact_uid    TEXT PRIMARY KEY REFERENCES fact(uid),
    intent_uid  TEXT NOT NULL REFERENCES signed_action_intent(uid)
) STRICT;

CREATE INDEX fact_action_intent_by_intent
    ON fact_action_intent(intent_uid, fact_uid);
