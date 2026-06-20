CREATE TABLE IF NOT EXISTS work_metadata (
    id INTEGER PRIMARY KEY,
    owner_kind TEXT NOT NULL CHECK (owner_kind IN ('record', 'transfer', 'transfer_item', 'transfer_interaction')),
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

CREATE INDEX IF NOT EXISTS idx_work_metadata_owner
ON work_metadata(owner_kind, owner_id);

CREATE INDEX IF NOT EXISTS idx_work_metadata_status
ON work_metadata(status);

CREATE TABLE IF NOT EXISTS work_subject (
    id INTEGER PRIMARY KEY,
    subject_kind TEXT NOT NULL CHECK (subject_kind IN ('app_user', 'organ', 'transfer_party', 'external_actor', 'placeholder')),
    app_user_id INTEGER REFERENCES app_user(id) ON DELETE CASCADE,
    organ_id INTEGER REFERENCES organ(id) ON DELETE CASCADE,
    transfer_party_id INTEGER REFERENCES transfer_party(id) ON DELETE CASCADE,
    remote_base_url TEXT,
    remote_public_key TEXT,
    remote_subject_uid TEXT,
    display_name_snapshot TEXT,
    organ_name_snapshot TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(created_at) IS NOT NULL),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(updated_at) IS NOT NULL),
    CHECK (
        (subject_kind = 'app_user' AND app_user_id IS NOT NULL)
        OR (subject_kind = 'organ' AND organ_id IS NOT NULL)
        OR (subject_kind = 'transfer_party' AND transfer_party_id IS NOT NULL)
        OR (subject_kind IN ('external_actor', 'placeholder') AND length(trim(COALESCE(display_name_snapshot, ''))) > 0)
    )
) STRICT;

CREATE UNIQUE INDEX IF NOT EXISTS uq_work_subject_app_user
ON work_subject(app_user_id)
WHERE subject_kind = 'app_user' AND app_user_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS uq_work_subject_organ
ON work_subject(organ_id)
WHERE subject_kind = 'organ' AND organ_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS uq_work_subject_transfer_party
ON work_subject(transfer_party_id)
WHERE subject_kind = 'transfer_party' AND transfer_party_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS uq_work_subject_remote
ON work_subject(subject_kind, remote_base_url, remote_subject_uid)
WHERE remote_base_url IS NOT NULL AND remote_subject_uid IS NOT NULL;

CREATE TABLE IF NOT EXISTS work_assignment (
    id INTEGER PRIMARY KEY,
    work_metadata_id INTEGER NOT NULL REFERENCES work_metadata(id) ON DELETE CASCADE,
    work_subject_id INTEGER NOT NULL REFERENCES work_subject(id) ON DELETE CASCADE,
    assignment_kind TEXT NOT NULL DEFAULT 'responsible' CHECK (assignment_kind IN ('responsible', 'observer', 'helper')),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(created_at) IS NOT NULL),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(updated_at) IS NOT NULL),
    UNIQUE(work_metadata_id, work_subject_id, assignment_kind)
) STRICT;

CREATE INDEX IF NOT EXISTS idx_work_assignment_metadata
ON work_assignment(work_metadata_id);

CREATE INDEX IF NOT EXISTS idx_work_assignment_subject
ON work_assignment(work_subject_id);

INSERT INTO work_metadata (
    owner_kind,
    owner_id,
    task_type,
    start_at,
    end_at,
    estimate_seconds,
    metadata_json,
    created_at,
    updated_at
)
SELECT
    'record',
    record.id,
    task_type.task_type,
    schedule.start_at,
    schedule.end_at,
    effort.estimate_seconds,
    json_object('categories', COALESCE(json(categories.categories_json), json('[]'))),
    CURRENT_TIMESTAMP,
    CURRENT_TIMESTAMP
FROM record
LEFT JOIN (
    SELECT
        record_id,
        json_extract(freestyle_data_structure, '$.task_type') AS task_type
    FROM record_extension
    WHERE namespace = 'task.type'
) task_type ON task_type.record_id = record.id
LEFT JOIN (
    SELECT
        record_id,
        json_extract(freestyle_data_structure, '$.start_at') AS start_at,
        json_extract(freestyle_data_structure, '$.end_at') AS end_at
    FROM record_extension
    WHERE namespace = 'task.schedule'
) schedule ON schedule.record_id = record.id
LEFT JOIN (
    SELECT
        record_id,
        CAST(json_extract(freestyle_data_structure, '$.estimate_seconds') AS INTEGER) AS estimate_seconds
    FROM record_extension
    WHERE namespace = 'task.effort'
) effort ON effort.record_id = record.id
LEFT JOIN (
    SELECT
        record_id,
        COALESCE(json_extract(freestyle_data_structure, '$.categories'), '[]') AS categories_json
    FROM record_extension
    WHERE namespace = 'task.categories'
) categories ON categories.record_id = record.id
WHERE task_type.task_type IS NOT NULL
   OR schedule.start_at IS NOT NULL
   OR schedule.end_at IS NOT NULL
   OR effort.estimate_seconds IS NOT NULL
   OR categories.categories_json IS NOT NULL
   OR EXISTS (
       SELECT 1
       FROM record_link link
       WHERE link.record_id = record.id
         AND link.link_type = 'assigned_to'
         AND link.target_table = 'app_user'
   )
ON CONFLICT(owner_kind, owner_id) DO UPDATE SET
    task_type = COALESCE(excluded.task_type, work_metadata.task_type),
    start_at = COALESCE(excluded.start_at, work_metadata.start_at),
    end_at = COALESCE(excluded.end_at, work_metadata.end_at),
    estimate_seconds = COALESCE(excluded.estimate_seconds, work_metadata.estimate_seconds),
    metadata_json = excluded.metadata_json,
    updated_at = CURRENT_TIMESTAMP;

INSERT INTO work_subject (
    subject_kind,
    app_user_id,
    display_name_snapshot,
    created_at,
    updated_at
)
SELECT DISTINCT
    'app_user',
    app_user.id,
    COALESCE(NULLIF(trim(app_user.name), ''), NULLIF(trim(app_user.username), ''), 'user ' || app_user.id),
    CURRENT_TIMESTAMP,
    CURRENT_TIMESTAMP
FROM record_link link
JOIN app_user ON app_user.id = link.target_id
WHERE link.link_type = 'assigned_to'
  AND link.target_table = 'app_user'
ON CONFLICT(app_user_id) WHERE subject_kind = 'app_user' AND app_user_id IS NOT NULL DO UPDATE SET
    display_name_snapshot = excluded.display_name_snapshot,
    updated_at = CURRENT_TIMESTAMP;

INSERT INTO work_assignment (
    work_metadata_id,
    work_subject_id,
    assignment_kind,
    created_at,
    updated_at
)
SELECT DISTINCT
    metadata.id,
    subject.id,
    'responsible',
    CURRENT_TIMESTAMP,
    CURRENT_TIMESTAMP
FROM record_link link
JOIN work_metadata metadata
    ON metadata.owner_kind = 'record'
   AND metadata.owner_id = link.record_id
JOIN work_subject subject
    ON subject.subject_kind = 'app_user'
   AND subject.app_user_id = link.target_id
WHERE link.link_type = 'assigned_to'
  AND link.target_table = 'app_user'
ON CONFLICT(work_metadata_id, work_subject_id, assignment_kind) DO NOTHING;
