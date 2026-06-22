ALTER TABLE organ
ADD COLUMN proximity INTEGER NOT NULL DEFAULT 100 CHECK (proximity >= 0);

CREATE TABLE IF NOT EXISTS transfer_visibility_policy (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL UNIQUE REFERENCES transfer(id) ON DELETE CASCADE,
    visibility_mode TEXT NOT NULL DEFAULT 'hidden' CHECK (visibility_mode IN ('hidden', 'public', 'restricted')),
    max_visible_proximity INTEGER CHECK (max_visible_proximity IS NULL OR max_visible_proximity >= 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;

CREATE INDEX IF NOT EXISTS idx_transfer_visibility_policy_mode
ON transfer_visibility_policy(visibility_mode);

INSERT INTO transfer_visibility_policy(transfer_id, visibility_mode)
SELECT transfer.id, 'hidden'
FROM transfer
WHERE NOT EXISTS (
    SELECT 1
    FROM transfer_visibility_policy policy
    WHERE policy.transfer_id = transfer.id
);
