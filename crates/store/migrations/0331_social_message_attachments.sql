CREATE TABLE social_private_outbox_attachments (
    id TEXT PRIMARY KEY,
    context TEXT NOT NULL,
    body TEXT NOT NULL CHECK(length(CAST(body AS BLOB)) <= CASE WHEN json_extract(CASE WHEN json_valid(body) THEN body ELSE '{}' END, '$.envelope.purpose') = 'content' THEN 8388608 ELSE 32768 END),
    hash TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','stored','ready','expired','held','cancelled')),
    error TEXT,
    record_uid TEXT REFERENCES record(uid),
    recipient_owner TEXT NOT NULL DEFAULT '',
    recipient_generation INTEGER NOT NULL DEFAULT 0
);
INSERT INTO social_private_outbox_attachments
    (id,context,body,hash,expires_at,state,error,record_uid,recipient_owner,recipient_generation)
SELECT id,context,body,hash,expires_at,state,error,record_uid,recipient_owner,recipient_generation
FROM social_private_outbox;

CREATE TABLE social_private_destination_attachments (
    envelope TEXT NOT NULL REFERENCES social_private_outbox_attachments(id) ON DELETE CASCADE,
    service TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','stored','failed','cancelled')),
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    receipt TEXT,
    error TEXT,
    PRIMARY KEY(envelope,service)
);
INSERT INTO social_private_destination_attachments
    (envelope,service,state,attempts,next_attempt,receipt,error)
SELECT envelope,service,state,attempts,next_attempt,receipt,error
FROM social_private_destination;
DROP TABLE social_private_destination;
DROP TABLE social_private_outbox;
ALTER TABLE social_private_outbox_attachments RENAME TO social_private_outbox;
ALTER TABLE social_private_destination_attachments RENAME TO social_private_destination;
CREATE INDEX social_private_message ON social_private_outbox(record_uid,state);

CREATE TABLE social_service_envelope_attachments (
    id TEXT PRIMARY KEY,
    route TEXT NOT NULL REFERENCES social_reply_route(id),
    sender TEXT NOT NULL,
    hash TEXT NOT NULL,
    body TEXT NOT NULL CHECK(length(CAST(body AS BLOB)) <= CASE WHEN partition = 'trusted' AND json_extract(CASE WHEN json_valid(body) THEN body ELSE '{}' END, '$.envelope.purpose') = 'content' THEN 8388608 ELSE 32768 END),
    partition TEXT NOT NULL CHECK(partition IN ('stranger','trusted','control')),
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    next_pickup INTEGER NOT NULL DEFAULT 0
);
INSERT INTO social_service_envelope_attachments
    (id,route,sender,hash,body,partition,created_at,expires_at,next_pickup)
SELECT id,route,sender,hash,body,partition,created_at,expires_at,next_pickup
FROM social_service_envelope;
DROP TABLE social_service_envelope;
ALTER TABLE social_service_envelope_attachments RENAME TO social_service_envelope;
CREATE INDEX social_service_route ON social_service_envelope(route,created_at,id);
