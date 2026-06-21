ALTER TABLE transfer_event
ADD COLUMN previous_event_hash TEXT;

ALTER TABLE transfer_event
ADD COLUMN event_hash TEXT;

ALTER TABLE transfer_event
ADD COLUMN validation_state TEXT NOT NULL DEFAULT 'pending' CHECK (validation_state IN ('pending', 'valid', 'invalid'));

ALTER TABLE transfer_event
ADD COLUMN validation_error TEXT;
