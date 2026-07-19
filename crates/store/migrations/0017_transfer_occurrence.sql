-- Phase 4 materializes agreed promise terms as immutable directed occurrences.
-- Reservation defaults and application formulas are local Cell policy; only
-- occurrence/claim evidence and formula hashes enter signed Facts.
ALTER TABLE configuration
    ADD COLUMN transfer_reservation_default TEXT NOT NULL DEFAULT 'none'
        CHECK (transfer_reservation_default IN ('none', 'proposed', 'agreed', 'active'));

ALTER TABLE configuration
    ADD COLUMN transfer_application_formula TEXT NOT NULL DEFAULT 'incoming()'
        CHECK (length(trim(transfer_application_formula)) > 0);

CREATE TABLE transfer_phase4_request (
    idempotency_key TEXT PRIMARY KEY CHECK (length(trim(idempotency_key)) > 0),
    kind            TEXT NOT NULL CHECK (kind IN ('activation', 'claim', 'application')),
    target_uid      TEXT NOT NULL,
    fact_uid        TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at      TEXT NOT NULL
) STRICT;

CREATE TRIGGER transfer_phase4_request_collision
BEFORE INSERT ON transfer_phase4_request
WHEN EXISTS (
        SELECT 1 FROM transfer_revision
        WHERE idempotency_key = NEW.idempotency_key
    ) OR EXISTS (
        SELECT 1 FROM transfer_invitation_event
        WHERE idempotency_key = NEW.idempotency_key
    ) OR EXISTS (
        SELECT 1 FROM transfer_agreement_event
        WHERE idempotency_key = NEW.idempotency_key
    )
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by another action');
END;

CREATE TRIGGER transfer_revision_phase4_request_collision
BEFORE INSERT ON transfer_revision
WHEN EXISTS (
    SELECT 1 FROM transfer_phase4_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by occurrence action');
END;

CREATE TRIGGER transfer_invitation_phase4_request_collision
BEFORE INSERT ON transfer_invitation_event
WHEN EXISTS (
    SELECT 1 FROM transfer_phase4_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by occurrence action');
END;

CREATE TRIGGER transfer_agreement_phase4_request_collision
BEFORE INSERT ON transfer_agreement_event
WHEN EXISTS (
    SELECT 1 FROM transfer_phase4_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by occurrence action');
END;

CREATE UNIQUE INDEX promise_occurrence_identity
    ON promise(uid, transfer_uid);

CREATE TABLE transfer_exchange_path (
    uid                  TEXT PRIMARY KEY,
    transfer_uid         TEXT NOT NULL REFERENCES transfer(record_uid),
    revision             INTEGER NOT NULL CHECK (revision > 0),
    primary_promise_uid  TEXT NOT NULL REFERENCES promise(uid),
    opposite_promise_uid TEXT REFERENCES promise(uid),
    created_at           TEXT NOT NULL,
    CHECK (opposite_promise_uid IS NULL OR opposite_promise_uid != primary_promise_uid),
    UNIQUE (transfer_uid, revision, primary_promise_uid),
    UNIQUE (uid, transfer_uid, revision),
    FOREIGN KEY (primary_promise_uid, transfer_uid)
        REFERENCES promise(uid, transfer_uid),
    FOREIGN KEY (opposite_promise_uid, transfer_uid)
        REFERENCES promise(uid, transfer_uid)
) STRICT;

CREATE TRIGGER transfer_exchange_path_promise_reuse
BEFORE INSERT ON transfer_exchange_path
WHEN EXISTS (
    SELECT 1 FROM transfer_exchange_path existing
    WHERE existing.transfer_uid = NEW.transfer_uid
      AND existing.revision = NEW.revision
      AND (
          existing.primary_promise_uid = NEW.primary_promise_uid
          OR existing.opposite_promise_uid = NEW.primary_promise_uid
          OR (NEW.opposite_promise_uid IS NOT NULL AND (
              existing.primary_promise_uid = NEW.opposite_promise_uid
              OR existing.opposite_promise_uid = NEW.opposite_promise_uid
          ))
      )
)
BEGIN
    SELECT RAISE(ABORT, 'promise already belongs to an exchange path');
END;

CREATE UNIQUE INDEX transfer_exchange_path_opposite
    ON transfer_exchange_path(transfer_uid, revision, opposite_promise_uid)
    WHERE opposite_promise_uid IS NOT NULL;

CREATE TABLE transfer_activation_event (
    uid             TEXT PRIMARY KEY,
    transfer_uid    TEXT NOT NULL REFERENCES transfer(record_uid),
    revision        INTEGER NOT NULL CHECK (revision > 0),
    actor_person_uid TEXT NOT NULL REFERENCES record(uid),
    fact_uid        TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    idempotency_key TEXT NOT NULL UNIQUE REFERENCES transfer_phase4_request(idempotency_key),
    created_at      TEXT NOT NULL,
    UNIQUE (uid, transfer_uid, revision)
) STRICT;

CREATE TABLE transfer_occurrence (
    uid                 TEXT PRIMARY KEY,
    transfer_uid        TEXT NOT NULL REFERENCES transfer(record_uid),
    revision            INTEGER NOT NULL CHECK (revision > 0),
    promise_uid         TEXT NOT NULL UNIQUE REFERENCES promise(uid),
    exchange_path_uid   TEXT NOT NULL REFERENCES transfer_exchange_path(uid),
    activation_event_uid TEXT NOT NULL REFERENCES transfer_activation_event(uid),
    record_uid          TEXT REFERENCES record(uid),
    concept_uid         TEXT REFERENCES concept(uid),
    unit_uid            TEXT REFERENCES concept(uid),
    quantity            REAL NOT NULL CHECK (quantity > 0),
    giver_person_uid    TEXT NOT NULL REFERENCES record(uid),
    receiver_person_uid TEXT NOT NULL REFERENCES record(uid),
    window_start        TEXT,
    window_end          TEXT,
    location_lat        REAL CHECK (location_lat IS NULL OR location_lat BETWEEN -90 AND 90),
    location_lon        REAL CHECK (location_lon IS NULL OR location_lon BETWEEN -180 AND 180),
    location_address    TEXT,
    delivery_claimed    INTEGER NOT NULL DEFAULT 0 CHECK (delivery_claimed IN (0, 1)),
    receipt_claimed     INTEGER NOT NULL DEFAULT 0 CHECK (receipt_claimed IN (0, 1)),
    disputed            INTEGER NOT NULL DEFAULT 0 CHECK (disputed IN (0, 1)),
    dispute_fact_uid    TEXT REFERENCES fact(uid),
    disputed_at         TEXT,
    created_at          TEXT NOT NULL,
    CHECK (record_uid IS NOT NULL OR concept_uid IS NOT NULL),
    CHECK ((location_lat IS NULL) = (location_lon IS NULL)),
    CHECK ((disputed = 0 AND dispute_fact_uid IS NULL AND disputed_at IS NULL)
        OR (disputed = 1 AND dispute_fact_uid IS NOT NULL AND disputed_at IS NOT NULL)),
    FOREIGN KEY (promise_uid, transfer_uid)
        REFERENCES promise(uid, transfer_uid),
    FOREIGN KEY (exchange_path_uid, transfer_uid, revision)
        REFERENCES transfer_exchange_path(uid, transfer_uid, revision),
    FOREIGN KEY (activation_event_uid, transfer_uid, revision)
        REFERENCES transfer_activation_event(uid, transfer_uid, revision),
    FOREIGN KEY (transfer_uid, giver_person_uid)
        REFERENCES transfer_party(transfer_uid, actor_uid),
    FOREIGN KEY (transfer_uid, receiver_person_uid)
        REFERENCES transfer_party(transfer_uid, actor_uid)
) STRICT;

CREATE INDEX transfer_occurrence_path
    ON transfer_occurrence(exchange_path_uid, created_at);

CREATE INDEX transfer_occurrence_roles
    ON transfer_occurrence(transfer_uid, giver_person_uid, receiver_person_uid);

CREATE TABLE transfer_occurrence_claim_event (
    uid              TEXT PRIMARY KEY,
    occurrence_uid   TEXT NOT NULL REFERENCES transfer_occurrence(uid),
    role              TEXT NOT NULL CHECK (role IN ('delivery', 'receipt')),
    asserted          INTEGER NOT NULL CHECK (asserted IN (0, 1)),
    actor_person_uid  TEXT NOT NULL REFERENCES record(uid),
    fact_uid          TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    idempotency_key   TEXT NOT NULL UNIQUE REFERENCES transfer_phase4_request(idempotency_key),
    created_at        TEXT NOT NULL
) STRICT;

CREATE INDEX transfer_occurrence_claim_history
    ON transfer_occurrence_claim_event(occurrence_uid, role, created_at, uid);

CREATE TABLE transfer_occurrence_application_event (
    uid                TEXT PRIMARY KEY,
    occurrence_uid     TEXT NOT NULL REFERENCES transfer_occurrence(uid),
    receiver_person_uid TEXT NOT NULL REFERENCES record(uid),
    formula_hash       TEXT NOT NULL,
    version            INTEGER NOT NULL CHECK (version > 0),
    fact_uid            TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    idempotency_key     TEXT NOT NULL UNIQUE REFERENCES transfer_phase4_request(idempotency_key),
    created_at          TEXT NOT NULL,
    UNIQUE (uid, occurrence_uid)
) STRICT;

CREATE UNIQUE INDEX transfer_occurrence_application_history
    ON transfer_occurrence_application_event(occurrence_uid, version);

CREATE TABLE transfer_occurrence_application_policy (
    occurrence_uid      TEXT PRIMARY KEY REFERENCES transfer_occurrence(uid),
    receiver_person_uid TEXT NOT NULL REFERENCES record(uid),
    formula             TEXT NOT NULL CHECK (length(trim(formula)) > 0),
    formula_hash        TEXT NOT NULL,
    version             INTEGER NOT NULL CHECK (version > 0),
    latest_event_uid    TEXT NOT NULL UNIQUE,
    updated_at          TEXT NOT NULL,
    FOREIGN KEY (latest_event_uid, occurrence_uid)
        REFERENCES transfer_occurrence_application_event(uid, occurrence_uid)
) STRICT;
