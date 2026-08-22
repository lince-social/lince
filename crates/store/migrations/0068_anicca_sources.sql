-- Anicca makes Frequency and Rule declarations directly file-backed.
-- Quantity is the explicit 0/1 execution switch. `next_at` is the durable
-- cursor that the successful occurrence protocol writes back to `.lingua`.
ALTER TABLE frequency ADD COLUMN quantity INTEGER NOT NULL DEFAULT 1
    CHECK (quantity IN (0, 1));
ALTER TABLE frequency ADD COLUMN timezone TEXT NOT NULL DEFAULT 'UTC';
ALTER TABLE frequency ADD COLUMN next_at TEXT;
UPDATE frequency SET next_at = anchor_at WHERE next_at IS NULL;

ALTER TABLE frequency_revision ADD COLUMN quantity INTEGER NOT NULL DEFAULT 1
    CHECK (quantity IN (0, 1));
ALTER TABLE frequency_revision ADD COLUMN timezone TEXT NOT NULL DEFAULT 'UTC';
ALTER TABLE frequency_revision ADD COLUMN next_at TEXT;
UPDATE frequency_revision SET next_at = anchor_at WHERE next_at IS NULL;

ALTER TABLE recurrence ADD COLUMN slug TEXT;
UPDATE recurrence SET slug = uid WHERE slug IS NULL;
CREATE UNIQUE INDEX idx_recurrence_slug ON recurrence(slug);
ALTER TABLE recurrence ADD COLUMN frequency_uid TEXT REFERENCES frequency(uid);
ALTER TABLE recurrence ADD COLUMN quantity INTEGER NOT NULL DEFAULT 1
    CHECK (quantity IN (0, 1));

ALTER TABLE recurrence_revision ADD COLUMN slug TEXT;
ALTER TABLE recurrence_revision ADD COLUMN frequency_uid TEXT REFERENCES frequency(uid);
ALTER TABLE recurrence_revision ADD COLUMN quantity INTEGER NOT NULL DEFAULT 1
    CHECK (quantity IN (0, 1));

-- A firing receipt is immutable and names the intended beat. It is the retry
-- boundary between committing consequences and advancing the source file.
CREATE TABLE anicca_rule_firing (
    rule_uid      TEXT NOT NULL REFERENCES recurrence(uid),
    frequency_uid TEXT NOT NULL REFERENCES frequency(uid),
    intended_at   TEXT NOT NULL,
    applied_at    TEXT NOT NULL,
    PRIMARY KEY (rule_uid, frequency_uid, intended_at)
);
