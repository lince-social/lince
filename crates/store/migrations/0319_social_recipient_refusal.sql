ALTER TABLE social_service_completed ADD COLUMN stage TEXT NOT NULL DEFAULT 'recipient-durable' CHECK(stage IN ('recipient-durable','recipient-refused'));
CREATE TABLE social_receive_failure (
    context TEXT NOT NULL REFERENCES record(uid),
    service TEXT NOT NULL,
    envelope TEXT NOT NULL,
    reference TEXT NOT NULL CHECK(length(CAST(reference AS BLOB))<=2048),
    error TEXT NOT NULL CHECK(length(CAST(error AS BLOB))<=2048),
    expires_at INTEGER NOT NULL,
    discard INTEGER NOT NULL DEFAULT 0 CHECK(discard IN (0,1)),
    next_attempt INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(context,service,envelope)
);
