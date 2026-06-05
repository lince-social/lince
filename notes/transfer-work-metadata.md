# Work Metadata

The previous aftermath section overlapped with Kanban. That is a real design signal: expected time, start/end date, assignees, and work state should probably become general-purpose Lince metadata that can attach to Records and Transfers.

For Transfers, useful work metadata includes:

- Expected duration.
- Start date.
- End date.
- Assignees.
- Work status.
- Completion notes.

## Tables

```sql
CREATE TABLE work_metadata (
    id INTEGER PRIMARY KEY,
    owner_kind TEXT NOT NULL,
    owner_id INTEGER NOT NULL,
    expected_duration_seconds REAL,
    started_at TEXT,
    ended_at TEXT,
    status TEXT,
    freestyle_data_structure TEXT,
    CHECK (owner_kind IN ('record', 'transfer', 'transfer_item', 'transfer_interaction'))
) STRICT;

CREATE TABLE work_assignment (
    id INTEGER PRIMARY KEY,
    work_metadata_id INTEGER NOT NULL REFERENCES work_metadata(id) ON DELETE CASCADE,
    subject_id INTEGER NOT NULL REFERENCES transfer_visibility_subject(id),
    assignment_kind TEXT NOT NULL DEFAULT 'responsible',
    CHECK (assignment_kind IN ('responsible', 'observer', 'helper'))
) STRICT;
```

This keeps Kanban behavior reusable without forcing every Transfer to become a Kanban card.
