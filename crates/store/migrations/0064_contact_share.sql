ALTER TABLE organ_contact ADD COLUMN share_protein TEXT;

CREATE TABLE contact_share (
    contact_organ TEXT NOT NULL,
    record_uid TEXT NOT NULL,
    picked INTEGER NOT NULL DEFAULT 0,
    held INTEGER NOT NULL DEFAULT 0,
    added_at TEXT NOT NULL,
    PRIMARY KEY (contact_organ, record_uid)
);

CREATE INDEX contact_share_by_record ON contact_share (record_uid);
