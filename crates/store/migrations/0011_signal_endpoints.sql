ALTER TABLE embedded_authority RENAME TO signal_endpoint_authority;
DELETE FROM signal_endpoint_authority;
ALTER TABLE embedded RENAME TO signal_endpoint;
ALTER TABLE signal_endpoint RENAME COLUMN key_epoch TO credential_epoch;
ALTER TABLE embedded_audit RENAME TO signal_endpoint_audit;
ALTER TABLE signal_endpoint_audit RENAME COLUMN device_uid TO endpoint_uid;

CREATE TABLE signal_endpoint_token (
    endpoint_uid TEXT PRIMARY KEY REFERENCES signal_endpoint(uid),
    digest TEXT NOT NULL UNIQUE
);
