-- Unified message table replaces both record_comment and transfer_message.
-- A message targets exactly one of: a Record or a Transfer (checked by constraint).
-- Transfer messages may also reference a specific interaction.
-- parent_message_id enables threaded replies within the same target.

CREATE TABLE IF NOT EXISTS message (
    id INTEGER PRIMARY KEY,
    record_id INTEGER REFERENCES record(id) ON DELETE CASCADE,
    transfer_id INTEGER REFERENCES transfer(id) ON DELETE CASCADE,
    interaction_id INTEGER REFERENCES transfer_interaction(id) ON DELETE SET NULL,
    parent_message_id INTEGER REFERENCES message(id) ON DELETE CASCADE,
    author_label TEXT,
    party_id INTEGER REFERENCES transfer_party(id) ON DELETE SET NULL,
    event_id INTEGER REFERENCES transfer_event(id) ON DELETE SET NULL,
    body TEXT NOT NULL CHECK (length(trim(body)) > 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(created_at) IS NOT NULL),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(updated_at) IS NOT NULL),
    deleted_at TEXT CHECK (deleted_at IS NULL OR julianday(deleted_at) IS NOT NULL),
    sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0),
    origin_organ_id INTEGER REFERENCES organ(id),
    CHECK (
        (record_id IS NOT NULL AND transfer_id IS NULL) OR
        (transfer_id IS NOT NULL AND record_id IS NULL)
    )
) STRICT;

CREATE INDEX IF NOT EXISTS idx_message_record_created ON message(record_id, created_at DESC) WHERE record_id IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_message_transfer_created ON message(transfer_id, created_at) WHERE transfer_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS uq_message_sync_uid ON message(sync_uid) WHERE sync_uid IS NOT NULL;

-- Migrate record_comment rows into message (record path)
INSERT INTO message (record_id, body, author_label, created_at, updated_at, deleted_at, sync_uid, origin_organ_id)
SELECT record_id, body, NULL, created_at, updated_at, deleted_at, sync_uid, origin_organ_id
FROM record_comment;

-- Migrate transfer_message rows into message (transfer path)
INSERT INTO message (transfer_id, interaction_id, party_id, event_id, body, created_at)
SELECT transfer_id, interaction_id, party_id, event_id, body, created_at
FROM transfer_message;

DROP TABLE IF EXISTS record_comment;
DROP TABLE IF EXISTS transfer_message;

-- Agreement type and percentage threshold for Transfer
ALTER TABLE transfer_identity ADD COLUMN agreement_type TEXT;
ALTER TABLE transfer_identity ADD COLUMN agreement_percentage INTEGER CHECK (agreement_percentage IS NULL OR (agreement_percentage >= 0 AND agreement_percentage <= 100));
