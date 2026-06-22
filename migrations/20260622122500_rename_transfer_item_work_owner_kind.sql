PRAGMA foreign_keys = OFF;

CREATE TABLE work_metadata_new (
    id INTEGER PRIMARY KEY,
    owner_kind TEXT NOT NULL CHECK (owner_kind IN ('record', 'transfer', 'transfer_structured_item', 'transfer_interaction')),
    owner_id INTEGER NOT NULL CHECK (owner_id > 0),
    task_type TEXT CHECK (task_type IS NULL OR task_type IN ('epic', 'feature', 'task', 'other')),
    status TEXT,
    start_at TEXT CHECK (start_at IS NULL OR julianday(start_at) IS NOT NULL),
    end_at TEXT CHECK (end_at IS NULL OR julianday(end_at) IS NOT NULL),
    estimate_seconds INTEGER CHECK (estimate_seconds IS NULL OR estimate_seconds >= 0),
    completion_notes TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(created_at) IS NOT NULL),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(updated_at) IS NOT NULL),
    UNIQUE(owner_kind, owner_id)
) STRICT;

INSERT INTO work_metadata_new (
    id,
    owner_kind,
    owner_id,
    task_type,
    status,
    start_at,
    end_at,
    estimate_seconds,
    completion_notes,
    metadata_json,
    created_at,
    updated_at
)
SELECT
    id,
    CASE owner_kind
        WHEN 'transfer_item' THEN 'transfer_structured_item'
        ELSE owner_kind
    END,
    owner_id,
    task_type,
    status,
    start_at,
    end_at,
    estimate_seconds,
    completion_notes,
    metadata_json,
    created_at,
    updated_at
FROM work_metadata;

DROP TABLE work_metadata;
ALTER TABLE work_metadata_new RENAME TO work_metadata;

CREATE INDEX IF NOT EXISTS idx_work_metadata_owner
ON work_metadata(owner_kind, owner_id);

CREATE INDEX IF NOT EXISTS idx_work_metadata_status
ON work_metadata(status);

CREATE UNIQUE INDEX IF NOT EXISTS uq_work_metadata_owner
ON work_metadata(owner_kind, owner_id);

PRAGMA foreign_keys = ON;
