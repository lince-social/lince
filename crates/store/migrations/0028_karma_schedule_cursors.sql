-- Karma K2.3: durable schedule cursors, fenced work leases, and immutable
-- schedule occurrence intents. The indexed next_intended_at column feeds a
-- tickless in-memory deadline index; it is never polled at a fixed cadence.

CREATE TABLE karma_schedule_cursor (
    activation_hash       TEXT PRIMARY KEY
                          REFERENCES karma_frequency_activation(activation_hash),
    frequency_uid         TEXT NOT NULL REFERENCES karma_frequency(record_uid),
    cursor_revision       INTEGER NOT NULL CHECK (cursor_revision >= 1),
    cadence_kind          TEXT NOT NULL CHECK (cadence_kind IN ('elapsed', 'calendar')),
    lifecycle             TEXT NOT NULL
                          CHECK (lifecycle IN ('armed', 'leased', 'paused', 'superseded', 'failed')),
    cursor_json           TEXT NOT NULL CHECK (json_valid(cursor_json)),
    last_intended_at      TEXT,
    next_intended_at      TEXT,
    required_resolution_ms INTEGER NOT NULL CHECK (required_resolution_ms >= 1),
    max_lateness_ms       INTEGER NOT NULL CHECK (max_lateness_ms >= 0),
    coalesce_window_ms    INTEGER NOT NULL CHECK (coalesce_window_ms >= 0),
    overload_policy       TEXT NOT NULL
                          CHECK (overload_policy IN ('pause-and-ask', 'reject-activation', 'degrade-within-grant')),
    demand_json           TEXT NOT NULL CHECK (json_valid(demand_json)),
    admitted_resolution_ms INTEGER CHECK (admitted_resolution_ms >= 1),
    admission_degraded    INTEGER NOT NULL DEFAULT 0
                          CHECK (admission_degraded IN (0, 1)),
    admitted_at           TEXT,
    lease_fencing_token   INTEGER NOT NULL DEFAULT 0 CHECK (lease_fencing_token >= 0),
    lease_owner           TEXT,
    lease_expires_at      TEXT,
    last_occurrence_sequence INTEGER NOT NULL DEFAULT 0 CHECK (last_occurrence_sequence >= 0),
    last_error_json       TEXT CHECK (last_error_json IS NULL OR json_valid(last_error_json)),
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL,
    CHECK (coalesce_window_ms <= max_lateness_ms),
    CHECK ((admitted_resolution_ms IS NULL) = (admitted_at IS NULL)),
    CHECK (admitted_resolution_ms IS NOT NULL OR admission_degraded = 0),
    CHECK (lifecycle NOT IN ('armed', 'leased') OR next_intended_at IS NOT NULL),
    CHECK ((lifecycle = 'leased') =
           (lease_owner IS NOT NULL AND lease_expires_at IS NOT NULL))
) STRICT;

CREATE INDEX karma_schedule_cursor_armed_deadline
    ON karma_schedule_cursor(lifecycle, next_intended_at, required_resolution_ms, activation_hash);
CREATE INDEX karma_schedule_cursor_frequency
    ON karma_schedule_cursor(frequency_uid, lifecycle, activation_hash);
CREATE INDEX karma_schedule_cursor_expired_lease
    ON karma_schedule_cursor(lifecycle, lease_expires_at, activation_hash);

CREATE TABLE karma_schedule_occurrence (
    occurrence_hash       TEXT PRIMARY KEY
                          CHECK (length(occurrence_hash) = 71 AND occurrence_hash GLOB 'sha256:[0-9a-f]*'),
    cadence_kind          TEXT NOT NULL CHECK (cadence_kind IN ('elapsed', 'calendar')),
    activation_hash       TEXT NOT NULL REFERENCES karma_frequency_activation(activation_hash),
    sequence              INTEGER NOT NULL CHECK (sequence >= 1),
    claimed_cursor_revision INTEGER NOT NULL CHECK (claimed_cursor_revision >= 1),
    lease_fencing_token   INTEGER NOT NULL CHECK (lease_fencing_token >= 1),
    observed_at           TEXT NOT NULL,
    emission_kind         TEXT NOT NULL CHECK (emission_kind IN ('individual', 'coalesced')),
    first_intended_at     TEXT NOT NULL,
    last_intended_at      TEXT NOT NULL,
    covered_boundary_count INTEGER NOT NULL CHECK (covered_boundary_count >= 1),
    semantic_occurrence_count INTEGER NOT NULL CHECK (semantic_occurrence_count >= 1),
    occurrence_json       TEXT NOT NULL CHECK (json_valid(occurrence_json)),
    created_at            TEXT NOT NULL,
    CHECK (first_intended_at <= last_intended_at),
    CHECK ((emission_kind = 'individual' AND
            semantic_occurrence_count = covered_boundary_count) OR
           (emission_kind = 'coalesced' AND semantic_occurrence_count = 1)),
    UNIQUE (activation_hash, sequence)
) STRICT;

CREATE INDEX karma_schedule_occurrence_activation
    ON karma_schedule_occurrence(activation_hash, sequence);
CREATE INDEX karma_schedule_occurrence_intended_range
    ON karma_schedule_occurrence(activation_hash, first_intended_at, last_intended_at);

CREATE TRIGGER karma_schedule_cursor_identity_immutable
BEFORE UPDATE OF activation_hash, frequency_uid, cadence_kind,
                 required_resolution_ms, max_lateness_ms,
                 coalesce_window_ms, overload_policy, demand_json
ON karma_schedule_cursor
BEGIN SELECT RAISE(ABORT, 'Karma schedule cursor identity and timer contract are immutable'); END;

CREATE TRIGGER karma_schedule_cursor_immutable_delete
BEFORE DELETE ON karma_schedule_cursor
BEGIN SELECT RAISE(ABORT, 'Karma schedule cursors are retained as history'); END;

CREATE TRIGGER karma_schedule_occurrence_immutable_update
BEFORE UPDATE ON karma_schedule_occurrence
BEGIN SELECT RAISE(ABORT, 'Karma schedule occurrences are immutable'); END;

CREATE TRIGGER karma_schedule_occurrence_immutable_delete
BEFORE DELETE ON karma_schedule_occurrence
BEGIN SELECT RAISE(ABORT, 'Karma schedule occurrences are immutable'); END;

CREATE TRIGGER karma_schedule_cursor_scope_insert
BEFORE INSERT ON karma_schedule_cursor
WHEN NOT EXISTS (
    SELECT 1 FROM karma_frequency_activation activation
    WHERE activation.activation_hash = NEW.activation_hash
      AND activation.frequency_uid = NEW.frequency_uid
)
BEGIN SELECT RAISE(ABORT, 'Karma schedule cursor activation scope is invalid'); END;
