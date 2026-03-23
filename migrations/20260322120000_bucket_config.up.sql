ALTER TABLE configuration
ADD COLUMN bucket_enabled INTEGER NOT NULL DEFAULT 0;

ALTER TABLE configuration
ADD COLUMN bucket_username TEXT;

ALTER TABLE configuration
ADD COLUMN bucket_password TEXT;

ALTER TABLE configuration
ADD COLUMN bucket_uri TEXT;

ALTER TABLE configuration
ADD COLUMN bucket_name TEXT;

ALTER TABLE configuration
ADD COLUMN bucket_region TEXT;
