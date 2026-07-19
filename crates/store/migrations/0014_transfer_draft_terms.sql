-- Phase 1 signs quantities together with their unit and location context.
-- Nullable party_uid is an OPEN promise slot. Its reuse policy determines
-- whether a later signed claim consumes the proposal or copies it.
ALTER TABLE transfer ADD COLUMN default_location_lat REAL
    CHECK (default_location_lat IS NULL OR default_location_lat BETWEEN -90 AND 90);
ALTER TABLE transfer ADD COLUMN default_location_lon REAL
    CHECK (default_location_lon IS NULL OR default_location_lon BETWEEN -180 AND 180);
ALTER TABLE transfer ADD COLUMN default_location_address TEXT
    CHECK ((default_location_lat IS NULL) = (default_location_lon IS NULL));

ALTER TABLE promise
    ADD COLUMN unit_uid TEXT REFERENCES concept(uid);

ALTER TABLE promise ADD COLUMN location_lat REAL
    CHECK (location_lat IS NULL OR location_lat BETWEEN -90 AND 90);
ALTER TABLE promise ADD COLUMN location_lon REAL
    CHECK (location_lon IS NULL OR location_lon BETWEEN -180 AND 180);
ALTER TABLE promise ADD COLUMN location_address TEXT
    CHECK ((location_lat IS NULL) = (location_lon IS NULL));

ALTER TABLE promise
    ADD COLUMN open_reuse_policy TEXT NOT NULL DEFAULT 'duplicate'
        CHECK (open_reuse_policy IN ('duplicate', 'consume'));

CREATE INDEX promise_unit
    ON promise(unit_uid);
