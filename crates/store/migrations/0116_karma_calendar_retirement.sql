DROP TRIGGER karma_schedule_cursor_identity_immutable;
DROP TRIGGER karma_schedule_cursor_immutable_delete;
DROP TRIGGER karma_schedule_cursor_scope_insert;
CREATE TABLE karma_schedule_cursor_retirement (
    activation_hash TEXT PRIMARY KEY REFERENCES karma_frequency_activation(activation_hash),
    frequency_uid TEXT NOT NULL REFERENCES karma_frequency(record_uid),
    cursor_revision INTEGER NOT NULL CHECK(cursor_revision >= 1),
    cadence_kind TEXT NOT NULL CHECK(cadence_kind IN ('elapsed', 'calendar')),
    lifecycle TEXT NOT NULL CHECK(lifecycle IN ('armed', 'leased', 'paused', 'retired', 'superseded', 'failed')),
    cursor_json TEXT NOT NULL CHECK(json_valid(cursor_json)),
    last_intended_at TEXT,
    next_intended_at TEXT,
    required_resolution_ms INTEGER NOT NULL CHECK(required_resolution_ms >= 1),
    max_lateness_ms INTEGER NOT NULL CHECK(max_lateness_ms >= 0),
    coalesce_window_ms INTEGER NOT NULL CHECK(coalesce_window_ms >= 0),
    overload_policy TEXT NOT NULL CHECK(overload_policy IN ('pause-and-ask', 'reject-activation', 'degrade-within-grant')),
    demand_json TEXT NOT NULL CHECK(json_valid(demand_json)),
    admitted_resolution_ms INTEGER CHECK(admitted_resolution_ms >= 1),
    admission_degraded INTEGER NOT NULL DEFAULT 0 CHECK(admission_degraded IN (0, 1)),
    admitted_at TEXT,
    lease_fencing_token INTEGER NOT NULL DEFAULT 0 CHECK(lease_fencing_token >= 0),
    lease_owner TEXT,
    lease_expires_at TEXT,
    last_occurrence_sequence INTEGER NOT NULL DEFAULT 0 CHECK(last_occurrence_sequence >= 0),
    last_error_json TEXT CHECK(last_error_json IS NULL OR json_valid(last_error_json)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK(coalesce_window_ms <= max_lateness_ms),
    CHECK((admitted_resolution_ms IS NULL) = (admitted_at IS NULL)),
    CHECK(admitted_resolution_ms IS NOT NULL OR admission_degraded = 0),
    CHECK(lifecycle NOT IN ('armed', 'leased') OR next_intended_at IS NOT NULL),
    CHECK(lifecycle != 'retired' OR next_intended_at IS NULL),
    CHECK((lifecycle = 'leased') = (lease_owner IS NOT NULL AND lease_expires_at IS NOT NULL))
) STRICT;
INSERT INTO karma_schedule_cursor_retirement SELECT * FROM karma_schedule_cursor;
DROP TABLE karma_schedule_cursor;
ALTER TABLE karma_schedule_cursor_retirement RENAME TO karma_schedule_cursor;
CREATE INDEX karma_schedule_cursor_armed_deadline
ON karma_schedule_cursor(lifecycle, next_intended_at, required_resolution_ms, activation_hash);
CREATE INDEX karma_schedule_cursor_frequency
ON karma_schedule_cursor(frequency_uid, lifecycle, activation_hash);
CREATE INDEX karma_schedule_cursor_expired_lease
ON karma_schedule_cursor(lifecycle, lease_expires_at, activation_hash);
CREATE TRIGGER karma_schedule_cursor_identity_immutable
BEFORE UPDATE OF activation_hash, frequency_uid, cadence_kind, required_resolution_ms,
max_lateness_ms, coalesce_window_ms, overload_policy, demand_json ON karma_schedule_cursor
BEGIN SELECT RAISE(ABORT, 'Karma schedule cursor identity and timer contract are immutable'); END;
CREATE TRIGGER karma_schedule_cursor_immutable_delete
BEFORE DELETE ON karma_schedule_cursor
BEGIN SELECT RAISE(ABORT, 'Karma schedule cursors are retained as history'); END;
CREATE TRIGGER karma_schedule_cursor_scope_insert
BEFORE INSERT ON karma_schedule_cursor
WHEN NOT EXISTS (
    SELECT 1 FROM karma_frequency_activation activation
    WHERE activation.activation_hash = NEW.activation_hash AND activation.frequency_uid = NEW.frequency_uid
)
BEGIN SELECT RAISE(ABORT, 'Karma schedule cursor activation scope is invalid'); END;
