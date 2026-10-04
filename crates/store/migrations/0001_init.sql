CREATE TABLE record (
    uid         TEXT PRIMARY KEY,
    slug        TEXT UNIQUE,
    kind        TEXT NOT NULL DEFAULT 'plain',
    head        TEXT NOT NULL DEFAULT '',
    body        TEXT NOT NULL DEFAULT '',

    quantity_mantissa TEXT NOT NULL DEFAULT '0',
    quantity_scale    INTEGER NOT NULL DEFAULT 0,
    unit_uid    TEXT REFERENCES concept(uid),
    place_uid   TEXT REFERENCES place(uid),
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
, deleted_at TEXT, organ_uid TEXT, replica_root TEXT, created_hlc INTEGER);

CREATE TABLE fact (
    uid        TEXT PRIMARY KEY,
    record_uid TEXT NOT NULL REFERENCES record(uid),

    delta_mantissa TEXT NOT NULL,
    delta_scale    INTEGER NOT NULL,
    at         TEXT NOT NULL,
    actor_uid  TEXT,
    cause_kind TEXT NOT NULL,
    cause_uid  TEXT,
    payload    TEXT,
    prev_hash  TEXT NOT NULL,
    hash       TEXT NOT NULL,
    signature  TEXT
, commit_sequence INTEGER);

CREATE TABLE concept (
    uid            TEXT PRIMARY KEY,
    canonical_name TEXT NOT NULL UNIQUE,
    origin_organ   TEXT,
    instinct       TEXT,
    created_at     TEXT NOT NULL
);

CREATE TABLE lingua (
    uid          TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    owner_organ  TEXT,
    visibility   TEXT NOT NULL DEFAULT 'private'
                 CHECK (visibility IN ('private', 'shared', 'public')),
    created_at   TEXT NOT NULL
);

CREATE TABLE lingua_concept (
    lingua_uid  TEXT NOT NULL REFERENCES lingua(uid),
    concept_uid TEXT NOT NULL REFERENCES concept(uid),
    adopted_at  TEXT NOT NULL,
    PRIMARY KEY (lingua_uid, concept_uid)
);

CREATE TABLE concept_name (
    concept_uid TEXT NOT NULL REFERENCES concept(uid),
    lang        TEXT NOT NULL,
    name        TEXT NOT NULL,
    UNIQUE(concept_uid, lang, name)
);

CREATE TABLE concept_parent (
    concept_uid TEXT NOT NULL REFERENCES concept(uid),
    parent_uid  TEXT NOT NULL REFERENCES concept(uid),
    UNIQUE(concept_uid, parent_uid)
);

CREATE TABLE concept_equivalence (
    a_uid TEXT NOT NULL REFERENCES concept(uid),
    b_uid TEXT NOT NULL REFERENCES concept(uid),
    declared_by TEXT,
    UNIQUE(a_uid, b_uid)
);

CREATE TABLE record_assertion (
    uid               TEXT PRIMARY KEY,
    subject_uid       TEXT NOT NULL REFERENCES record(uid),
    predicate_uid     TEXT NOT NULL REFERENCES concept(uid),
    object_uid        TEXT REFERENCES record(uid),
    role              TEXT NOT NULL DEFAULT 'ordinary'
                      CHECK (role IN ('ordinary', 'identity')),
    quantity_mantissa TEXT,
    quantity_scale    INTEGER,
    unit_uid          TEXT REFERENCES concept(uid),
    asserted_by       TEXT,
    created_at        TEXT NOT NULL,
    retracted_at      TEXT,
    retracted_by      TEXT,
    CHECK ((quantity_mantissa IS NULL) = (quantity_scale IS NULL)),
    CHECK (role != 'identity' OR
           (object_uid IS NULL AND quantity_mantissa IS NULL AND unit_uid IS NULL))
);

CREATE TABLE promise (
    uid          TEXT PRIMARY KEY,
    record_uid   TEXT REFERENCES record(uid),
    concept_uid  TEXT REFERENCES concept(uid),
    delta        REAL NOT NULL,
    window_start TEXT,
    window_end   TEXT,
    party_uid    TEXT,
    state        TEXT NOT NULL DEFAULT 'proposed',
    condition    TEXT,
    transfer_uid TEXT,
    rule_uid     TEXT,
    reserve_from TEXT NOT NULL DEFAULT 'active',
    signature    TEXT,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0), unit_uid TEXT REFERENCES concept(uid), location_lat REAL
    CHECK (location_lat IS NULL OR location_lat BETWEEN -90 AND 90), location_lon REAL
    CHECK (location_lon IS NULL OR location_lon BETWEEN -180 AND 180), location_address TEXT
    CHECK ((location_lat IS NULL) = (location_lon IS NULL)), open_reuse_policy TEXT NOT NULL DEFAULT 'duplicate'
        CHECK (open_reuse_policy IN ('duplicate', 'consume')), source_promise_uid TEXT REFERENCES promise(uid), item_json TEXT CHECK (item_json IS NULL OR json_valid(item_json)),
    CHECK (record_uid IS NOT NULL OR concept_uid IS NOT NULL OR transfer_uid IS NOT NULL)
);

CREATE TABLE place (
    uid     TEXT PRIMARY KEY,
    lat     REAL,
    lon     REAL,
    address TEXT,
    area    TEXT
);

CREATE TABLE record_extension (
    record_uid TEXT NOT NULL REFERENCES record(uid),
    namespace  TEXT NOT NULL,
    version    INTEGER NOT NULL DEFAULT 1,
    fds        TEXT NOT NULL CHECK (json_valid(fds)),
    UNIQUE(record_uid, namespace)
);

CREATE TABLE signal (
    record_uid  TEXT PRIMARY KEY REFERENCES record(uid),
    source_kind TEXT NOT NULL,
    source      TEXT NOT NULL,
    schedule    TEXT NOT NULL,
    parse       TEXT NOT NULL DEFAULT 'number',
    last_sampled_at TEXT
, actor_uid TEXT);

CREATE TABLE effect_queue (
    uid        TEXT PRIMARY KEY,
    kind       TEXT NOT NULL,
    payload    TEXT NOT NULL,
    origin_uid TEXT,
    status     TEXT NOT NULL DEFAULT 'queued',
    attempts   INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    finished_at TEXT,
    result     TEXT
, request_id TEXT);

CREATE TABLE decision (
    record_uid  TEXT PRIMARY KEY REFERENCES record(uid),
    subject_uid TEXT NOT NULL,
    kind        TEXT NOT NULL,
    options     TEXT NOT NULL CHECK (json_valid(options)),
    default_opt TEXT,
    expires_at  TEXT,
    decided_at  TEXT,
    answer      TEXT
);

CREATE TABLE transfer (
    record_uid     TEXT PRIMARY KEY REFERENCES record(uid),
    agreement_type TEXT NOT NULL DEFAULT 'individual',
    agreement_pct  INTEGER,
    settlement     TEXT NOT NULL DEFAULT 'individual',
    visibility     TEXT NOT NULL DEFAULT 'hidden',
    max_proximity  INTEGER,
    satiation      TEXT,
    parent_uid     TEXT REFERENCES transfer(record_uid),
    source_uid     TEXT
, reserve_default TEXT, require_confirmation INTEGER NOT NULL DEFAULT 0, revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0), default_location_lat REAL
    CHECK (default_location_lat IS NULL OR default_location_lat BETWEEN -90 AND 90), default_location_lon REAL
    CHECK (default_location_lon IS NULL OR default_location_lon BETWEEN -180 AND 180), default_location_address TEXT
    CHECK ((default_location_lat IS NULL) = (default_location_lon IS NULL)));

CREATE TABLE transfer_party (
    uid          TEXT PRIMARY KEY,
    transfer_uid TEXT NOT NULL REFERENCES transfer(record_uid),
    actor_uid    TEXT NOT NULL,
    kind         TEXT NOT NULL DEFAULT 'participant',
    UNIQUE(transfer_uid, actor_uid)
);

CREATE TABLE transfer_agreement (
    uid          TEXT PRIMARY KEY,
    transfer_uid TEXT NOT NULL REFERENCES transfer(record_uid),
    party_uid    TEXT NOT NULL REFERENCES transfer_party(uid),
    level        INTEGER NOT NULL DEFAULT 0,
    at           TEXT NOT NULL, revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0), last_event_uid TEXT,
    UNIQUE(transfer_uid, party_uid)
);

CREATE TABLE visibility_rule (
    uid          TEXT PRIMARY KEY,
    subject_kind TEXT NOT NULL,
    subject_uid  TEXT,
    target_uid   TEXT NOT NULL,
    field        TEXT,
    grant_level  TEXT NOT NULL
);

CREATE TABLE identity_key (
    actor_uid  TEXT NOT NULL,
    key_id     TEXT NOT NULL,
    public_key TEXT NOT NULL,
    UNIQUE(actor_uid, key_id)
);

CREATE TABLE configuration (
    id                           INTEGER PRIMARY KEY CHECK (id = 1),
    quantity                     INTEGER NOT NULL DEFAULT 0,
    name                         TEXT NOT NULL DEFAULT 'Default',
    language                     TEXT NOT NULL DEFAULT 'en',
    timezone                     INTEGER NOT NULL DEFAULT 0,
    style                        TEXT NOT NULL DEFAULT 'catppuccin_macchiato',
    command_notification_seconds REAL NOT NULL DEFAULT 0,
    delete_confirmation          INTEGER NOT NULL DEFAULT 1,
    error_toast_seconds          REAL NOT NULL DEFAULT 5,
    keybinding_mode              INTEGER NOT NULL DEFAULT 0,
    created_at                   TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at                   TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP

, attention_budget_per_day INTEGER NOT NULL DEFAULT 12, transfer_reservation_default TEXT NOT NULL DEFAULT 'none'
        CHECK (transfer_reservation_default IN ('none', 'proposed', 'agreed', 'active')), transfer_application_formula TEXT NOT NULL DEFAULT 'incoming()'
        CHECK (length(trim(transfer_application_formula)) > 0), transfer_remainder_policy TEXT NOT NULL DEFAULT 'visible'
        CHECK (transfer_remainder_policy IN ('visible', 'local_draft')), storage_budget_bytes INTEGER NOT NULL DEFAULT 2147483648, interface_snapshot_seconds INTEGER NOT NULL DEFAULT 30 CHECK (interface_snapshot_seconds BETWEEN 1 AND 86400), interface_history_seconds INTEGER NOT NULL DEFAULT 300 CHECK (interface_history_seconds BETWEEN interface_snapshot_seconds AND 604800), interface_history_count INTEGER NOT NULL DEFAULT 10 CHECK (interface_history_count BETWEEN 1 AND 100), interface_close_suspends INTEGER NOT NULL DEFAULT 1 CHECK (interface_close_suspends IN (0, 1)), sand_delete_confirmation INTEGER NOT NULL DEFAULT 1 CHECK (sand_delete_confirmation IN (0, 1))) STRICT;

CREATE TABLE role (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE CHECK (length(trim(name)) > 0)
) STRICT;

CREATE TABLE permission (
    id          INTEGER PRIMARY KEY,
    subject     TEXT NOT NULL CHECK (length(trim(subject)) > 0),
    action      TEXT NOT NULL CHECK (length(trim(action)) > 0),
    description TEXT CHECK (description IS NULL OR length(trim(description)) > 0),
    UNIQUE(subject, action)
) STRICT;

CREATE TABLE role_permission (
    role_id       INTEGER NOT NULL REFERENCES role(id) ON DELETE CASCADE CHECK (role_id > 0),
    permission_id INTEGER NOT NULL REFERENCES permission(id) ON DELETE CASCADE CHECK (permission_id > 0),
    PRIMARY KEY (role_id, permission_id)
) STRICT;

CREATE TABLE concept_conversion (
    a_uid       TEXT NOT NULL REFERENCES concept(uid),
    b_uid       TEXT NOT NULL REFERENCES concept(uid),
    numerator   TEXT NOT NULL,
    denominator TEXT NOT NULL,
    UNIQUE(a_uid, b_uid)
);

CREATE TABLE retention_policy (
    kind            TEXT PRIMARY KEY,
    horizon_seconds INTEGER NOT NULL CHECK (horizon_seconds >= 0)
);

CREATE TABLE sense_rule (
    record_uid     TEXT PRIMARY KEY REFERENCES record(uid),
    watch_concept  TEXT,
    max_proximity  INTEGER NOT NULL DEFAULT 1,
    min_confidence REAL NOT NULL DEFAULT 0.0,
    auto           TEXT NOT NULL DEFAULT 'draft_only'

);

CREATE TABLE discovery_cache (
    promise_uid  TEXT PRIMARY KEY,
    organ        TEXT NOT NULL,
    proximity    INTEGER NOT NULL,
    concept      TEXT,
    unit         TEXT,
    delta        REAL NOT NULL,
    window_start TEXT,
    window_end   TEXT,
    confidence   REAL NOT NULL DEFAULT 0.5,
    fetched_at   TEXT NOT NULL
);

CREATE TABLE organ_contact (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),
    trust      TEXT NOT NULL DEFAULT 'unknown',
    proximity  INTEGER NOT NULL DEFAULT 1,
    sync_out   INTEGER NOT NULL DEFAULT 0,
    sync_in    INTEGER NOT NULL DEFAULT 0
, last_synced_seq INTEGER NOT NULL DEFAULT 0, mode TEXT NOT NULL DEFAULT 'replica', catchup_interval_secs INTEGER NOT NULL DEFAULT 30, node_id TEXT, pending_introduction INTEGER NOT NULL DEFAULT 0, peer_acked_seq INTEGER NOT NULL DEFAULT 0, scope_fields TEXT, scope_version INTEGER NOT NULL DEFAULT 0, accept_fields TEXT, accept_version INTEGER NOT NULL DEFAULT 0, unreachable_since TEXT, mailed_at TEXT, share_protein TEXT, closed_by_default INTEGER NOT NULL DEFAULT 0, share_seen_seq INTEGER, awaiting_roster_since TEXT);

CREATE TABLE sync_quarantine (
    uid        TEXT PRIMARY KEY,
    from_organ TEXT NOT NULL,
    reason     TEXT NOT NULL,
    payload    TEXT NOT NULL,
    at         TEXT NOT NULL
);

CREATE TABLE transfer_revision (
    transfer_uid    TEXT NOT NULL REFERENCES transfer(record_uid),
    revision        INTEGER NOT NULL CHECK (revision > 0),
    fact_uid        TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    idempotency_key TEXT,
    created_at      TEXT NOT NULL,
    PRIMARY KEY (transfer_uid, revision),
    UNIQUE (idempotency_key),
    CHECK (idempotency_key IS NULL OR length(trim(idempotency_key)) > 0)
) STRICT;

CREATE TABLE transfer_invitation (
    uid                   TEXT PRIMARY KEY,
    transfer_uid          TEXT NOT NULL REFERENCES transfer(record_uid),
    addressed_person_uid  TEXT NOT NULL REFERENCES record(uid),
    invited_by_person_uid TEXT NOT NULL REFERENCES record(uid),
    status                TEXT NOT NULL DEFAULT 'pending'
                          CHECK (status IN ('pending', 'accepted', 'rejected',
                                            'withdrawn', 'expired')),
    party_uid             TEXT UNIQUE,
    expires_at            TEXT,
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL, attempt INTEGER NOT NULL DEFAULT 1 CHECK (attempt > 0),
    CHECK (
        (status = 'accepted' AND party_uid IS NOT NULL)
        OR (status != 'accepted' AND party_uid IS NULL)
    ),
    FOREIGN KEY (party_uid, transfer_uid, addressed_person_uid)
        REFERENCES transfer_party(uid, transfer_uid, actor_uid)
);

CREATE TABLE transfer_invitation_event (
    uid             TEXT PRIMARY KEY,
    invitation_uid  TEXT NOT NULL REFERENCES transfer_invitation(uid),
    transfer_uid    TEXT NOT NULL REFERENCES transfer(record_uid),
    attempt         INTEGER NOT NULL CHECK (attempt > 0),
    kind            TEXT NOT NULL CHECK (kind IN (
                        'addressed', 'accepted', 'rejected', 'withdrawn',
                        'expired', 'reopened'
                    )),
    actor_uid       TEXT REFERENCES record(uid),
    revision        INTEGER CHECK (revision IS NULL OR revision > 0),
    fact_uid        TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    idempotency_key TEXT NOT NULL UNIQUE CHECK (length(trim(idempotency_key)) > 0),
    created_at      TEXT NOT NULL,
    FOREIGN KEY (invitation_uid, transfer_uid)
        REFERENCES transfer_invitation(uid, transfer_uid)
) STRICT;

CREATE TABLE transfer_agreement_event (
    uid             TEXT PRIMARY KEY,
    transfer_uid    TEXT NOT NULL REFERENCES transfer(record_uid),
    revision        INTEGER NOT NULL CHECK (revision > 0),
    party_uid       TEXT NOT NULL REFERENCES transfer_party(uid),
    person_uid      TEXT NOT NULL REFERENCES record(uid),
    from_level      INTEGER NOT NULL CHECK (from_level BETWEEN 0 AND 2),
    to_level        INTEGER NOT NULL CHECK (to_level BETWEEN 0 AND 2),
    fact_uid        TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    idempotency_key TEXT NOT NULL UNIQUE CHECK (length(trim(idempotency_key)) > 0),
    created_at      TEXT NOT NULL,
    CHECK (abs(to_level - from_level) = 1),
    FOREIGN KEY (party_uid, transfer_uid, person_uid)
        REFERENCES transfer_party(uid, transfer_uid, actor_uid)
) STRICT;

CREATE TABLE transfer_agreement_coalition (
    transfer_uid   TEXT NOT NULL REFERENCES transfer(record_uid),
    revision       INTEGER NOT NULL CHECK (revision > 0),
    threshold_pct  INTEGER NOT NULL CHECK (threshold_pct BETWEEN 1 AND 100),
    eligible_count INTEGER NOT NULL CHECK (eligible_count > 0),
    frozen_by_event_uid TEXT NOT NULL UNIQUE REFERENCES transfer_agreement_event(uid),
    frozen_at      TEXT NOT NULL,
    PRIMARY KEY (transfer_uid, revision)
) STRICT;

CREATE TABLE transfer_agreement_coalition_member (
    transfer_uid TEXT NOT NULL,
    revision     INTEGER NOT NULL,
    party_uid    TEXT NOT NULL REFERENCES transfer_party(uid),
    PRIMARY KEY (transfer_uid, revision, party_uid),
    FOREIGN KEY (transfer_uid, revision)
        REFERENCES transfer_agreement_coalition(transfer_uid, revision),
    FOREIGN KEY (party_uid, transfer_uid)
        REFERENCES transfer_party(uid, transfer_uid)
) STRICT;

CREATE TABLE transfer_dependency (
    uid             TEXT PRIMARY KEY,
    transfer_uid    TEXT NOT NULL REFERENCES transfer(record_uid),
    revision        INTEGER NOT NULL CHECK (revision > 0),
    scope           TEXT NOT NULL CHECK (scope IN ('transfer', 'promise')),
    promise_uid     TEXT REFERENCES promise(uid),
    upstream_kind   TEXT NOT NULL CHECK (upstream_kind IN ('transfer', 'promise')),
    upstream_uid    TEXT NOT NULL CHECK (length(trim(upstream_uid)) > 0),
    required_state  TEXT NOT NULL DEFAULT 'kept' CHECK (required_state IN (
                        'open', 'proposed', 'agreed', 'active', 'kept',
                        'broken', 'withdrawn'
                    )), upstream_origin_uid TEXT, upstream_reference_uid TEXT REFERENCES transfer_remote_reference(uid),
    CHECK (
        (scope = 'transfer' AND promise_uid IS NULL)
        OR (scope = 'promise' AND promise_uid IS NOT NULL)
    )
) STRICT;

CREATE TABLE transfer_phase4_request (
    idempotency_key TEXT PRIMARY KEY CHECK (length(trim(idempotency_key)) > 0),
    kind            TEXT NOT NULL CHECK (kind IN ('activation', 'claim', 'application')),
    target_uid      TEXT NOT NULL,
    fact_uid        TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at      TEXT NOT NULL
) STRICT;

CREATE TABLE transfer_exchange_path (
    uid                  TEXT PRIMARY KEY,
    transfer_uid         TEXT NOT NULL REFERENCES transfer(record_uid),
    revision             INTEGER NOT NULL CHECK (revision > 0),
    primary_promise_uid  TEXT NOT NULL REFERENCES promise(uid),
    opposite_promise_uid TEXT REFERENCES promise(uid),
    created_at           TEXT NOT NULL, public_exchange_uid TEXT,
    CHECK (opposite_promise_uid IS NULL OR opposite_promise_uid != primary_promise_uid),
    UNIQUE (transfer_uid, revision, primary_promise_uid),
    UNIQUE (uid, transfer_uid, revision),
    FOREIGN KEY (primary_promise_uid, transfer_uid)
        REFERENCES promise(uid, transfer_uid),
    FOREIGN KEY (opposite_promise_uid, transfer_uid)
        REFERENCES promise(uid, transfer_uid)
) STRICT;

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
    created_at          TEXT NOT NULL, system_disputed INTEGER NOT NULL DEFAULT 0
        CHECK (system_disputed IN (0, 1)), system_dispute_fact_uid TEXT REFERENCES fact(uid), system_disputed_at TEXT,
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

CREATE TABLE signed_action_intent (
    uid                 TEXT PRIMARY KEY,
    session_id          TEXT NOT NULL CHECK (length(trim(session_id)) > 0),
    session_challenge   TEXT NOT NULL CHECK (length(trim(session_challenge)) > 0),
    sequence            INTEGER NOT NULL CHECK (sequence > 0),
    message_id          TEXT NOT NULL CHECK (length(trim(message_id)) > 0),
    actor_person_uid    TEXT NOT NULL REFERENCES record(uid),
    key_id              TEXT NOT NULL CHECK (length(trim(key_id)) > 0),
    action_base64       TEXT NOT NULL CHECK (length(action_base64) > 0),
    signature           TEXT NOT NULL CHECK (length(signature) > 0),
    status              TEXT NOT NULL DEFAULT 'pending'
                        CHECK (status IN ('pending', 'committed', 'failed')),
    error_code          TEXT,
    error_message       TEXT,
    received_at         TEXT NOT NULL,
    finished_at         TEXT,
    UNIQUE (session_id, sequence),
    UNIQUE (session_id, message_id),
    FOREIGN KEY (actor_person_uid, key_id)
        REFERENCES identity_key(actor_uid, key_id),
    CHECK ((status = 'pending' AND finished_at IS NULL
            AND error_code IS NULL AND error_message IS NULL)
        OR (status = 'committed' AND finished_at IS NOT NULL
            AND error_code IS NULL AND error_message IS NULL)
        OR (status = 'failed' AND finished_at IS NOT NULL
            AND error_message IS NOT NULL))
) STRICT;

CREATE TABLE fact_action_intent (
    fact_uid    TEXT PRIMARY KEY REFERENCES fact(uid),
    intent_uid  TEXT NOT NULL REFERENCES signed_action_intent(uid)
) STRICT;

CREATE TABLE transfer_phase5_request (
    idempotency_key TEXT PRIMARY KEY CHECK (length(trim(idempotency_key)) > 0),
    kind            TEXT NOT NULL CHECK (kind = 'settlement'),
    target_uid      TEXT NOT NULL,
    fact_uid        TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at      TEXT NOT NULL
) STRICT;

CREATE TABLE transfer_occurrence_remainder_policy (
    occurrence_uid   TEXT PRIMARY KEY REFERENCES transfer_occurrence(uid),
    owner_person_uid TEXT NOT NULL REFERENCES record(uid),
    policy           TEXT NOT NULL CHECK (policy IN ('visible', 'local_draft')),
    updated_at       TEXT NOT NULL
) STRICT;

CREATE TABLE transfer_occurrence_settlement_slice (
    uid                       TEXT PRIMARY KEY,
    occurrence_uid            TEXT NOT NULL REFERENCES transfer_occurrence(uid),
    transfer_uid              TEXT NOT NULL REFERENCES transfer(record_uid),
    promise_uid               TEXT NOT NULL REFERENCES promise(uid),
    owner_person_uid          TEXT NOT NULL REFERENCES record(uid),
    canonical_quantity        REAL NOT NULL CHECK (canonical_quantity > 0),
    canonical_unit_uid        TEXT REFERENCES concept(uid),
    cumulative_before         REAL NOT NULL CHECK (cumulative_before >= 0),
    cumulative_after          REAL NOT NULL CHECK (cumulative_after > cumulative_before),
    remaining_after           REAL NOT NULL CHECK (remaining_after >= 0),
    evidence_fact_uid         TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    application_fact_uid      TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    local_record_uid          TEXT NOT NULL REFERENCES record(uid),
    local_delta               REAL NOT NULL,
    local_cumulative_before   REAL NOT NULL,
    local_cumulative_after    REAL NOT NULL,
    application_formula       TEXT NOT NULL CHECK (length(trim(application_formula)) > 0),
    application_formula_hash  TEXT NOT NULL CHECK (length(application_formula_hash) = 64),
    application_formula_version INTEGER NOT NULL CHECK (application_formula_version >= 0),
    remainder_policy          TEXT NOT NULL CHECK (remainder_policy IN ('visible', 'local_draft')),
    idempotency_key           TEXT NOT NULL UNIQUE REFERENCES transfer_phase5_request(idempotency_key),
    created_at                TEXT NOT NULL,
    CHECK (evidence_fact_uid != application_fact_uid)
) STRICT;

CREATE TABLE transfer_phase5_correction_request (
    idempotency_key TEXT PRIMARY KEY CHECK (length(trim(idempotency_key)) > 0),
    kind            TEXT NOT NULL CHECK (kind IN ('compensation', 'dispute')),
    target_uid      TEXT NOT NULL,
    fact_uid        TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at      TEXT NOT NULL
) STRICT;

CREATE TABLE transfer_occurrence_settlement_compensation (
    uid                           TEXT PRIMARY KEY,
    settlement_uid                TEXT NOT NULL UNIQUE
        REFERENCES transfer_occurrence_settlement_slice(uid),
    occurrence_uid                TEXT NOT NULL REFERENCES transfer_occurrence(uid),
    transfer_uid                  TEXT NOT NULL REFERENCES transfer(record_uid),
    owner_person_uid              TEXT NOT NULL REFERENCES record(uid),
    original_application_fact_uid TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    compensation_fact_uid         TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    local_record_uid              TEXT NOT NULL REFERENCES record(uid),
    inverse_delta                 REAL NOT NULL,
    idempotency_key               TEXT NOT NULL UNIQUE
        REFERENCES transfer_phase5_correction_request(idempotency_key),
    created_at                    TEXT NOT NULL
) STRICT;

CREATE TABLE transfer_occurrence_dispute_event (
    uid              TEXT PRIMARY KEY,
    occurrence_uid   TEXT NOT NULL REFERENCES transfer_occurrence(uid),
    transfer_uid     TEXT NOT NULL REFERENCES transfer(record_uid),
    actor_person_uid TEXT NOT NULL REFERENCES record(uid),
    asserted         INTEGER NOT NULL CHECK (asserted IN (0, 1)),
    fact_uid         TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    idempotency_key  TEXT NOT NULL UNIQUE
        REFERENCES transfer_phase5_correction_request(idempotency_key),
    created_at       TEXT NOT NULL
) STRICT;

CREATE TABLE transfer_source_group_result (
    uid                 TEXT PRIMARY KEY,
    source_uid          TEXT NOT NULL REFERENCES record(uid),
    policy              TEXT NOT NULL CHECK (policy = 'first_completes'),
    transfer_uid        TEXT NOT NULL REFERENCES transfer(record_uid),
    transfer_revision   INTEGER NOT NULL CHECK (transfer_revision > 0),
    settlement_uid      TEXT NOT NULL UNIQUE REFERENCES transfer_occurrence_settlement_slice(uid),
    fact_uid            TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at          TEXT NOT NULL,
    UNIQUE (source_uid, policy)
) STRICT;

CREATE TABLE transfer_source_group_loser (
    uid                 TEXT PRIMARY KEY,
    result_uid          TEXT NOT NULL REFERENCES transfer_source_group_result(uid),
    transfer_uid        TEXT NOT NULL REFERENCES transfer(record_uid),
    transfer_revision   INTEGER NOT NULL CHECK (transfer_revision > 0),
    fact_uid            TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at          TEXT NOT NULL,
    UNIQUE (result_uid, transfer_uid)
) STRICT;

CREATE TABLE transfer_phase6_bulk_request (
    uid                 TEXT PRIMARY KEY,
    idempotency_key     TEXT NOT NULL UNIQUE CHECK (length(trim(idempotency_key)) > 0),
    actor_person_uid    TEXT NOT NULL REFERENCES record(uid),
    review_token        TEXT NOT NULL CHECK (length(review_token) = 64),
    item_count          INTEGER NOT NULL CHECK (item_count > 0),
    created_at          TEXT NOT NULL
) STRICT;

CREATE TABLE transfer_phase6_bulk_item (
    bulk_uid                    TEXT NOT NULL REFERENCES transfer_phase6_bulk_request(uid),
    ordinal                     INTEGER NOT NULL CHECK (ordinal >= 0),
    occurrence_uid              TEXT NOT NULL REFERENCES transfer_occurrence(uid),
    transfer_uid                TEXT NOT NULL REFERENCES transfer(record_uid),
    transfer_revision           INTEGER NOT NULL CHECK (transfer_revision > 0),
    role                        TEXT NOT NULL CHECK (role IN ('delivery', 'receipt')),
    expected_delivery_claimed   INTEGER NOT NULL CHECK (expected_delivery_claimed IN (0, 1)),
    expected_receipt_claimed    INTEGER NOT NULL CHECK (expected_receipt_claimed IN (0, 1)),
    claim_event_uid             TEXT NOT NULL UNIQUE REFERENCES transfer_occurrence_claim_event(uid),
    fact_uid                    TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    PRIMARY KEY (bulk_uid, ordinal),
    UNIQUE (bulk_uid, occurrence_uid, role)
) STRICT;

CREATE TABLE transfer_correction_link (
    uid                    TEXT PRIMARY KEY,
    kind                   TEXT NOT NULL CHECK (kind IN ('remainder', 'reversal')),
    source_transfer_uid    TEXT NOT NULL REFERENCES transfer(record_uid),
    source_occurrence_uid  TEXT NOT NULL REFERENCES transfer_occurrence(uid),
    created_transfer_uid   TEXT NOT NULL UNIQUE REFERENCES transfer(record_uid),
    source_revision        INTEGER NOT NULL CHECK (source_revision > 0),
    canonical_quantity     REAL NOT NULL CHECK (canonical_quantity > 0),
    actor_person_uid       TEXT NOT NULL REFERENCES record(uid),
    fact_uid               TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    idempotency_key        TEXT NOT NULL UNIQUE
        REFERENCES transfer_revision(idempotency_key),
    created_at             TEXT NOT NULL
) STRICT;

CREATE TABLE transfer_promise_successor (
    uid                     TEXT PRIMARY KEY,
    transfer_uid            TEXT NOT NULL REFERENCES transfer(record_uid),
    predecessor_promise_uid TEXT NOT NULL UNIQUE REFERENCES promise(uid),
    successor_promise_uid   TEXT NOT NULL UNIQUE REFERENCES promise(uid),
    revision                INTEGER NOT NULL CHECK (revision > 0),
    actor_person_uid        TEXT NOT NULL REFERENCES record(uid),
    fact_uid                TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    idempotency_key         TEXT NOT NULL UNIQUE
        REFERENCES transfer_revision(idempotency_key),
    created_at              TEXT NOT NULL,
    CHECK (predecessor_promise_uid != successor_promise_uid)
) STRICT;

CREATE TABLE transfer_delivery_policy (
    uid                  TEXT PRIMARY KEY,
    transfer_uid         TEXT NOT NULL REFERENCES transfer(record_uid),
    origin_organ_uid     TEXT NOT NULL,
    recipient_person_uid TEXT NOT NULL,
    recipient_organ_uid  TEXT NOT NULL,
    mode                 TEXT NOT NULL DEFAULT 'hosted'
                         CHECK (mode IN ('hosted', 'replicated')),
    state                TEXT NOT NULL DEFAULT 'active'
                         CHECK (state IN ('active', 'revoked')),
    revision             INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0),
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    UNIQUE (transfer_uid, recipient_person_uid, recipient_organ_uid)
) STRICT;

CREATE TABLE transfer_delivery_policy_event (
    uid                  TEXT PRIMARY KEY,
    delivery_uid         TEXT NOT NULL REFERENCES transfer_delivery_policy(uid),
    revision             INTEGER NOT NULL CHECK (revision > 0),
    kind                 TEXT NOT NULL CHECK (kind IN ('created', 'mode_changed', 'revoked')),
    from_mode            TEXT CHECK (from_mode IS NULL OR from_mode IN ('hosted', 'replicated')),
    to_mode              TEXT NOT NULL CHECK (to_mode IN ('hosted', 'replicated')),
    from_state           TEXT CHECK (from_state IS NULL OR from_state IN ('active', 'revoked')),
    to_state             TEXT NOT NULL CHECK (to_state IN ('active', 'revoked')),
    actor_person_uid     TEXT NOT NULL,
    fact_uid             TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    request_id           TEXT NOT NULL UNIQUE,
    created_at           TEXT NOT NULL,
    UNIQUE (delivery_uid, revision),
    CHECK (
        (kind = 'created' AND revision = 1 AND from_mode IS NULL AND from_state IS NULL)
        OR (kind = 'mode_changed' AND from_mode IS NOT NULL AND from_state = 'active' AND to_state = 'active')
        OR (kind = 'revoked' AND from_mode IS NOT NULL AND from_state = 'active' AND to_state = 'revoked')
    )
) STRICT;

CREATE TABLE transfer_delivery_outbox (
    uid               TEXT PRIMARY KEY,
    envelope_uid      TEXT NOT NULL UNIQUE,
    delivery_uid      TEXT NOT NULL REFERENCES transfer_delivery_policy(uid),
    cursor            INTEGER NOT NULL CHECK (cursor > 0),
    transfer_revision INTEGER NOT NULL CHECK (transfer_revision > 0),
    payload            TEXT NOT NULL CHECK (json_valid(payload)),
    payload_hash       TEXT NOT NULL,
    status             TEXT NOT NULL DEFAULT 'queued'
                       CHECK (status IN ('queued', 'sent', 'failed', 'cancelled')),
    attempts           INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_attempt_at    TEXT NOT NULL,
    last_attempt_at    TEXT,
    last_error         TEXT,
    acknowledged_cursor INTEGER CHECK (acknowledged_cursor IS NULL OR acknowledged_cursor >= cursor),
    created_at         TEXT NOT NULL,
    sent_at            TEXT,
    UNIQUE (delivery_uid, cursor)
) STRICT;

CREATE TABLE transfer_delivery_retry_event (
    uid          TEXT PRIMARY KEY,
    delivery_uid TEXT NOT NULL REFERENCES transfer_delivery_policy(uid),
    outbox_uid   TEXT NOT NULL REFERENCES transfer_delivery_outbox(uid),
    request_id   TEXT NOT NULL UNIQUE,
    created_at   TEXT NOT NULL
) STRICT;

CREATE TABLE transfer_delivery_receipt (
    uid               TEXT PRIMARY KEY,
    delivery_uid      TEXT NOT NULL,
    envelope_uid      TEXT NOT NULL,
    cursor            INTEGER NOT NULL CHECK (cursor > 0),
    kind              TEXT NOT NULL CHECK (kind IN ('received', 'seen')),
    actor_organ_uid   TEXT NOT NULL,
    payload_hash      TEXT NOT NULL,
    key_id            TEXT NOT NULL,
    signature         TEXT NOT NULL,
    signed_payload    TEXT NOT NULL CHECK (json_valid(signed_payload)),
    local_fact_uid    TEXT UNIQUE REFERENCES fact(uid),
    request_id        TEXT NOT NULL UNIQUE,
    created_at        TEXT NOT NULL,
    UNIQUE (delivery_uid, envelope_uid, kind, actor_organ_uid)
) STRICT;

CREATE TABLE transfer_remote_reference (
    uid                  TEXT PRIMARY KEY,
    origin_organ_uid     TEXT NOT NULL,
    transfer_uid         TEXT NOT NULL,
    delivery_policy_uid  TEXT NOT NULL,
    recipient_person_uid TEXT NOT NULL,
    recipient_organ_uid  TEXT NOT NULL,
    mode                 TEXT NOT NULL DEFAULT 'hosted'
                         CHECK (mode IN ('hosted', 'replicated')),
    state                TEXT NOT NULL DEFAULT 'active'
                         CHECK (state IN ('active', 'revoked')),
    policy_revision      INTEGER NOT NULL DEFAULT 1 CHECK (policy_revision > 0),
    hosted_url           TEXT,
    last_cursor          INTEGER NOT NULL DEFAULT 0 CHECK (last_cursor >= 0),
    last_transfer_revision INTEGER NOT NULL DEFAULT 0 CHECK (last_transfer_revision >= 0),
    last_envelope_uid    TEXT,
    last_payload_hash    TEXT,
    projection           TEXT CHECK (projection IS NULL OR json_valid(projection)),
    disclosure           TEXT CHECK (disclosure IS NULL OR json_valid(disclosure)),
    last_fetched_at      TEXT,
    last_error           TEXT,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    UNIQUE (origin_organ_uid, transfer_uid, recipient_person_uid, recipient_organ_uid)
) STRICT;

CREATE TABLE transfer_remote_policy_event (
    uid             TEXT PRIMARY KEY,
    reference_uid   TEXT NOT NULL REFERENCES transfer_remote_reference(uid),
    policy_revision INTEGER NOT NULL CHECK (policy_revision > 0),
    kind            TEXT NOT NULL CHECK (kind IN ('reference', 'mode_changed', 'revoked')),
    mode            TEXT NOT NULL CHECK (mode IN ('hosted', 'replicated')),
    state           TEXT NOT NULL CHECK (state IN ('active', 'revoked')),
    envelope_uid    TEXT,
    payload_hash    TEXT NOT NULL,
    signed_payload  TEXT NOT NULL CHECK (json_valid(signed_payload)),
    received_at     TEXT NOT NULL,
    UNIQUE (reference_uid, policy_revision)
) STRICT;

CREATE TABLE transfer_delivery_pull_request (
    uid             TEXT PRIMARY KEY,
    reference_uid   TEXT NOT NULL REFERENCES transfer_remote_reference(uid),
    request_id      TEXT NOT NULL UNIQUE,
    after_cursor    INTEGER NOT NULL CHECK (after_cursor >= 0),
    status          TEXT NOT NULL DEFAULT 'queued'
                    CHECK (status IN ('queued', 'failed', 'completed', 'cancelled')),
    attempts        INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_attempt_at TEXT NOT NULL,
    last_attempt_at TEXT,
    last_error      TEXT,
    completed_cursor INTEGER CHECK (completed_cursor IS NULL OR completed_cursor >= after_cursor),
    created_at      TEXT NOT NULL,
    completed_at    TEXT
) STRICT;

CREATE TABLE transfer_replica_envelope (
    envelope_uid      TEXT PRIMARY KEY,
    reference_uid     TEXT NOT NULL REFERENCES transfer_remote_reference(uid),
    origin_organ_uid  TEXT NOT NULL,
    transfer_uid      TEXT NOT NULL,
    recipient_person_uid TEXT NOT NULL,
    recipient_organ_uid  TEXT NOT NULL,
    cursor            INTEGER NOT NULL CHECK (cursor > 0),
    transfer_revision INTEGER NOT NULL CHECK (transfer_revision > 0),
    payload           TEXT NOT NULL CHECK (json_valid(payload)),
    payload_hash      TEXT NOT NULL,
    received_at       TEXT NOT NULL,
    UNIQUE (reference_uid, cursor)
) STRICT;

CREATE TABLE transfer_remote_conflict (
    uid                    TEXT PRIMARY KEY,
    origin_organ_uid       TEXT NOT NULL,
    transfer_uid           TEXT NOT NULL,
    recipient_person_uid   TEXT NOT NULL,
    command_uid            TEXT,
    request_id             TEXT,
    envelope_uid           TEXT,
    submitted_revision     INTEGER CHECK (submitted_revision IS NULL OR submitted_revision >= 0),
    authoritative_revision INTEGER NOT NULL CHECK (authoritative_revision >= 0),
    authoritative_cursor   INTEGER NOT NULL CHECK (authoritative_cursor >= 0),
    code                   TEXT NOT NULL,
    reviewed_payload       TEXT NOT NULL CHECK (json_valid(reviewed_payload)),
    created_at             TEXT NOT NULL,
    UNIQUE (origin_organ_uid, command_uid),
    UNIQUE (origin_organ_uid, request_id)
) STRICT;

CREATE TABLE transfer_remote_command (
    command_uid           TEXT PRIMARY KEY,
    request_id            TEXT NOT NULL UNIQUE,
    direction             TEXT NOT NULL CHECK (direction IN ('outgoing', 'incoming')),
    origin_organ_uid      TEXT NOT NULL,
    sender_organ_uid      TEXT NOT NULL,
    transfer_uid          TEXT NOT NULL,
    actor_person_uid      TEXT NOT NULL,
    expected_revision     INTEGER CHECK (expected_revision IS NULL OR expected_revision >= 0),
    payload               TEXT NOT NULL CHECK (json_valid(payload)),
    payload_hash          TEXT NOT NULL,
    status                TEXT NOT NULL DEFAULT 'queued'
                          CHECK (status IN ('queued', 'sent', 'accepted', 'rejected', 'failed')),
    attempts              INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_attempt_at       TEXT NOT NULL,
    last_attempt_at       TEXT,
    last_error_code       TEXT,
    last_error            TEXT,
    result_payload        TEXT CHECK (result_payload IS NULL OR json_valid(result_payload)),
    authoritative_revision INTEGER CHECK (authoritative_revision IS NULL OR authoritative_revision >= 0),
    created_at            TEXT NOT NULL,
    finished_at           TEXT
) STRICT;

CREATE TABLE fact_remote_command (
    fact_uid    TEXT PRIMARY KEY REFERENCES fact(uid),
    command_uid TEXT NOT NULL REFERENCES transfer_remote_command(command_uid)
) WITHOUT ROWID, STRICT;

CREATE TABLE organ_transfer_request_nonce (
    sender_organ_uid TEXT NOT NULL,
    nonce            TEXT NOT NULL,
    request_hash     TEXT NOT NULL,
    method           TEXT NOT NULL,
    path             TEXT NOT NULL,
    request_timestamp TEXT NOT NULL,
    received_at      TEXT NOT NULL,
    PRIMARY KEY (sender_organ_uid, nonce)
) WITHOUT ROWID, STRICT;

CREATE TABLE transfer_application_handoff (
    uid                    TEXT PRIMARY KEY,
    origin_organ_uid       TEXT NOT NULL,
    participant_organ_uid  TEXT NOT NULL,
    participant_person_uid TEXT NOT NULL,
    transfer_uid           TEXT NOT NULL,
    occurrence_uid         TEXT NOT NULL,
    settlement_slice_uid   TEXT NOT NULL,
    origin_revision        INTEGER NOT NULL CHECK (origin_revision > 0),
    canonical_slice_hash   TEXT NOT NULL,
    state                  TEXT NOT NULL DEFAULT 'pending'
                           CHECK (state IN ('pending', 'accepted', 'rejected', 'compensated')),
    attestation_uid        TEXT,
    request_id             TEXT NOT NULL UNIQUE,
    created_at             TEXT NOT NULL,
    updated_at             TEXT NOT NULL,
    UNIQUE (origin_organ_uid, settlement_slice_uid, participant_person_uid)
) STRICT;

CREATE TABLE transfer_application_attestation (
    uid                    TEXT PRIMARY KEY,
    origin_organ_uid       TEXT NOT NULL,
    participant_organ_uid  TEXT NOT NULL,
    participant_person_uid TEXT NOT NULL,
    transfer_uid           TEXT NOT NULL,
    occurrence_uid         TEXT NOT NULL,
    settlement_slice_uid   TEXT NOT NULL,
    origin_revision        INTEGER NOT NULL CHECK (origin_revision > 0),
    canonical_slice_hash   TEXT NOT NULL,
    formula_commitment     TEXT NOT NULL,
    formula_version        TEXT NOT NULL,
    application_fact_uid   TEXT NOT NULL,
    applied_at             TEXT NOT NULL,
    key_id                 TEXT NOT NULL,
    signature              TEXT NOT NULL,
    payload                TEXT NOT NULL CHECK (json_valid(payload)),
    received_at            TEXT NOT NULL,
    UNIQUE (origin_organ_uid, settlement_slice_uid, participant_person_uid)
) STRICT;

CREATE TABLE transfer_application_handoff_event (
    uid             TEXT PRIMARY KEY,
    handoff_uid     TEXT NOT NULL REFERENCES transfer_application_handoff(uid),
    kind            TEXT NOT NULL CHECK (kind IN ('pending', 'accepted', 'rejected', 'compensated')),
    from_state      TEXT CHECK (from_state IS NULL OR from_state IN ('pending', 'accepted', 'rejected', 'compensated')),
    to_state        TEXT NOT NULL CHECK (to_state IN ('pending', 'accepted', 'rejected', 'compensated')),
    attestation_uid TEXT REFERENCES transfer_application_attestation(uid),
    fact_uid        TEXT REFERENCES fact(uid),
    reason_code     TEXT,
    request_id      TEXT NOT NULL UNIQUE,
    created_at      TEXT NOT NULL
) STRICT;

CREATE TABLE transfer_remote_application_handoff (
    uid                         TEXT PRIMARY KEY,
    reference_uid              TEXT NOT NULL REFERENCES transfer_remote_reference(uid),
    origin_organ_uid            TEXT NOT NULL,
    participant_organ_uid       TEXT NOT NULL,
    participant_person_uid      TEXT NOT NULL,
    transfer_uid                TEXT NOT NULL,
    occurrence_uid              TEXT NOT NULL,
    source_promise_uid          TEXT NOT NULL,
    settlement_slice_uid        TEXT NOT NULL,
    origin_revision             INTEGER NOT NULL CHECK (origin_revision > 0),
    canonical_quantity          REAL NOT NULL CHECK (canonical_quantity > 0),
    canonical_unit_uid          TEXT,
    canonical_cumulative_before REAL NOT NULL CHECK (canonical_cumulative_before >= 0),
    canonical_cumulative_after  REAL NOT NULL CHECK (canonical_cumulative_after > canonical_cumulative_before),
    canonical_remaining_after   REAL NOT NULL CHECK (canonical_remaining_after >= 0),
    canonical_slice_hash        TEXT NOT NULL,
    envelope_uid                TEXT NOT NULL,
    envelope_payload_hash       TEXT NOT NULL,
    origin_created_at           TEXT NOT NULL,
    state                       TEXT NOT NULL DEFAULT 'pending'
                                CHECK (state IN ('pending', 'applied')),
    local_application_uid       TEXT,
    received_at                 TEXT NOT NULL,
    updated_at                  TEXT NOT NULL, application_direction INTEGER NOT NULL DEFAULT 1
    CHECK (application_direction IN (-1, 1)), origin_state TEXT NOT NULL DEFAULT 'pending'
    CHECK (origin_state IN ('pending', 'accepted', 'rejected', 'compensated')),
    UNIQUE (origin_organ_uid, settlement_slice_uid, participant_person_uid)
) STRICT;

CREATE TABLE transfer_application_handoff_detail (
    handoff_uid                 TEXT PRIMARY KEY
                                REFERENCES transfer_application_handoff(uid),
    source_promise_uid          TEXT NOT NULL,
    canonical_quantity          REAL NOT NULL CHECK (canonical_quantity > 0),
    canonical_unit_uid          TEXT,
    canonical_cumulative_before REAL NOT NULL CHECK (canonical_cumulative_before >= 0),
    canonical_cumulative_after  REAL NOT NULL CHECK (canonical_cumulative_after > canonical_cumulative_before),
    canonical_remaining_after   REAL NOT NULL CHECK (canonical_remaining_after >= 0),
    application_direction       INTEGER NOT NULL CHECK (application_direction IN (-1, 1)),
    origin_evidence_fact_uid    TEXT UNIQUE REFERENCES fact(uid),
    origin_acceptance_fact_uid  TEXT UNIQUE REFERENCES fact(uid),
    created_at                  TEXT NOT NULL
) STRICT;

CREATE TABLE transfer_application_attestation_outbox (
    attestation_uid TEXT PRIMARY KEY,
    handoff_uid     TEXT NOT NULL REFERENCES transfer_remote_application_handoff(uid),
    reference_uid   TEXT NOT NULL REFERENCES transfer_remote_reference(uid),
    origin_organ_uid TEXT NOT NULL,
    payload         TEXT NOT NULL CHECK (json_valid(payload)),
    status          TEXT NOT NULL DEFAULT 'queued'
                    CHECK (status IN ('queued', 'failed', 'sent')),
    attempts        INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    next_attempt_at TEXT NOT NULL,
    last_attempt_at TEXT,
    last_error      TEXT,
    created_at      TEXT NOT NULL,
    sent_at         TEXT
) STRICT;

CREATE TABLE karma_request (
    request_id   TEXT PRIMARY KEY CHECK (length(request_id) BETWEEN 1 AND 200),
    family       TEXT NOT NULL CHECK (length(family) BETWEEN 1 AND 64),
    payload_hash TEXT NOT NULL
                 CHECK (length(payload_hash) = 71 AND payload_hash GLOB 'sha256:[0-9a-f]*'),
    created_at   TEXT NOT NULL
) STRICT;

CREATE TABLE karma_program_revision (
    revision_hash TEXT PRIMARY KEY
                  CHECK (length(revision_hash) = 71 AND revision_hash GLOB 'sha256:[0-9a-f]*'),
    program_uid   TEXT NOT NULL
                  REFERENCES karma_program(record_uid)
                  DEFERRABLE INITIALLY DEFERRED,
    schema_name   TEXT NOT NULL CHECK (schema_name = 'karma.program.v1'),
    ast_json      TEXT NOT NULL CHECK (json_valid(ast_json)),
    canonical_dsl TEXT NOT NULL CHECK (length(canonical_dsl) > 0),
    proof_json    TEXT NOT NULL CHECK (json_valid(proof_json)),
    proof_status  TEXT NOT NULL CHECK (proof_status IN ('accepted', 'rejected')),
    created_at    TEXT NOT NULL,
    UNIQUE (program_uid, revision_hash)
) STRICT;

CREATE TABLE karma_program (
    record_uid           TEXT PRIMARY KEY REFERENCES record(uid),
    handle_revision      INTEGER NOT NULL CHECK (handle_revision >= 1),
    status               TEXT NOT NULL
                         CHECK (status IN ('draft', 'proven', 'active', 'paused', 'retired')),
    head_revision_hash   TEXT NOT NULL
                         REFERENCES karma_program_revision(revision_hash)
                         DEFERRABLE INITIALLY DEFERRED,
    active_revision_hash TEXT
                         REFERENCES karma_program_revision(revision_hash)
                         DEFERRABLE INITIALLY DEFERRED,
    owner_person_uid     TEXT,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    CHECK ((status = 'active') = (active_revision_hash IS NOT NULL))
) STRICT;

CREATE TABLE karma_program_request (
    request_id               TEXT PRIMARY KEY REFERENCES karma_request(request_id),
    action                   TEXT NOT NULL
                             CHECK (action IN ('create', 'revise', 'activate', 'pause')),
    payload_hash             TEXT NOT NULL
                             CHECK (length(payload_hash) = 71 AND payload_hash GLOB 'sha256:[0-9a-f]*'),
    program_uid              TEXT NOT NULL REFERENCES karma_program(record_uid),
    expected_handle_revision INTEGER CHECK (expected_handle_revision IS NULL OR expected_handle_revision >= 1),
    result_handle_revision   INTEGER NOT NULL CHECK (result_handle_revision >= 1),
    result_json              TEXT NOT NULL CHECK (json_valid(result_json)),
    revision_hash            TEXT REFERENCES karma_program_revision(revision_hash),
    fact_uid                 TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at               TEXT NOT NULL
) STRICT;

CREATE TABLE karma_frequency_revision (
    revision_hash         TEXT PRIMARY KEY
                          CHECK (length(revision_hash) = 71 AND revision_hash GLOB 'sha256:[0-9a-f]*'),
    frequency_uid         TEXT NOT NULL
                          REFERENCES karma_frequency(record_uid)
                          DEFERRABLE INITIALLY DEFERRED,
    schema_name           TEXT NOT NULL CHECK (schema_name = 'karma.frequency.v1'),
    ast_json              TEXT NOT NULL CHECK (json_valid(ast_json)),
    canonical_dsl         TEXT NOT NULL CHECK (length(canonical_dsl) > 0),
    default_compiled_json TEXT NOT NULL CHECK (json_valid(default_compiled_json)),
    created_at            TEXT NOT NULL,
    UNIQUE (frequency_uid, revision_hash)
) STRICT;

CREATE TABLE karma_frequency (
    record_uid             TEXT PRIMARY KEY REFERENCES record(uid),
    handle_revision        INTEGER NOT NULL CHECK (handle_revision >= 1),
    status                 TEXT NOT NULL
                           CHECK (status IN ('proven', 'active', 'paused', 'retired')),
    head_revision_hash     TEXT NOT NULL
                           REFERENCES karma_frequency_revision(revision_hash)
                           DEFERRABLE INITIALLY DEFERRED,
    active_revision_hash   TEXT
                           REFERENCES karma_frequency_revision(revision_hash)
                           DEFERRABLE INITIALLY DEFERRED,
    active_activation_hash TEXT
                           REFERENCES karma_frequency_activation(activation_hash)
                           DEFERRABLE INITIALLY DEFERRED,
    latest_activation_hash TEXT
                           REFERENCES karma_frequency_activation(activation_hash)
                           DEFERRABLE INITIALLY DEFERRED,
    owner_person_uid       TEXT,
    created_at             TEXT NOT NULL,
    updated_at             TEXT NOT NULL,
    CHECK ((status = 'active') =
           (active_revision_hash IS NOT NULL AND active_activation_hash IS NOT NULL)),
    CHECK ((active_revision_hash IS NULL) = (active_activation_hash IS NULL)),
    CHECK (active_activation_hash IS NULL OR active_activation_hash = latest_activation_hash)
) STRICT;

CREATE TABLE karma_frequency_activation (
    activation_hash             TEXT PRIMARY KEY
                                CHECK (length(activation_hash) = 71 AND activation_hash GLOB 'sha256:[0-9a-f]*'),
    frequency_uid               TEXT NOT NULL REFERENCES karma_frequency(record_uid),
    activating_handle_revision  INTEGER NOT NULL CHECK (activating_handle_revision >= 2),
    definition_revision_hash    TEXT NOT NULL REFERENCES karma_frequency_revision(revision_hash),
    effective_parameter_hash    TEXT NOT NULL
                                CHECK (length(effective_parameter_hash) = 71 AND effective_parameter_hash GLOB 'sha256:[0-9a-f]*'),
    effective_parameters_json   TEXT NOT NULL CHECK (json_valid(effective_parameters_json)),
    compiled_json               TEXT NOT NULL CHECK (json_valid(compiled_json)),
    epoch_json                  TEXT NOT NULL CHECK (json_valid(epoch_json)),
    previous_activation_hash    TEXT REFERENCES karma_frequency_activation(activation_hash),
    cause_action                TEXT NOT NULL
                                CHECK (cause_action IN ('activate-revision', 'set-parameters', 'reset-parameters')),
    activated_at                TEXT NOT NULL,
    UNIQUE (frequency_uid, activating_handle_revision)
) STRICT;

CREATE TABLE karma_frequency_request (
    request_id               TEXT PRIMARY KEY REFERENCES karma_request(request_id),
    action                   TEXT NOT NULL
                             CHECK (action IN ('create', 'revise', 'activate', 'set-parameters', 'reset-parameters', 'pause')),
    payload_hash             TEXT NOT NULL
                             CHECK (length(payload_hash) = 71 AND payload_hash GLOB 'sha256:[0-9a-f]*'),
    frequency_uid            TEXT NOT NULL REFERENCES karma_frequency(record_uid),
    expected_handle_revision INTEGER CHECK (expected_handle_revision IS NULL OR expected_handle_revision >= 1),
    result_handle_revision   INTEGER NOT NULL CHECK (result_handle_revision >= 1),
    result_json              TEXT NOT NULL CHECK (json_valid(result_json)),
    revision_hash            TEXT REFERENCES karma_frequency_revision(revision_hash),
    activation_hash          TEXT REFERENCES karma_frequency_activation(activation_hash),
    fact_uid                 TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at               TEXT NOT NULL
) STRICT;

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

CREATE TABLE karma_occurrence_sequence (
    singleton     INTEGER PRIMARY KEY CHECK (singleton = 1),
    next_sequence INTEGER NOT NULL CHECK (next_sequence >= 1)
) STRICT;

CREATE TABLE karma_occurrence (
    occurrence_hash       TEXT PRIMARY KEY
                          CHECK (length(occurrence_hash) = 71 AND occurrence_hash GLOB 'sha256:[0-9a-f]*'),
    cell_sequence         INTEGER NOT NULL UNIQUE CHECK (cell_sequence >= 1),
    source_kind           TEXT NOT NULL
                          CHECK (source_kind IN ('schedule-tick', 'schedule-coalesced',
                                                 'calendar-tick', 'calendar-coalesced')),
    source_identity       TEXT NOT NULL
                          CHECK (length(source_identity) = 71 AND source_identity GLOB 'sha256:[0-9a-f]*'),
    logical_at            TEXT NOT NULL,
    parent_occurrence_hash TEXT REFERENCES karma_occurrence(occurrence_hash),
    envelope_json         TEXT NOT NULL CHECK (json_valid(envelope_json)),
    received_at           TEXT NOT NULL,
    UNIQUE (source_kind, source_identity)
) STRICT;

CREATE TABLE karma_schedule_occurrence_expansion (
    schedule_occurrence_hash TEXT PRIMARY KEY
                             REFERENCES karma_schedule_occurrence(occurrence_hash),
    cadence_kind          TEXT NOT NULL CHECK (cadence_kind IN ('elapsed', 'calendar')),
    emission_kind         TEXT NOT NULL CHECK (emission_kind IN ('individual', 'coalesced')),
    next_ordinal          INTEGER NOT NULL DEFAULT 0 CHECK (next_ordinal >= 0),
    total_items           INTEGER NOT NULL CHECK (total_items >= 1),
    completed             INTEGER NOT NULL DEFAULT 0 CHECK (completed IN (0, 1)),
    created_at            TEXT NOT NULL,
    updated_at            TEXT NOT NULL,
    CHECK (next_ordinal <= total_items),
    CHECK ((completed = 1) = (next_ordinal = total_items))
) STRICT;

CREATE TABLE karma_occurrence_processing_state (
    singleton          INTEGER PRIMARY KEY CHECK (singleton = 1),
    next_cell_sequence INTEGER NOT NULL CHECK (next_cell_sequence >= 1)
) STRICT;

CREATE TABLE karma_occurrence_program_epoch (
    occurrence_hash    TEXT PRIMARY KEY REFERENCES karma_occurrence(occurrence_hash),
    cell_sequence      INTEGER NOT NULL UNIQUE CHECK (cell_sequence >= 1),
    epoch_hash         TEXT NOT NULL UNIQUE
                       CHECK (length(epoch_hash) = 71 AND epoch_hash GLOB 'sha256:[0-9a-f]*'),
    epoch_json         TEXT NOT NULL CHECK (json_valid(epoch_json)),
    member_count       INTEGER NOT NULL CHECK (member_count >= 0),
    next_member_ordinal INTEGER NOT NULL DEFAULT 0 CHECK (next_member_ordinal >= 0),
    completed          INTEGER NOT NULL DEFAULT 0 CHECK (completed IN (0, 1)),
    created_at         TEXT NOT NULL,
    updated_at         TEXT NOT NULL,
    completed_at       TEXT,
    CHECK (next_member_ordinal <= member_count),
    CHECK ((completed = 1) = (next_member_ordinal = member_count)),
    CHECK ((completed = 1) = (completed_at IS NOT NULL))
) STRICT;

CREATE TABLE karma_run (
    run_hash             TEXT PRIMARY KEY
                         CHECK (length(run_hash) = 71 AND run_hash GLOB 'sha256:[0-9a-f]*'),
    occurrence_hash      TEXT NOT NULL REFERENCES karma_occurrence(occurrence_hash),
    cell_sequence        INTEGER NOT NULL CHECK (cell_sequence >= 1),
    program_epoch_hash   TEXT NOT NULL REFERENCES karma_occurrence_program_epoch(epoch_hash),
    member_ordinal       INTEGER NOT NULL CHECK (member_ordinal >= 0),
    program_uid          TEXT NOT NULL REFERENCES karma_program(record_uid),
    program_revision_hash TEXT NOT NULL REFERENCES karma_program_revision(revision_hash),
    status               TEXT NOT NULL
                         CHECK (status IN ('succeeded', 'not-applicable', 'blocked',
                                           'evaluation-failed')),
    fuel_used            INTEGER NOT NULL CHECK (fuel_used >= 0),
    run_json             TEXT NOT NULL CHECK (json_valid(run_json)),
    created_at           TEXT NOT NULL,
    UNIQUE (occurrence_hash, program_revision_hash),
    UNIQUE (occurrence_hash, member_ordinal)
) STRICT;

CREATE TABLE karma_program_state_event (
    event_hash                 TEXT PRIMARY KEY
                               CHECK (length(event_hash) = 71 AND event_hash GLOB 'sha256:[0-9a-f]*'),
    program_uid                TEXT NOT NULL REFERENCES karma_program(record_uid),
    node_id                    TEXT NOT NULL,
    state_revision             INTEGER NOT NULL CHECK (state_revision >= 1),
    previous_event_hash        TEXT REFERENCES karma_program_state_event(event_hash),
    source_run_hash            TEXT NOT NULL REFERENCES karma_run(run_hash),
    definition_revision_hash   TEXT NOT NULL REFERENCES karma_program_revision(revision_hash),
    activation_handle_revision INTEGER NOT NULL CHECK (activation_handle_revision >= 1),
    reset_reason               TEXT
                               CHECK (reset_reason IS NULL OR reset_reason IN
                                      ('program-activation', 'revision-change', 'migration-reset')),
    state_kind                 TEXT NOT NULL CHECK (state_kind IN ('delay', 'control', 'reset')),
    state_json                 TEXT CHECK (state_json IS NULL OR json_valid(state_json)),
    event_json                 TEXT NOT NULL CHECK (json_valid(event_json)),
    created_at                 TEXT NOT NULL,
    UNIQUE (program_uid, node_id, state_revision),
    UNIQUE (source_run_hash, node_id),
    CHECK ((state_revision = 1) = (previous_event_hash IS NULL)),
    CHECK ((state_kind = 'reset') = (state_json IS NULL)),
    CHECK (state_kind != 'reset' OR reset_reason IS NOT NULL)
) STRICT;

CREATE TABLE karma_program_node_state (
    program_uid                TEXT NOT NULL REFERENCES karma_program(record_uid),
    node_id                    TEXT NOT NULL,
    state_revision             INTEGER NOT NULL CHECK (state_revision >= 1),
    current_event_hash         TEXT NOT NULL UNIQUE REFERENCES karma_program_state_event(event_hash),
    definition_revision_hash   TEXT NOT NULL REFERENCES karma_program_revision(revision_hash),
    activation_handle_revision INTEGER NOT NULL CHECK (activation_handle_revision >= 1),
    state_kind                 TEXT NOT NULL CHECK (state_kind IN ('delay', 'control', 'reset')),
    state_json                 TEXT CHECK (state_json IS NULL OR json_valid(state_json)),
    updated_at                 TEXT NOT NULL,
    PRIMARY KEY (program_uid, node_id),
    CHECK ((state_kind = 'reset') = (state_json IS NULL))
) STRICT;

CREATE TABLE karma_candidate (
    candidate_hash        TEXT PRIMARY KEY
                          CHECK (length(candidate_hash) = 71 AND candidate_hash GLOB 'sha256:[0-9a-f]*'),
    source_run_hash       TEXT NOT NULL REFERENCES karma_run(run_hash),
    occurrence_hash       TEXT NOT NULL REFERENCES karma_occurrence(occurrence_hash),
    program_uid           TEXT NOT NULL REFERENCES karma_program(record_uid),
    program_revision_hash TEXT NOT NULL REFERENCES karma_program_revision(revision_hash),
    node_id               TEXT NOT NULL,
    output_port           TEXT NOT NULL,
    route                 TEXT NOT NULL CHECK (route IN ('observe', 'recommend', 'draft', 'ask', 'act')),
    template              TEXT NOT NULL,
    status                TEXT NOT NULL CHECK (status = 'proposed'),
    proposal_json         TEXT NOT NULL CHECK (json_valid(proposal_json)),
    created_at            TEXT NOT NULL,
    UNIQUE (source_run_hash, node_id, output_port)
) STRICT;

CREATE TABLE karma_candidate_state (
    candidate_hash     TEXT PRIMARY KEY REFERENCES karma_candidate(candidate_hash),
    state_revision     INTEGER NOT NULL CHECK (state_revision >= 1),
    status             TEXT NOT NULL CHECK (status IN ('proposed', 'accepted', 'dismissed', 'snoozed')),
    snoozed_until      TEXT,
    current_event_hash TEXT,
    actor_person_uid   TEXT,
    updated_at         TEXT NOT NULL,
    CHECK ((status = 'snoozed') = (snoozed_until IS NOT NULL)),
    CHECK ((state_revision = 1) = (current_event_hash IS NULL))
) STRICT;

CREATE TABLE karma_candidate_review_event (
    event_hash          TEXT PRIMARY KEY
                        CHECK (length(event_hash) = 71 AND event_hash GLOB 'sha256:[0-9a-f]*'),
    candidate_hash      TEXT NOT NULL REFERENCES karma_candidate(candidate_hash),
    state_revision      INTEGER NOT NULL CHECK (state_revision >= 2),
    previous_event_hash TEXT REFERENCES karma_candidate_review_event(event_hash),
    request_id          TEXT NOT NULL UNIQUE REFERENCES karma_request(request_id),
    action              TEXT NOT NULL CHECK (action IN ('accept', 'dismiss', 'snooze')),
    status              TEXT NOT NULL CHECK (status IN ('accepted', 'dismissed', 'snoozed')),
    snoozed_until       TEXT,
    actor_person_uid    TEXT,
    evidence_json       TEXT NOT NULL CHECK (json_valid(evidence_json)),
    fact_uid            TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at          TEXT NOT NULL,
    UNIQUE (candidate_hash, state_revision),
    CHECK ((state_revision = 2) = (previous_event_hash IS NULL)),
    CHECK ((status = 'snoozed') = (snoozed_until IS NOT NULL))
) STRICT;

CREATE TABLE karma_candidate_review_request (
    request_id              TEXT PRIMARY KEY REFERENCES karma_request(request_id),
    payload_hash            TEXT NOT NULL,
    candidate_hash          TEXT NOT NULL REFERENCES karma_candidate(candidate_hash),
    expected_state_revision INTEGER NOT NULL,
    result_state_revision   INTEGER NOT NULL,
    result_json             TEXT NOT NULL CHECK (json_valid(result_json)),
    event_hash              TEXT NOT NULL REFERENCES karma_candidate_review_event(event_hash),
    fact_uid                TEXT NOT NULL REFERENCES fact(uid),
    created_at              TEXT NOT NULL
) STRICT;

CREATE TABLE karma_grant_revision (
    grant_uid           TEXT NOT NULL
                        REFERENCES karma_grant(record_uid)
                        DEFERRABLE INITIALLY DEFERRED,
    revision_hash       TEXT NOT NULL
                        CHECK (length(revision_hash) = 71 AND revision_hash GLOB 'sha256:[0-9a-f]*'),
    schema_name         TEXT NOT NULL CHECK (schema_name = 'karma.grant.v1'),
    revision_json       TEXT NOT NULL CHECK (json_valid(revision_json)),
    principal_person_uid TEXT NOT NULL,
    signer_key_id       TEXT NOT NULL CHECK (length(signer_key_id) BETWEEN 1 AND 200),
    revision_signature  TEXT NOT NULL CHECK (length(revision_signature) BETWEEN 1 AND 2048),
    created_at          TEXT NOT NULL,
    PRIMARY KEY (grant_uid, revision_hash)
) STRICT;

CREATE TABLE karma_grant (
    record_uid           TEXT PRIMARY KEY REFERENCES record(uid),
    handle_revision      INTEGER NOT NULL CHECK (handle_revision >= 1),
    status               TEXT NOT NULL CHECK (status IN ('draft', 'active', 'revoked')),
    head_revision_hash   TEXT NOT NULL,
    active_revision_hash TEXT,
    principal_person_uid TEXT NOT NULL,
    created_at           TEXT NOT NULL,
    updated_at           TEXT NOT NULL,
    CHECK ((status = 'active') = (active_revision_hash IS NOT NULL)),
    FOREIGN KEY (record_uid, head_revision_hash)
        REFERENCES karma_grant_revision(grant_uid, revision_hash)
        DEFERRABLE INITIALLY DEFERRED,
    FOREIGN KEY (record_uid, active_revision_hash)
        REFERENCES karma_grant_revision(grant_uid, revision_hash)
        DEFERRABLE INITIALLY DEFERRED
) STRICT;

CREATE TABLE karma_grant_request (
    request_id               TEXT PRIMARY KEY REFERENCES karma_request(request_id),
    action                   TEXT NOT NULL CHECK (action IN ('create', 'narrow', 'activate', 'revoke')),
    payload_hash             TEXT NOT NULL
                             CHECK (length(payload_hash) = 71 AND payload_hash GLOB 'sha256:[0-9a-f]*'),
    grant_uid                TEXT NOT NULL REFERENCES karma_grant(record_uid),
    expected_handle_revision INTEGER CHECK (expected_handle_revision IS NULL OR expected_handle_revision >= 1),
    result_handle_revision   INTEGER NOT NULL CHECK (result_handle_revision >= 1),
    result_json              TEXT NOT NULL CHECK (json_valid(result_json)),
    revision_hash            TEXT,
    fact_uid                 TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    created_at               TEXT NOT NULL,
    FOREIGN KEY (grant_uid, revision_hash)
        REFERENCES karma_grant_revision(grant_uid, revision_hash)
) STRICT;

CREATE TABLE karma_intent (
    intent_hash           TEXT PRIMARY KEY
                          CHECK (length(intent_hash) = 71 AND intent_hash GLOB 'sha256:[0-9a-f]*'),

    candidate_hash        TEXT NOT NULL UNIQUE REFERENCES karma_candidate(candidate_hash),
    grant_uid             TEXT NOT NULL REFERENCES karma_grant(record_uid),
    grant_revision_hash   TEXT NOT NULL,
    grant_handle_revision INTEGER NOT NULL CHECK (grant_handle_revision >= 1),
    program_uid           TEXT NOT NULL REFERENCES karma_program(record_uid),
    program_revision_hash TEXT NOT NULL REFERENCES karma_program_revision(revision_hash),
    template              TEXT NOT NULL,
    capability            TEXT NOT NULL,
    idempotency_key       TEXT NOT NULL UNIQUE
                          CHECK (length(idempotency_key) BETWEEN 1 AND 200),

    window_index          INTEGER CHECK (window_index IS NULL OR window_index >= 0),
    quantity_unit_uid     TEXT,
    quantity_scale        INTEGER CHECK (quantity_scale IS NULL OR quantity_scale BETWEEN 0 AND 9),
    quantity_mantissa     TEXT,
    deadline              TEXT NOT NULL,
    intent_json           TEXT NOT NULL CHECK (json_valid(intent_json)),
    created_at            TEXT NOT NULL,
    CHECK ((quantity_unit_uid IS NULL) = (quantity_mantissa IS NULL)
       AND (quantity_unit_uid IS NULL) = (quantity_scale IS NULL)),
    FOREIGN KEY (grant_uid, grant_revision_hash)
        REFERENCES karma_grant_revision(grant_uid, revision_hash)
) STRICT;

CREATE TABLE karma_intent_status (
    status            TEXT PRIMARY KEY,
    holds_reservation INTEGER NOT NULL CHECK (holds_reservation IN (0, 1))
) STRICT;

CREATE TABLE karma_intent_event (
    event_hash          TEXT PRIMARY KEY
                        CHECK (length(event_hash) = 71 AND event_hash GLOB 'sha256:[0-9a-f]*'),
    intent_hash         TEXT NOT NULL REFERENCES karma_intent(intent_hash),
    state_revision      INTEGER NOT NULL CHECK (state_revision >= 1),
    previous_event_hash TEXT REFERENCES karma_intent_event(event_hash),
    status              TEXT NOT NULL REFERENCES karma_intent_status(status),
    reason              TEXT,

    cause_request_id    TEXT NOT NULL REFERENCES karma_request(request_id),
    actor_person_uid    TEXT NOT NULL,
    event_json          TEXT NOT NULL CHECK (json_valid(event_json)),
    created_at          TEXT NOT NULL,
    UNIQUE (intent_hash, state_revision),
    CHECK ((state_revision = 1) = (previous_event_hash IS NULL)),
    CHECK (state_revision > 1 OR reason IS NULL)
) STRICT;

CREATE TABLE karma_intent_state (
    intent_hash        TEXT PRIMARY KEY REFERENCES karma_intent(intent_hash),
    state_revision     INTEGER NOT NULL CHECK (state_revision >= 1),
    status             TEXT NOT NULL REFERENCES karma_intent_status(status),
    current_event_hash TEXT NOT NULL UNIQUE REFERENCES karma_intent_event(event_hash),
    cancelled_reason   TEXT,
    actor_person_uid   TEXT NOT NULL,
    updated_at         TEXT NOT NULL,
    CHECK ((status = 'cancelled') = (cancelled_reason IS NOT NULL))
) STRICT;

CREATE TABLE fact_concept_event (
    uid         TEXT PRIMARY KEY,
    fact_uid    TEXT NOT NULL REFERENCES fact(uid),
    concept_uid TEXT REFERENCES concept(uid),
    actor_uid   TEXT,
    note        TEXT,
    at          TEXT NOT NULL
);

CREATE TABLE fact_concept (
    fact_uid    TEXT PRIMARY KEY REFERENCES fact(uid),
    concept_uid TEXT REFERENCES concept(uid),
    event_uid   TEXT NOT NULL REFERENCES fact_concept_event(uid),
    at          TEXT NOT NULL
);

CREATE TABLE entry (
    uid             TEXT PRIMARY KEY,

    record_uid      TEXT NOT NULL REFERENCES record(uid),

    amount_mantissa TEXT NOT NULL,
    amount_scale    INTEGER NOT NULL,
    note            TEXT,

    occurred_at     TEXT NOT NULL,

    state           TEXT NOT NULL,
    revision        INTEGER NOT NULL,

    fact_uid        TEXT REFERENCES fact(uid),
    actor_uid       TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE entry_revision (
    uid                  TEXT PRIMARY KEY,
    entry_uid            TEXT NOT NULL REFERENCES entry(uid),
    revision             INTEGER NOT NULL,

    kind                 TEXT NOT NULL,
    amount_mantissa      TEXT NOT NULL,
    amount_scale         INTEGER NOT NULL,
    note                 TEXT,
    occurred_at          TEXT NOT NULL,

    fact_uid             TEXT REFERENCES fact(uid),

    compensated_fact_uid TEXT REFERENCES fact(uid),
    request_id           TEXT NOT NULL,
    actor_uid            TEXT,
    at                   TEXT NOT NULL,
    UNIQUE(entry_uid, revision)
);

CREATE TABLE recurrence (
    uid             TEXT PRIMARY KEY,

    record_uid      TEXT NOT NULL REFERENCES record(uid),

    consequences_json TEXT NOT NULL CHECK (json_valid(consequences_json)),

    condition_src   TEXT,
    gate            TEXT,
    carry           TEXT,
    note            TEXT,

    cadence_json    TEXT NOT NULL CHECK (json_valid(cadence_json)),

    anchor_at       TEXT NOT NULL,

    state           TEXT NOT NULL,
    revision        INTEGER NOT NULL,
    actor_uid       TEXT,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
, slug TEXT, quantity INTEGER NOT NULL DEFAULT 1
    CHECK (quantity IN (0, 1)), name TEXT NOT NULL DEFAULT 'Karma rule', bindings_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(bindings_json)));

CREATE TABLE recurrence_revision (
    uid             TEXT PRIMARY KEY,
    recurrence_uid  TEXT NOT NULL REFERENCES recurrence(uid),
    revision        INTEGER NOT NULL,

    kind            TEXT NOT NULL,
    consequences_json TEXT NOT NULL CHECK (json_valid(consequences_json)),

    condition_src   TEXT,
    gate            TEXT,
    carry           TEXT,
    note            TEXT,
    cadence_json    TEXT NOT NULL CHECK (json_valid(cadence_json)),
    anchor_at       TEXT NOT NULL,
    state           TEXT NOT NULL,
    request_id      TEXT NOT NULL,
    actor_uid       TEXT,
    at              TEXT NOT NULL, slug TEXT, quantity INTEGER NOT NULL DEFAULT 1
    CHECK (quantity IN (0, 1)), name TEXT NOT NULL DEFAULT 'Karma rule', bindings_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(bindings_json)),
    UNIQUE(recurrence_uid, revision)
);

CREATE TABLE recurrence_skip (
    recurrence_uid TEXT NOT NULL REFERENCES recurrence(uid),
    due_at         TEXT NOT NULL,
    note           TEXT,
    actor_uid      TEXT,
    at             TEXT NOT NULL,
    PRIMARY KEY (recurrence_uid, due_at)
);

CREATE TABLE sync_op (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    tbl         TEXT NOT NULL,
    uid         TEXT NOT NULL,
    field       TEXT NOT NULL DEFAULT '',
    kind        TEXT NOT NULL CHECK (kind IN ('set', 'tombstone', 'fact', 'crdt', 'snapshot')),

    value       TEXT,
    hlc         INTEGER NOT NULL,
    actor_cell  TEXT NOT NULL,
    organ_uid   TEXT NOT NULL
, replica_root TEXT);

CREATE TABLE record_doc (
    record_uid  TEXT PRIMARY KEY REFERENCES record(uid),
    snapshot    BLOB NOT NULL,
    through_seq INTEGER NOT NULL DEFAULT 0,
    updated_at  TEXT NOT NULL
, generation INTEGER
CHECK(generation IS NULL OR (typeof(generation) = 'integer' AND generation > 0)), base_revision INTEGER
CHECK(base_revision IS NULL OR (typeof(base_revision) = 'integer' AND base_revision > 0)));

CREATE TABLE sync_outbox (
    contact_organ TEXT NOT NULL,
    tbl           TEXT NOT NULL,
    uid           TEXT NOT NULL,
    field         TEXT NOT NULL,

    kind          TEXT NOT NULL,
    seq           INTEGER NOT NULL,
    attempts      INTEGER NOT NULL DEFAULT 0,
    queued_at     TEXT NOT NULL,
    PRIMARY KEY (contact_organ, tbl, uid, field, kind)
);

CREATE TABLE replica_grant (
    root_record   TEXT NOT NULL,
    contact_organ TEXT NOT NULL,
    state         TEXT NOT NULL CHECK (state IN ('offered', 'accepted')),
    created_at    TEXT NOT NULL,
    PRIMARY KEY (root_record, contact_organ)
);

CREATE TABLE organ_roster (
    organ_uid  TEXT PRIMARY KEY,

    root_key   TEXT NOT NULL,

    version    INTEGER NOT NULL,

    not_after  TEXT NOT NULL,
    payload    TEXT NOT NULL,
    signature  TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE identity_succession (
    organ_uid  TEXT NOT NULL,
    old_key    TEXT NOT NULL,
    new_key    TEXT NOT NULL,
    signature  TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (organ_uid, old_key, new_key)
);

CREATE TABLE identity_revocation (
    organ_uid   TEXT NOT NULL,
    revoked_key TEXT NOT NULL,
    signature   TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    PRIMARY KEY (organ_uid, revoked_key)
);

CREATE TABLE enrolment_token (
    token_hash TEXT PRIMARY KEY,
    expires_at TEXT NOT NULL,
    used_at    TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE thread_invite (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),
    from_organ TEXT NOT NULL UNIQUE,

    root       TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE organ_login (
    organ_uid  TEXT PRIMARY KEY REFERENCES record(uid),

    person_uid TEXT NOT NULL UNIQUE REFERENCES record(uid),
    created_at TEXT NOT NULL
);

CREATE TABLE person_credential (

    person_uid    TEXT NOT NULL PRIMARY KEY REFERENCES record(uid) ON DELETE CASCADE,
    username      TEXT NOT NULL UNIQUE CHECK (length(trim(username)) > 0),
    password_hash TEXT NOT NULL CHECK (length(trim(password_hash)) > 0),
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;

CREATE TABLE organ_public_record (
    organ_uid  TEXT PRIMARY KEY,
    packet     BLOB NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE door_request (
    uid         TEXT PRIMARY KEY,

    node_id     TEXT NOT NULL,

    organ_uid   TEXT NOT NULL,

    intro       TEXT NOT NULL,
    received_at TEXT NOT NULL
);

CREATE TABLE local_capability (
    capability TEXT PRIMARY KEY
);

CREATE TABLE reference_read (

    reader_organ TEXT NOT NULL,
    record_uid   TEXT NOT NULL,

    root_record  TEXT NOT NULL,

    reads        INTEGER NOT NULL DEFAULT 1,
    last_read_at TEXT NOT NULL,
    PRIMARY KEY (reader_organ, record_uid, root_record)
);

CREATE TABLE karma_program_execution (
    program_uid TEXT PRIMARY KEY REFERENCES karma_program(record_uid),

    executes    INTEGER NOT NULL CHECK (executes IN (0, 1)),

    note        TEXT,
    updated_at  TEXT NOT NULL
) STRICT;

CREATE TABLE mailbox_registration (
    organ_uid TEXT PRIMARY KEY,

    root_key TEXT NOT NULL,
    label TEXT NOT NULL DEFAULT '',

    quota_bytes INTEGER NOT NULL,
    registered_at TEXT NOT NULL
);

CREATE TABLE mailbox_bundle (
    uid TEXT PRIMARY KEY,
    to_organ TEXT NOT NULL REFERENCES mailbox_registration(organ_uid) ON DELETE CASCADE,

    from_organ TEXT NOT NULL,
    from_cell TEXT NOT NULL,

    from_node TEXT NOT NULL DEFAULT '',

    body TEXT NOT NULL,
    bytes INTEGER NOT NULL,
    received_at TEXT NOT NULL,

    expires_at TEXT NOT NULL
);

CREATE TABLE mailbox_expiry_notice (
    uid TEXT PRIMARY KEY,
    to_organ TEXT NOT NULL,
    from_organ TEXT NOT NULL,
    from_cell TEXT NOT NULL,

    from_node TEXT NOT NULL DEFAULT '',
    bytes INTEGER NOT NULL,
    received_at TEXT NOT NULL,
    expired_at TEXT NOT NULL,

    notified_at TEXT
);

CREATE TABLE mailbox_request (
    organ_uid TEXT PRIMARY KEY,

    root_key  TEXT NOT NULL,

    label     TEXT NOT NULL DEFAULT '',
    asked_at  TEXT NOT NULL
);

CREATE TABLE mailbox_invite (
    token_hash   TEXT PRIMARY KEY,
    label        TEXT NOT NULL DEFAULT '',
    quota_bytes  INTEGER NOT NULL,
    expires_at   TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    used_at      TEXT,

    used_by      TEXT
);

CREATE TABLE contact_share (
    contact_organ TEXT NOT NULL,
    record_uid TEXT NOT NULL,
    picked INTEGER NOT NULL DEFAULT 0,
    held INTEGER NOT NULL DEFAULT 0,
    added_at TEXT NOT NULL,
    PRIMARY KEY (contact_organ, record_uid)
);

CREATE TABLE record_move (
    record_uid     TEXT PRIMARY KEY,
    contact_organ  TEXT NOT NULL,
    started_at     TEXT NOT NULL,
    handed_over_at TEXT
);

CREATE TABLE record_change (
    seq             INTEGER PRIMARY KEY AUTOINCREMENT,
    record_uid      TEXT NOT NULL,
    field           TEXT NOT NULL,
    cause           TEXT NOT NULL CHECK (cause IN ('local', 'remote')),
    winner_organ    TEXT,
    displaced       TEXT,
    displaced_local INTEGER NOT NULL DEFAULT 0,
    at              TEXT NOT NULL
);

CREATE TABLE contact_rate (
    from_organ    TEXT NOT NULL,
    kind          TEXT NOT NULL,
    window_start  TEXT NOT NULL,
    count         INTEGER NOT NULL DEFAULT 0,
    backoff_until TEXT,
    reason        TEXT,
    PRIMARY KEY (from_organ, kind)
);

CREATE TABLE offer_refusal (
    kind        TEXT NOT NULL,
    subject_uid TEXT NOT NULL,
    other_party TEXT NOT NULL,
    at          TEXT NOT NULL,
    until       TEXT NOT NULL,
    PRIMARY KEY (kind, subject_uid, other_party)
);

CREATE TABLE person_access (
    person_uid TEXT NOT NULL PRIMARY KEY REFERENCES record(uid) ON DELETE CASCADE,
    role_id INTEGER REFERENCES role(id) CHECK (role_id IS NULL OR role_id > 0),
    read_filter TEXT,
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0)
) STRICT;

CREATE TABLE role_policy (
    role_id INTEGER NOT NULL PRIMARY KEY REFERENCES role(id) ON DELETE CASCADE,
    policy TEXT CHECK (
        CASE
            WHEN policy IS NULL THEN 1
            WHEN json_valid(policy) THEN json_type(policy) = 'object'
            ELSE 0
        END
    ),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0)
) STRICT;

CREATE TABLE record_revision (
    record_uid TEXT NOT NULL PRIMARY KEY REFERENCES record(uid) ON DELETE CASCADE,
    revision INTEGER NOT NULL CHECK(revision > 0)
, document_generation INTEGER NOT NULL DEFAULT 1
CHECK(typeof(document_generation) = 'integer' AND document_generation > 0)) STRICT;

CREATE TABLE operation_receipt (
    organ_uid TEXT NOT NULL REFERENCES record(uid),
    person_uid TEXT NOT NULL REFERENCES record(uid),
    operation_uid TEXT NOT NULL,
    payload_digest BLOB NOT NULL CHECK(typeof(payload_digest) = 'blob' AND length(payload_digest) = 32),
    outcome TEXT NOT NULL CHECK(json_valid(outcome) AND json_type(outcome) = 'object'),
    accepted_at TEXT NOT NULL CHECK(length(accepted_at) > 0),
    PRIMARY KEY (organ_uid, person_uid, operation_uid)
) STRICT;

CREATE TABLE operation_receipt_record (
    organ_uid TEXT NOT NULL,
    person_uid TEXT NOT NULL,
    operation_uid TEXT NOT NULL,
    record_uid TEXT NOT NULL REFERENCES record(uid),
    PRIMARY KEY (organ_uid, person_uid, operation_uid, record_uid),
    FOREIGN KEY (organ_uid, person_uid, operation_uid)
        REFERENCES operation_receipt(organ_uid, person_uid, operation_uid)
) STRICT;

CREATE TABLE person_auth_generation (
    person_uid TEXT NOT NULL PRIMARY KEY,
    generation INTEGER NOT NULL CHECK(generation > 0)
) STRICT;

CREATE TABLE organ_login_generation (
    organ_uid TEXT NOT NULL PRIMARY KEY,
    generation INTEGER NOT NULL CHECK(generation > 0)
) STRICT;

CREATE TABLE person_device (
    person_uid TEXT NOT NULL,
    node_id TEXT NOT NULL,
    revoked INTEGER NOT NULL CHECK(revoked IN (0, 1)),
    revision INTEGER NOT NULL CHECK(revision > 0),
    PRIMARY KEY (person_uid, node_id)
) STRICT;

CREATE TABLE role_permission_revision (
    role_id INTEGER NOT NULL PRIMARY KEY CHECK (role_id > 0),
    revision INTEGER NOT NULL CHECK (revision > 0)
) STRICT;

CREATE TABLE interface_area_transition (
    request_id TEXT PRIMARY KEY NOT NULL,
    actor_uid TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE self_update (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    automatic INTEGER NOT NULL DEFAULT 0 CHECK (automatic IN (0, 1)),
    installed_revision TEXT,
    installed_at TEXT
);

CREATE TABLE sync_activity (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    at INTEGER NOT NULL,
    activity TEXT NOT NULL,
    summary TEXT NOT NULL
);

CREATE TABLE sync_history_policy (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    seconds INTEGER NOT NULL CHECK (seconds BETWEEN 60 AND 7776000),
    max_entries INTEGER NOT NULL CHECK (max_entries BETWEEN 1 AND 10000)
);

CREATE TABLE record_property (
    record_uid TEXT NOT NULL REFERENCES record(uid),
    property TEXT NOT NULL,
    clock INTEGER NOT NULL,
    peer TEXT NOT NULL,
    change_uid TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (record_uid, property)
);

CREATE TABLE record_change_receipt (
    actor TEXT NOT NULL,
    change_uid TEXT NOT NULL,
    record_uid TEXT NOT NULL,
    payload TEXT NOT NULL,
    result TEXT NOT NULL,
    PRIMARY KEY (actor, change_uid)
);

CREATE TABLE record_doc_delivery (
    record_uid TEXT NOT NULL,
    contact_organ TEXT NOT NULL,
    version TEXT NOT NULL,
    PRIMARY KEY (record_uid, contact_organ)
);

CREATE TABLE record_edit_draft (
    source TEXT NOT NULL,
    record_uid TEXT NOT NULL,
    state TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (source, record_uid)
);

CREATE TABLE karma_rule_frequency (
    recurrence_uid TEXT PRIMARY KEY REFERENCES recurrence(uid) ON DELETE CASCADE,
    frequency_uid TEXT NOT NULL REFERENCES karma_frequency(record_uid),
    rule_revision INTEGER NOT NULL
) STRICT;

CREATE TABLE karma_signal_frequency (
    signal_uid TEXT PRIMARY KEY REFERENCES signal(record_uid) ON DELETE CASCADE,
    frequency_uid TEXT NOT NULL REFERENCES karma_frequency(record_uid)
) STRICT;

CREATE TABLE karma_rule_progress (
    singleton INTEGER PRIMARY KEY CHECK(singleton = 1),
    next_sequence INTEGER NOT NULL
) STRICT;

CREATE TABLE karma_frequency_usage (
    frequency_uid TEXT PRIMARY KEY REFERENCES karma_frequency(record_uid),
    enabled INTEGER NOT NULL CHECK(enabled IN (0, 1)),
    auto_paused INTEGER NOT NULL DEFAULT 0 CHECK(auto_paused IN (0, 1))
) STRICT;

CREATE TABLE karma_field (
    uid TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK (kind IN ('condition', 'threshold', 'consequence')),
    source TEXT NOT NULL,
    revision INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE karma_field_binding (
    rule_uid TEXT NOT NULL REFERENCES recurrence(uid) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    field_uid TEXT NOT NULL REFERENCES karma_field(uid),
    PRIMARY KEY (rule_uid, kind)
);

CREATE TABLE karma_editor_request (
    request_id TEXT PRIMARY KEY,
    result_uid TEXT NOT NULL
);

CREATE TABLE conversation_group (
    root TEXT PRIMARY KEY,
    owner TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    accepted INTEGER NOT NULL DEFAULT 0 CHECK (accepted IN (0, 1)),
    payload TEXT NOT NULL
);

CREATE TABLE conversation_delivery (
    root TEXT NOT NULL REFERENCES conversation_group(root) ON DELETE CASCADE,
    organ TEXT NOT NULL,
    revision INTEGER NOT NULL,
    PRIMARY KEY (root, organ)
);

CREATE TABLE projection_source (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    revision INTEGER NOT NULL CHECK (revision >= 0),
    runtime TEXT
);

CREATE TABLE projection_window (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    cache_key TEXT NOT NULL,
    source_revision INTEGER NOT NULL,
    base_ms INTEGER NOT NULL,
    expires_ms INTEGER NOT NULL,
    from_ms INTEGER NOT NULL,
    until_ms INTEGER NOT NULL,
    incomplete TEXT CHECK (incomplete IS NULL OR json_valid(incomplete))
);

CREATE TABLE projection_span (
    id TEXT PRIMARY KEY,
    record_uid TEXT NOT NULL,
    from_ms INTEGER NOT NULL,
    until_ms INTEGER NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload))
);

CREATE TABLE blob_sync (
    id TEXT PRIMARY KEY,
    owner TEXT NOT NULL,
    direction TEXT NOT NULL CHECK (direction IN ('incoming', 'outgoing')),
    peer TEXT NOT NULL,
    peer_organ TEXT,
    label TEXT NOT NULL,
    manifest TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('offered', 'accepted', 'completed', 'declined', 'cancelled')),
    destination TEXT,
    progress INTEGER NOT NULL DEFAULT 0 CHECK (progress >= 0),
    error TEXT NOT NULL DEFAULT '',
    settled INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE TABLE peer_delivery (
    organ_uid TEXT NOT NULL,
    cell_uid TEXT NOT NULL,
    node_id TEXT NOT NULL,
    attempted_at TEXT NOT NULL,
    succeeded_at TEXT,
    error TEXT,
    covered_seq INTEGER NOT NULL DEFAULT 0,
    addresses TEXT NOT NULL DEFAULT '[]',
    PRIMARY KEY (organ_uid, cell_uid, node_id)
);

CREATE TABLE commit_sequence (
    id INTEGER PRIMARY KEY CHECK(id = 1),
    value INTEGER NOT NULL CHECK(value >= 0)
) STRICT;

CREATE TABLE simulation_check_set (
    uid TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision > 0),
    checks_json TEXT NOT NULL CHECK(json_valid(checks_json))
) STRICT;

CREATE TABLE transfer_local_application (
    uid                         TEXT PRIMARY KEY,
    handoff_uid                 TEXT NOT NULL UNIQUE,
    participant_person_uid      TEXT NOT NULL,
    local_record_uid            TEXT NOT NULL REFERENCES record(uid),
    application_fact_uid        TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    local_delta                 REAL NOT NULL,
    local_cumulative_before     REAL NOT NULL,
    local_cumulative_after      REAL NOT NULL,
    application_formula         TEXT NOT NULL,
    application_formula_hash    TEXT NOT NULL,
    application_formula_version INTEGER NOT NULL CHECK (application_formula_version >= 0),
    authorization_intent_uid    TEXT REFERENCES signed_action_intent(uid),
    request_id                  TEXT NOT NULL UNIQUE,
    created_at                  TEXT NOT NULL
) STRICT;

CREATE TABLE "transfer_open_claim_pair" (
    uid                    TEXT PRIMARY KEY,
    transfer_uid           TEXT NOT NULL REFERENCES transfer(record_uid),
    source_promise_uid     TEXT NOT NULL REFERENCES promise(uid),
    source_record_uid      TEXT REFERENCES record(uid),
    proposer_promise_uid   TEXT NOT NULL UNIQUE REFERENCES promise(uid),
    claimant_promise_uid   TEXT NOT NULL UNIQUE REFERENCES promise(uid),
    proposer_person_uid    TEXT NOT NULL REFERENCES record(uid),
    claimant_person_uid    TEXT NOT NULL REFERENCES record(uid),
    revision               INTEGER NOT NULL CHECK (revision > 0),
    reuse_policy           TEXT NOT NULL CHECK (reuse_policy IN ('duplicate', 'consume')),
    idempotency_key        TEXT NOT NULL UNIQUE,
    created_at             TEXT NOT NULL,
    CHECK (proposer_person_uid != claimant_person_uid),
    CHECK (proposer_promise_uid != claimant_promise_uid),
    FOREIGN KEY (transfer_uid, revision)
        REFERENCES transfer_revision(transfer_uid, revision),
    FOREIGN KEY (idempotency_key)
        REFERENCES transfer_revision(idempotency_key)
) STRICT;

CREATE TABLE transfer_private_policy_event (
    uid TEXT PRIMARY KEY,
    transfer_uid TEXT NOT NULL,
    exchange_uid TEXT NOT NULL,
    person_uid TEXT NOT NULL REFERENCES record(uid),
    record_uid TEXT NOT NULL REFERENCES record(uid),
    version INTEGER NOT NULL CHECK (version > 0),
    formula TEXT NOT NULL,
    policy_hash TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    signature TEXT NOT NULL,
    key_id TEXT NOT NULL,
    public_key TEXT NOT NULL,
    request_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL, effects_json TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(effects_json)),
    UNIQUE (transfer_uid, exchange_uid, person_uid, version)
) STRICT;

CREATE TABLE transfer_private_policy (
    transfer_uid TEXT NOT NULL,
    exchange_uid TEXT NOT NULL,
    person_uid TEXT NOT NULL REFERENCES record(uid),
    event_uid TEXT NOT NULL UNIQUE REFERENCES transfer_private_policy_event(uid),
    PRIMARY KEY (transfer_uid, exchange_uid, person_uid)
) STRICT;

CREATE TABLE transfer_private_application_correction (
    uid TEXT PRIMARY KEY,
    application_uid TEXT NOT NULL UNIQUE REFERENCES transfer_local_application(uid),
    person_uid TEXT NOT NULL REFERENCES record(uid),
    fact_uid TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    request_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE transfer_stock_limit (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),
    writer_cell_uid TEXT NOT NULL REFERENCES record(uid),
    person_uid TEXT NOT NULL REFERENCES record(uid),
    minimum_mantissa TEXT NOT NULL,
    minimum_scale INTEGER NOT NULL,
    version INTEGER NOT NULL CHECK (version > 0),
    event_uid TEXT NOT NULL UNIQUE
) STRICT;

CREATE TABLE transfer_stock_limit_event (
    uid TEXT PRIMARY KEY,
    record_uid TEXT NOT NULL REFERENCES record(uid),
    person_uid TEXT NOT NULL REFERENCES record(uid),
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    signature TEXT NOT NULL,
    key_id TEXT NOT NULL,
    public_key TEXT NOT NULL,
    request_id TEXT NOT NULL UNIQUE,
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE transfer_stock_roster_history (
    organ_uid TEXT NOT NULL,
    version INTEGER NOT NULL,
    payload TEXT NOT NULL,
    not_after TEXT NOT NULL,
    PRIMARY KEY (organ_uid, version)
) STRICT;

CREATE TABLE transfer_cancellation (
    uid TEXT PRIMARY KEY,
    transfer_uid TEXT NOT NULL REFERENCES transfer(record_uid),
    revision INTEGER NOT NULL CHECK (revision > 0),
    exchange_path_uid TEXT NOT NULL REFERENCES transfer_exchange_path(uid),
    quantity_mantissa TEXT NOT NULL,
    quantity_scale INTEGER NOT NULL,
    occurrences TEXT NOT NULL CHECK (json_valid(occurrences)),
    required_people TEXT NOT NULL CHECK (json_valid(required_people)),
    proposer_uid TEXT NOT NULL REFERENCES record(uid),
    proposal_fact_uid TEXT REFERENCES fact(uid),
    request_id TEXT NOT NULL UNIQUE,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL,
    applied_fact_uid TEXT UNIQUE REFERENCES fact(uid)
) STRICT;

CREATE TABLE transfer_cancellation_application (
    request_id TEXT PRIMARY KEY,
    cancellation_uid TEXT NOT NULL UNIQUE REFERENCES transfer_cancellation(uid),
    person_uid TEXT NOT NULL REFERENCES record(uid),
    fact_uid TEXT NOT NULL UNIQUE REFERENCES fact(uid),
    payload TEXT NOT NULL
) STRICT;

CREATE TABLE transfer_occurrence_cancellation (
    occurrence_uid TEXT PRIMARY KEY REFERENCES transfer_occurrence(uid),
    cancellation_uid TEXT NOT NULL REFERENCES transfer_cancellation(uid),
    quantity_mantissa TEXT NOT NULL,
    quantity_scale INTEGER NOT NULL,
    fact_uid TEXT NOT NULL REFERENCES fact(uid)
) STRICT;

CREATE TABLE transfer_child_requirement (
    parent_uid TEXT NOT NULL REFERENCES transfer(record_uid),
    child_uid TEXT NOT NULL REFERENCES transfer(record_uid),
    required INTEGER NOT NULL CHECK (required IN (0, 1)),
    PRIMARY KEY (parent_uid, child_uid)
);

CREATE TABLE transfer_child_request (
    request_id TEXT PRIMARY KEY,
    payload TEXT NOT NULL,
    fact_uid TEXT NOT NULL REFERENCES fact(uid)
);

CREATE TABLE transfer_outcome_evidence (
    envelope_uid TEXT PRIMARY KEY,
    reference_uid TEXT NOT NULL REFERENCES transfer_remote_reference(uid),
    cursor INTEGER NOT NULL,
    revision INTEGER NOT NULL,
    policy_revision INTEGER NOT NULL,
    origin_created_at TEXT NOT NULL,
    received_at TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    payload_hash TEXT NOT NULL,
    UNIQUE (reference_uid, cursor)
);

CREATE TABLE transfer_private_effect (
    fact_uid TEXT PRIMARY KEY REFERENCES fact(uid),
    primary_fact_uid TEXT NOT NULL REFERENCES fact(uid),
    occurrence_uid TEXT NOT NULL,
    transfer_uid TEXT NOT NULL,
    exchange_uid TEXT NOT NULL,
    person_uid TEXT NOT NULL REFERENCES record(uid),
    record_uid TEXT NOT NULL REFERENCES record(uid),
    mode TEXT NOT NULL CHECK (mode IN ('quantity','fulfilment')),
    unit_uid TEXT REFERENCES concept(uid),
    formula TEXT NOT NULL,
    review_hash TEXT NOT NULL,
    UNIQUE (primary_fact_uid, record_uid)
) STRICT;

CREATE TABLE transfer_loan_agreement (
    transfer_uid TEXT NOT NULL REFERENCES record(uid),
    exchange_uid TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    from_ms INTEGER NOT NULL,
    until_ms INTEGER NOT NULL CHECK (until_ms > from_ms),
    agreement_fact_uid TEXT NOT NULL REFERENCES fact(uid),
    created_at TEXT NOT NULL,
    PRIMARY KEY (transfer_uid, exchange_uid, revision)
) STRICT;

CREATE TABLE transfer_loan_extension (
    request_id TEXT PRIMARY KEY,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    revision INTEGER NOT NULL CHECK (revision > 0),
    fact_uid TEXT NOT NULL REFERENCES fact(uid)
) STRICT;

CREATE TABLE fact_origin (
    fact_uid TEXT PRIMARY KEY REFERENCES fact(uid) ON DELETE CASCADE,
    organ_uid TEXT NOT NULL,
    cell_uid TEXT NOT NULL,
    payload TEXT NOT NULL CHECK (json_valid(payload) AND json_extract(payload, '$.uid') = fact_uid)
) STRICT;

CREATE TABLE transfer_sync_control (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    importing INTEGER NOT NULL DEFAULT 0 CHECK (importing IN (0, 1))
) STRICT;

CREATE TABLE transfer_sync_journal (
    seq INTEGER PRIMARY KEY AUTOINCREMENT,
    organ_uid TEXT NOT NULL,
    cell_uid TEXT NOT NULL,
    commit_sequence INTEGER NOT NULL,
    table_name TEXT NOT NULL,
    before_json TEXT CHECK (before_json IS NULL OR json_valid(before_json)),
    after_json TEXT CHECK (after_json IS NULL OR json_valid(after_json))
) STRICT;

CREATE TABLE transfer_sync_message (
    organ_uid TEXT NOT NULL,
    cell_uid TEXT NOT NULL,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    journal_end INTEGER,
    payload TEXT NOT NULL CHECK (json_valid(payload)),
    PRIMARY KEY (organ_uid, cell_uid, sequence)
) STRICT;

CREATE TABLE transfer_sync_owner (
    table_name TEXT NOT NULL,
    row_key TEXT NOT NULL,
    cell_uid TEXT NOT NULL,
    PRIMARY KEY (table_name, row_key)
) STRICT;

CREATE TABLE karma_rule_application (
    event_id TEXT NOT NULL,
    rule_uid TEXT NOT NULL,
    rule_revision INTEGER NOT NULL,
    status TEXT NOT NULL,
    reason TEXT,
    at TEXT NOT NULL,
    intended_at TEXT NOT NULL,
    frequency_uid TEXT,
    attempt INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(event_id, rule_uid, rule_revision, attempt)
) STRICT;

CREATE TABLE karma_schedule (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    revision INTEGER NOT NULL,
    cancelled INTEGER NOT NULL DEFAULT 0 CHECK(cancelled IN (0, 1)),
    actor_uid TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
) STRICT;

CREATE TABLE karma_schedule_revision (
    schedule_uid TEXT NOT NULL REFERENCES karma_schedule(uid),
    revision INTEGER NOT NULL,
    inputs TEXT NOT NULL CHECK(json_valid(inputs)),
    at TEXT NOT NULL,
    PRIMARY KEY(schedule_uid, revision)
) STRICT;

CREATE TABLE karma_schedule_boundary (
    uid TEXT PRIMARY KEY,
    schedule_uid TEXT NOT NULL REFERENCES karma_schedule(uid),
    revision INTEGER NOT NULL,
    purpose TEXT NOT NULL CHECK(purpose IN ('once', 'start', 'end')),
    input TEXT NOT NULL CHECK(json_valid(input)),
    intended_at_ms INTEGER NOT NULL,
    frequency_uid TEXT NOT NULL REFERENCES karma_frequency(record_uid),
    rule_uid TEXT NOT NULL REFERENCES recurrence(uid),
    current INTEGER NOT NULL CHECK(current IN (0, 1)),
    status TEXT NOT NULL,
    event_id TEXT,
    rule_revision INTEGER,
    attempt INTEGER NOT NULL DEFAULT 0,
    reason TEXT,
    completed_at TEXT
) STRICT;

CREATE TABLE karma_schedule_request (
    request_id TEXT PRIMARY KEY,
    fingerprint TEXT NOT NULL,
    schedule_uid TEXT NOT NULL REFERENCES karma_schedule(uid),
    result TEXT NOT NULL CHECK(json_valid(result))
) STRICT;

CREATE TABLE karma_effect_outcome (
    effect_uid TEXT NOT NULL,
    attempt INTEGER NOT NULL,
    status TEXT NOT NULL,
    result TEXT,
    at TEXT NOT NULL,
    PRIMARY KEY(effect_uid, attempt)
) STRICT;

CREATE TABLE "karma_schedule_cursor" (
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

CREATE TABLE karma_rule_evidence (
    event_id TEXT NOT NULL,
    rule_uid TEXT NOT NULL,
    rule_revision INTEGER NOT NULL,
    attempt INTEGER NOT NULL,
    evidence TEXT NOT NULL CHECK(json_valid(evidence)),
    PRIMARY KEY(event_id, rule_uid, rule_revision, attempt)
) STRICT;

CREATE TABLE karma_habit_import (
    organ_uid TEXT NOT NULL REFERENCES record(uid),
    tutorial TEXT NOT NULL,
    definition TEXT NOT NULL CHECK(json_valid(definition)),
    completed INTEGER NOT NULL DEFAULT 0 CHECK(completed IN (0, 1)),
    PRIMARY KEY(organ_uid, tutorial)
) STRICT;

CREATE TABLE karma_habit_object (
    organ_uid TEXT NOT NULL,
    tutorial TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('record', 'frequency', 'rule')),
    uid TEXT NOT NULL UNIQUE,
    created INTEGER NOT NULL DEFAULT 0 CHECK(created IN (0, 1)),
    PRIMARY KEY(organ_uid, tutorial, kind),
    FOREIGN KEY(organ_uid, tutorial) REFERENCES karma_habit_import(organ_uid, tutorial) ON DELETE CASCADE
) STRICT;

CREATE TABLE karma_habit_request (
    organ_uid TEXT NOT NULL,
    tutorial TEXT NOT NULL,
    request_id TEXT NOT NULL,
    fingerprint TEXT NOT NULL,
    result TEXT CHECK(result IS NULL OR json_valid(result)),
    PRIMARY KEY(organ_uid, request_id),
    FOREIGN KEY(organ_uid, tutorial) REFERENCES karma_habit_import(organ_uid, tutorial) ON DELETE CASCADE
) STRICT;

CREATE TABLE transfer_agreement_target_request (
    idempotency_key TEXT PRIMARY KEY CHECK (length(trim(idempotency_key)) > 0),
    transfer_uid TEXT NOT NULL REFERENCES transfer(record_uid),
    person_uid TEXT NOT NULL REFERENCES record(uid),
    revision INTEGER NOT NULL CHECK (revision > 0),
    target_level INTEGER NOT NULL CHECK (target_level BETWEEN 0 AND 2),
    fingerprint TEXT NOT NULL,
    result TEXT NOT NULL CHECK (json_valid(result)),
    created_at TEXT NOT NULL
) STRICT;

CREATE TABLE karma_transfer_stage (
    boundary_uid TEXT PRIMARY KEY REFERENCES karma_schedule_boundary(uid),
    parent_rule_uid TEXT NOT NULL,
    parent_revision INTEGER NOT NULL,
    position INTEGER NOT NULL,
    change_uid TEXT NOT NULL,
    origin TEXT NOT NULL CHECK(json_valid(origin)),
    UNIQUE(parent_rule_uid, parent_revision, position, change_uid)
) STRICT;

CREATE TABLE karma_transfer_command (
    command_uid TEXT PRIMARY KEY REFERENCES transfer_remote_command(command_uid),
    rule_uid TEXT NOT NULL,
    rule_revision INTEGER NOT NULL,
    origin TEXT NOT NULL CHECK(json_valid(origin)),
    cancelled INTEGER NOT NULL DEFAULT 0 CHECK(cancelled IN (0, 1)),
    reason TEXT,
    dispatched_at TEXT
) STRICT;

CREATE TABLE mailbox_roster_floor (
    organ_uid TEXT PRIMARY KEY REFERENCES mailbox_registration(organ_uid) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    payload TEXT NOT NULL
);

CREATE TABLE mailbox_device_ack (
    uid TEXT NOT NULL REFERENCES mailbox_bundle(uid) ON DELETE CASCADE,
    node_id TEXT NOT NULL,
    PRIMARY KEY (uid, node_id)
);

CREATE TABLE mailbox_inbox (
    uid TEXT PRIMARY KEY,
    carrier TEXT NOT NULL,
    body TEXT NOT NULL,
    body_hash TEXT NOT NULL,
    received_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending',
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    error TEXT
);

CREATE TABLE mailbox_outbox (
    intent TEXT PRIMARY KEY,
    uid TEXT NOT NULL UNIQUE,
    to_organ TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    requested_copies INTEGER NOT NULL CHECK(requested_copies BETWEEN 1 AND 2),
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    error TEXT
);

CREATE TABLE mailbox_outbox_receipt (
    uid TEXT NOT NULL REFERENCES mailbox_outbox(uid) ON DELETE CASCADE,
    carrier_node TEXT NOT NULL,
    carrier_organ TEXT NOT NULL,
    accepted_at TEXT NOT NULL,
    PRIMARY KEY(uid, carrier_node)
);

CREATE TABLE mail_left (
    uid TEXT NOT NULL,
    carrier_organ TEXT NOT NULL,
    carrier_node TEXT NOT NULL,
    to_organ TEXT NOT NULL,
    left_at TEXT NOT NULL,
    expired_at TEXT,
    PRIMARY KEY(uid, carrier_node)
);

CREATE TABLE mailbox_completion (
    uid TEXT PRIMARY KEY,
    to_organ TEXT NOT NULL REFERENCES mailbox_registration(organ_uid) ON DELETE CASCADE,
    body_hash TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    completed_at TEXT NOT NULL
);

CREATE TABLE social_document (
    kind TEXT NOT NULL CHECK(kind IN ('snippet','profile')),
    id TEXT NOT NULL,
    authority TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision > 0),
    hash TEXT NOT NULL,
    body TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    state TEXT NOT NULL,
    title TEXT NOT NULL,
    text TEXT NOT NULL,
    direction TEXT NOT NULL,
    language TEXT NOT NULL,
    area TEXT NOT NULL,
    concept TEXT NOT NULL,
    unit TEXT NOT NULL,
    source TEXT NOT NULL, generation INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY(kind,id)
);

CREATE TABLE social_revision (
    kind TEXT NOT NULL,
    id TEXT NOT NULL,
    hash TEXT NOT NULL,
    body TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    PRIMARY KEY(kind,id,hash)
);

CREATE TABLE social_publication_job (
    hash TEXT NOT NULL,
    destination TEXT NOT NULL,
    body TEXT NOT NULL,
    kind TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','accepted','failed','expired','cancelled')),
    error TEXT, receipt TEXT CHECK(receipt IS NULL OR length(CAST(receipt AS BLOB))<=512),
    PRIMARY KEY(hash,destination)
);

CREATE TABLE social_service_budget (
    source TEXT NOT NULL,
    direction TEXT NOT NULL,
    window INTEGER NOT NULL,
    bytes INTEGER NOT NULL,
    work INTEGER NOT NULL,
    PRIMARY KEY(source,direction)
);

CREATE TABLE karma_command_invocation (
    uid TEXT PRIMARY KEY,
    command_uid TEXT,
    command_revision INTEGER,
    configuration TEXT NOT NULL,
    host_uid TEXT,
    actor_uid TEXT,
    numeric INTEGER NOT NULL CHECK (numeric IN (0, 1)),
    context TEXT,
    status TEXT NOT NULL CHECK (status IN ('queued', 'running', 'completed', 'failed', 'indeterminate')),
    stdout TEXT,
    stderr TEXT,
    value TEXT,
    error TEXT,
    created_at TEXT NOT NULL,
    finished_at TEXT
);

CREATE TABLE karma_signal_sample (
    signal_uid TEXT PRIMARY KEY REFERENCES record(uid),
    invocation_uid TEXT NOT NULL REFERENCES karma_command_invocation(uid),
    value TEXT NOT NULL,
    sampled_at TEXT NOT NULL
);

CREATE TABLE fiote_activation (
    request_id TEXT PRIMARY KEY,
    fiote_uid TEXT NOT NULL REFERENCES record(uid),
    actor_uid TEXT,
    value TEXT NOT NULL,
    cause_json TEXT NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('queued', 'waiting', 'running', 'finished', 'interrupted', 'cancelled', 'refused')),
    thread_uid TEXT REFERENCES record(uid),
    detail TEXT NOT NULL DEFAULT '',
    created_at TEXT NOT NULL
);

CREATE TABLE transfer_draft_discard (
    request_id TEXT PRIMARY KEY,
    transfer_uid TEXT NOT NULL UNIQUE REFERENCES transfer(record_uid),
    person_uid TEXT NOT NULL REFERENCES record(uid),
    expected_revision INTEGER NOT NULL,
    fact_uid TEXT NOT NULL REFERENCES fact(uid)
);

CREATE TABLE social_public_asset (
    hash TEXT PRIMARY KEY CHECK(length(hash)=64),
    bytes BLOB NOT NULL CHECK(length(bytes)<=131072),
    width INTEGER NOT NULL CHECK(width BETWEEN 1 AND 2048),
    height INTEGER NOT NULL CHECK(height BETWEEN 1 AND 2048),
    touched_at INTEGER NOT NULL
);

CREATE TABLE social_ended_post (
    id TEXT PRIMARY KEY,
    authority TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision > 0),
    hash TEXT NOT NULL
, generation INTEGER NOT NULL DEFAULT 1);

CREATE TABLE social_device_state (
    id TEXT PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('account','session','authority')),
    context TEXT NOT NULL,
    body TEXT NOT NULL CHECK(length(CAST(body AS BLOB))<=716800),
    version INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL
);

CREATE TABLE social_private_seen (
    envelope TEXT PRIMARY KEY,
    hash TEXT NOT NULL,
    cipher_hash TEXT NOT NULL,
    message TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    receipt TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE TABLE social_message_identity (
    message TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL,
    record_uid TEXT NOT NULL,
    conversation TEXT NOT NULL
);

CREATE TABLE social_owner_control (
    owner TEXT PRIMARY KEY,
    generation INTEGER NOT NULL,
    body TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE TABLE social_reply_route (
    id TEXT PRIMARY KEY,
    owner TEXT NOT NULL,
    signing_key TEXT NOT NULL,
    pickup_key TEXT NOT NULL,
    body TEXT NOT NULL,
    post TEXT,
    state TEXT NOT NULL DEFAULT 'open' CHECK(state IN ('open','closed')),
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE TABLE social_sender_admission (
    route TEXT NOT NULL REFERENCES social_reply_route(id),
    sender TEXT NOT NULL,
    state TEXT NOT NULL,
    body TEXT NOT NULL,
    issued_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    PRIMARY KEY(route,sender)
);

CREATE TABLE social_service_completed (
    id TEXT PRIMARY KEY,
    route TEXT NOT NULL,
    sender TEXT NOT NULL,
    hash TEXT NOT NULL,
    expires_at INTEGER NOT NULL
, receipt TEXT, stage TEXT NOT NULL DEFAULT 'recipient-durable' CHECK(stage IN ('recipient-durable','recipient-refused')));

CREATE TABLE social_mailbox_pin (
    mailbox TEXT PRIMARY KEY,
    owner TEXT NOT NULL,
    signing_key TEXT NOT NULL,
    identity_key TEXT NOT NULL,
    pickup_key TEXT NOT NULL
, pickup_sequence INTEGER NOT NULL DEFAULT 0);

CREATE TABLE social_sender_counter (
    route TEXT NOT NULL,
    sender TEXT NOT NULL,
    introductions INTEGER NOT NULL DEFAULT 0,
    provisional INTEGER NOT NULL DEFAULT 0,
    expires_at INTEGER NOT NULL,
    PRIMARY KEY(route,sender)
);

CREATE TABLE social_posting_authority (
    owner TEXT PRIMARY KEY,
    editor TEXT NOT NULL,
    generation INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    body TEXT NOT NULL
);

CREATE TABLE social_message_work (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),
    conversation TEXT NOT NULL REFERENCES record(uid),
    context TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    error TEXT
);

CREATE TABLE social_session_peer (
    context TEXT NOT NULL,
    peer TEXT NOT NULL,
    session_id TEXT NOT NULL REFERENCES social_device_state(id) ON DELETE CASCADE,
    PRIMARY KEY(context,peer)
);

CREATE TABLE social_pickup_work (
    context TEXT NOT NULL,
    service TEXT NOT NULL,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    attempts INTEGER NOT NULL DEFAULT 0,
    error TEXT,
    PRIMARY KEY(context,service)
);

CREATE TABLE social_peer_work (
    conversation TEXT PRIMARY KEY REFERENCES record(uid),
    next_attempt INTEGER NOT NULL DEFAULT 0,
    attempts INTEGER NOT NULL DEFAULT 0,
    error TEXT
);

CREATE TABLE social_message_event (
    event TEXT PRIMARY KEY,
    record_uid TEXT NOT NULL REFERENCES record(uid),
    fact TEXT NOT NULL REFERENCES fact(uid),
    issued_at INTEGER NOT NULL,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    attempts INTEGER NOT NULL DEFAULT 0,
    error TEXT
);

CREATE TABLE social_receive_failure (
    context TEXT NOT NULL REFERENCES record(uid),
    service TEXT NOT NULL,
    envelope TEXT NOT NULL,
    reference TEXT NOT NULL CHECK(length(CAST(reference AS BLOB))<=2048),
    error TEXT NOT NULL CHECK(length(CAST(error AS BLOB))<=2048),
    expires_at INTEGER NOT NULL,
    discard INTEGER NOT NULL DEFAULT 0 CHECK(discard IN (0,1)),
    next_attempt INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(context,service,envelope)
);

CREATE TABLE social_gossip_item (
    hash TEXT PRIMARY KEY,
    post TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('snippet','profile-authority','posting-authority')),
    document_hash TEXT NOT NULL,
    body TEXT NOT NULL CHECK(length(CAST(body AS BLOB)) <= 16384),
    expires_at INTEGER NOT NULL,
    control INTEGER NOT NULL CHECK(control IN (0,1)),
    assigned INTEGER NOT NULL DEFAULT 0 CHECK(assigned IN (0,1))
);

CREATE TABLE social_gossip_forward (
    hash TEXT NOT NULL REFERENCES social_gossip_item(hash) ON DELETE CASCADE,
    peer TEXT NOT NULL,
    contact TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','accepted','cancelled')),
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    error TEXT,
    PRIMARY KEY(hash,peer)
);

CREATE TABLE social_gossip_seen (
    hash TEXT PRIMARY KEY,
    expires_at INTEGER NOT NULL,
    control INTEGER NOT NULL CHECK(control IN (0,1))
);

CREATE TABLE social_gossip_scan (
    id INTEGER PRIMARY KEY CHECK(id=1),
    after TEXT NOT NULL,
    error TEXT
);

CREATE TABLE social_ask_query (
    id TEXT PRIMARY KEY,
    actor TEXT,
    request TEXT NOT NULL CHECK(length(CAST(request AS BLOB)) <= 8192),
    peers TEXT NOT NULL CHECK(length(CAST(peers AS BLOB)) <= 2048),
    deadline INTEGER NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','completed','cancelled','expired')),
    error TEXT,
    results TEXT NOT NULL DEFAULT '[]' CHECK(length(CAST(results AS BLOB)) <= 262144)
);

CREATE TABLE social_ask_seen (
    id TEXT PRIMARY KEY,
    binding TEXT NOT NULL,
    source TEXT NOT NULL,
    deadline INTEGER NOT NULL,
    reserved INTEGER NOT NULL CHECK(reserved BETWEEN 1024 AND 197632),
    reply TEXT CHECK(length(CAST(reply AS BLOB)) <= 262144)
);

CREATE TABLE social_context_retention (
    context TEXT PRIMARY KEY REFERENCES record(uid),
    state TEXT NOT NULL CHECK(state IN ('active','dormant','retired','review')),
    retire_after INTEGER NOT NULL DEFAULT 0,
    checked_at INTEGER NOT NULL,
    queue_checked_at INTEGER NOT NULL DEFAULT 0,
    error TEXT CHECK(error IS NULL OR length(CAST(error AS BLOB))<=2048)
, metadata_after TEXT NOT NULL DEFAULT '');

CREATE TABLE social_listing_removal (
    post TEXT PRIMARY KEY,
    reason TEXT NOT NULL CHECK(length(CAST(reason AS BLOB))<=2048),
    removed_at INTEGER NOT NULL
);

CREATE TABLE social_report_work (
    id TEXT PRIMARY KEY,
    actor TEXT NOT NULL,
    service TEXT NOT NULL,
    body TEXT NOT NULL CHECK(length(CAST(body AS BLOB))<=16384),
    hash TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','accepted','refused','expired')),
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    error TEXT
);

CREATE TABLE social_received_report (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    hash TEXT NOT NULL,
    body TEXT NOT NULL CHECK(length(CAST(body AS BLOB))<=16384),
    received_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE TABLE social_report_admission (
    source TEXT PRIMARY KEY,
    day INTEGER NOT NULL,
    used INTEGER NOT NULL
);

CREATE TABLE social_report_seen (
    id TEXT PRIMARY KEY,
    source TEXT NOT NULL,
    hash TEXT NOT NULL,
    expires_at INTEGER NOT NULL
);

CREATE TABLE social_subscription_job (
    id TEXT PRIMARY KEY,
    config_hash TEXT NOT NULL,
    lease_token TEXT NOT NULL DEFAULT '',
    lease_until INTEGER NOT NULL DEFAULT 0,
    last_attempt INTEGER,
    last_network_attempt INTEGER,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    last_completed INTEGER,
    error TEXT CHECK(error IS NULL OR length(CAST(error AS BLOB))<=2048),
    source TEXT NOT NULL DEFAULT 'Not checked',
    results TEXT NOT NULL DEFAULT '[]' CHECK(length(CAST(results AS BLOB))<=16384)
);

CREATE TABLE social_subscription_seen (
    id TEXT PRIMARY KEY,
    subscription TEXT NOT NULL,
    post TEXT NOT NULL,
    hash TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    matched_at INTEGER NOT NULL,
    notified INTEGER NOT NULL CHECK(notified IN (0,1))
);

CREATE TABLE social_discovery_source (
    post TEXT NOT NULL,
    hash TEXT NOT NULL,
    source TEXT NOT NULL CHECK(length(CAST(source AS BLOB))<=128),
    checked_at INTEGER NOT NULL,
    PRIMARY KEY(post,hash,source)
);

CREATE TABLE social_discovery_conflict (
    post TEXT PRIMARY KEY,
    generation INTEGER NOT NULL,
    revision INTEGER NOT NULL,
    first_hash TEXT NOT NULL,
    first_body TEXT NOT NULL,
    second_hash TEXT NOT NULL,
    second_body TEXT NOT NULL,
    observed_at INTEGER NOT NULL
);

CREATE TABLE "social_profile_authority" (
    organ TEXT PRIMARY KEY,
    root_key TEXT NOT NULL,
    editor_key TEXT NOT NULL,
    generation INTEGER NOT NULL CHECK(generation>=0 AND (generation>0 OR editor_key='')),
    body TEXT NOT NULL CHECK(length(CAST(body AS BLOB))<=8192),
    expires_at INTEGER NOT NULL,
    profile_revision INTEGER NOT NULL DEFAULT 0,
    profile_hash TEXT
);

CREATE TABLE "social_private_outbox" (
    id TEXT PRIMARY KEY,
    context TEXT NOT NULL,
    body TEXT NOT NULL CHECK(length(CAST(body AS BLOB)) <= CASE WHEN json_extract(CASE WHEN json_valid(body) THEN body ELSE '{}' END, '$.envelope.purpose') = 'content' THEN 8388608 ELSE 32768 END),
    hash TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','stored','ready','expired','held','cancelled')),
    error TEXT,
    record_uid TEXT REFERENCES record(uid),
    recipient_owner TEXT NOT NULL DEFAULT '',
    recipient_generation INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE "social_private_destination" (
    envelope TEXT NOT NULL REFERENCES "social_private_outbox"(id) ON DELETE CASCADE,
    service TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending' CHECK(state IN ('pending','stored','failed','cancelled')),
    attempts INTEGER NOT NULL DEFAULT 0,
    next_attempt INTEGER NOT NULL DEFAULT 0,
    receipt TEXT,
    error TEXT,
    PRIMARY KEY(envelope,service)
);

CREATE TABLE "social_service_envelope" (
    id TEXT PRIMARY KEY,
    route TEXT NOT NULL REFERENCES social_reply_route(id),
    sender TEXT NOT NULL,
    hash TEXT NOT NULL,
    body TEXT NOT NULL CHECK(length(CAST(body AS BLOB)) <= CASE WHEN partition = 'trusted' AND json_extract(CASE WHEN json_valid(body) THEN body ELSE '{}' END, '$.envelope.purpose') = 'content' THEN 8388608 ELSE 32768 END),
    partition TEXT NOT NULL CHECK(partition IN ('stranger','trusted','control')),
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    next_pickup INTEGER NOT NULL DEFAULT 0
);

CREATE VIRTUAL TABLE "social_search" USING fts5(id, title, text);

CREATE TABLE mailbox_outbox_authority (
    uid TEXT PRIMARY KEY REFERENCES mailbox_outbox(uid) ON DELETE CASCADE,
    policy_hash TEXT NOT NULL
);

CREATE TABLE record_move_offer (
    uid TEXT PRIMARY KEY,
    root TEXT NOT NULL,
    peer TEXT NOT NULL,
    direction TEXT NOT NULL CHECK(direction IN ('incoming', 'outgoing')),
    state TEXT NOT NULL CHECK(state IN ('offered', 'accepted', 'transferring', 'received', 'complete', 'cancelled', 'declined', 'changed')),
    preview TEXT NOT NULL CHECK(json_valid(preview)),
    payload TEXT CHECK(payload IS NULL OR json_valid(payload)),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    cancel_sent INTEGER NOT NULL DEFAULT 0 CHECK(cancel_sent IN (0, 1)),
    error TEXT
);

CREATE TABLE record_move_member (
    offer_uid TEXT NOT NULL REFERENCES record_move_offer(uid),
    record_uid TEXT NOT NULL,
    PRIMARY KEY(offer_uid, record_uid)
);

CREATE TABLE offer_local_outcome (
    kind TEXT NOT NULL,
    subject_uid TEXT NOT NULL,
    other_party TEXT NOT NULL,
    outcome TEXT NOT NULL,
    at TEXT NOT NULL,
    PRIMARY KEY(kind, subject_uid, other_party)
);

CREATE TABLE description_assets (
    record_uid TEXT NOT NULL REFERENCES record(uid) ON DELETE CASCADE,
    asset TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('drawing', 'png', 'webp')),
    bytes BLOB NOT NULL CHECK(length(bytes) <= 4194304),
    PRIMARY KEY(record_uid, asset)
);

CREATE TABLE sand_package (
    origin TEXT NOT NULL,
    id TEXT NOT NULL,
    version INTEGER NOT NULL CHECK (version > 0),
    document TEXT NOT NULL,
    manifest TEXT NOT NULL,
    digest TEXT NOT NULL,
    public INTEGER NOT NULL DEFAULT 0 CHECK (public IN (0, 1)),
    origin_verified INTEGER NOT NULL CHECK (origin_verified IN (0, 1)),
    received_from TEXT,
    PRIMARY KEY (origin, id, version)
);

CREATE TABLE projection_schedule (
    id TEXT PRIMARY KEY,
    record_uid TEXT NOT NULL,
    from_ms INTEGER NOT NULL,
    until_ms INTEGER,
    payload TEXT NOT NULL CHECK(json_valid(payload)),
    CHECK(from_ms >= 0),
    CHECK(until_ms IS NULL OR until_ms > from_ms)
) STRICT;

CREATE TABLE organ_access_catalog (
    organ_uid TEXT PRIMARY KEY REFERENCES record(uid) ON DELETE CASCADE,
    generation INTEGER NOT NULL CHECK (generation >= 0),
    granted INTEGER NOT NULL CHECK (granted IN (0, 1)),
    observed_at TEXT NOT NULL
) STRICT;

CREATE TABLE person_role (
    person_uid TEXT NOT NULL REFERENCES person_access(person_uid) ON DELETE CASCADE,
    role_id INTEGER NOT NULL REFERENCES role(id),
    PRIMARY KEY (person_uid, role_id)
) STRICT;

CREATE TABLE authority_seed_state (id INTEGER PRIMARY KEY CHECK (id = 1)) STRICT;

CREATE TABLE shared_workspace (
    uid TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK (revision > 0),
    policy TEXT NOT NULL CHECK (json_valid(policy)),
    layout TEXT NOT NULL CHECK (json_valid(layout))
) STRICT;

CREATE TABLE workspace_change (
    uid TEXT PRIMARY KEY,
    workspace_uid TEXT NOT NULL REFERENCES shared_workspace(uid) ON DELETE CASCADE,
    actor_uid TEXT NOT NULL,
    base_revision INTEGER NOT NULL,
    applied_revision INTEGER,
    change TEXT NOT NULL CHECK (json_valid(change)),
    status TEXT NOT NULL CHECK (status IN ('pending', 'applied', 'rejected')),
    reviewed_by TEXT,
    created_at TEXT NOT NULL
, author_admission TEXT CHECK (author_admission IS NULL OR json_valid(author_admission))) STRICT;

CREATE TABLE workspace_receipt (
    workspace_uid TEXT NOT NULL REFERENCES shared_workspace(uid) ON DELETE CASCADE,
    actor_uid TEXT NOT NULL,
    request_id TEXT NOT NULL,
    payload TEXT NOT NULL,
    result TEXT NOT NULL CHECK (json_valid(result)),
    PRIMARY KEY (workspace_uid, actor_uid, request_id)
) STRICT;

CREATE TABLE workspace_draft (
    uid TEXT PRIMARY KEY,
    actor_uid TEXT NOT NULL,
    host_uid TEXT,
    workspace_uid TEXT NOT NULL,
    base_revision INTEGER NOT NULL CHECK (base_revision > 0),
    change TEXT NOT NULL CHECK (json_valid(change)),
    saved_at TEXT NOT NULL
) STRICT;

INSERT INTO "commit_sequence" ("id", "value") VALUES (1, 0);

INSERT INTO "karma_intent_status" ("status", "holds_reservation") VALUES ('authorized', 1);

INSERT INTO "karma_intent_status" ("status", "holds_reservation") VALUES ('cancelled', 0);

INSERT INTO "karma_occurrence_processing_state" ("singleton", "next_cell_sequence") VALUES (1, 1);

INSERT INTO "karma_occurrence_sequence" ("singleton", "next_sequence") VALUES (1, 1);

INSERT INTO "karma_rule_progress" ("singleton", "next_sequence") VALUES (1, 1);

INSERT INTO "projection_source" ("id", "revision", "runtime") VALUES (1, 0, NULL);

INSERT INTO "self_update" ("id", "automatic", "installed_revision", "installed_at") VALUES (1, 0, NULL, NULL);

INSERT INTO "social_gossip_scan" ("id", "after", "error") VALUES (1, '', NULL);

INSERT INTO "sync_history_policy" ("id", "seconds", "max_entries") VALUES (1, 604800, 1000);

INSERT INTO "transfer_sync_control" ("id", "importing") VALUES (1, 0);

DELETE FROM sqlite_sequence;

CREATE INDEX idx_record_kind ON record(kind);

CREATE INDEX idx_fact_record_at ON fact(record_uid, at);

CREATE INDEX idx_fact_cause ON fact(cause_kind, cause_uid);

CREATE UNIQUE INDEX idx_assertion_unary_active
    ON record_assertion(subject_uid, predicate_uid)
    WHERE object_uid IS NULL AND retracted_at IS NULL;

CREATE UNIQUE INDEX idx_assertion_binary_active
    ON record_assertion(subject_uid, predicate_uid, object_uid)
    WHERE object_uid IS NOT NULL AND retracted_at IS NULL;

CREATE UNIQUE INDEX idx_assertion_identity_active
    ON record_assertion(subject_uid)
    WHERE role = 'identity' AND retracted_at IS NULL;

CREATE INDEX idx_assertion_subject
    ON record_assertion(subject_uid, predicate_uid, retracted_at);

CREATE INDEX idx_assertion_object
    ON record_assertion(object_uid, predicate_uid, retracted_at);

CREATE INDEX idx_promise_record_state ON promise(record_uid, state);

CREATE INDEX idx_promise_transfer ON promise(transfer_uid);

CREATE INDEX idx_record_deleted ON record(deleted_at);

CREATE INDEX idx_record_organ ON record(organ_uid);

CREATE UNIQUE INDEX transfer_party_invitation_identity
    ON transfer_party (uid, transfer_uid, actor_uid);

CREATE UNIQUE INDEX transfer_invitation_one_pending
    ON transfer_invitation (transfer_uid, addressed_person_uid)
    WHERE status = 'pending';

CREATE INDEX transfer_invitation_by_transfer
    ON transfer_invitation (transfer_uid, created_at);

CREATE INDEX transfer_invitation_inbox
    ON transfer_invitation (addressed_person_uid, status, created_at);

CREATE INDEX promise_unit
    ON promise(unit_uid);

CREATE INDEX promise_source
    ON promise(source_promise_uid);

CREATE UNIQUE INDEX transfer_invitation_event_identity
    ON transfer_invitation(uid, transfer_uid);

CREATE INDEX transfer_invitation_event_history
    ON transfer_invitation_event(invitation_uid, attempt, created_at);

CREATE INDEX transfer_invitation_event_transfer
    ON transfer_invitation_event(transfer_uid, created_at);

CREATE INDEX transfer_agreement_event_history
    ON transfer_agreement_event(transfer_uid, revision, party_uid, created_at, uid);

CREATE UNIQUE INDEX transfer_party_agreement_identity
    ON transfer_party(uid, transfer_uid);

CREATE INDEX transfer_dependency_current
    ON transfer_dependency(transfer_uid, revision, scope, promise_uid);

CREATE INDEX transfer_dependency_upstream
    ON transfer_dependency(upstream_kind, upstream_uid);

CREATE UNIQUE INDEX promise_occurrence_identity
    ON promise(uid, transfer_uid);

CREATE UNIQUE INDEX transfer_exchange_path_opposite
    ON transfer_exchange_path(transfer_uid, revision, opposite_promise_uid)
    WHERE opposite_promise_uid IS NOT NULL;

CREATE INDEX transfer_occurrence_path
    ON transfer_occurrence(exchange_path_uid, created_at);

CREATE INDEX transfer_occurrence_roles
    ON transfer_occurrence(transfer_uid, giver_person_uid, receiver_person_uid);

CREATE INDEX transfer_occurrence_claim_history
    ON transfer_occurrence_claim_event(occurrence_uid, role, created_at, uid);

CREATE UNIQUE INDEX transfer_occurrence_application_history
    ON transfer_occurrence_application_event(occurrence_uid, version);

CREATE INDEX fact_action_intent_by_intent
    ON fact_action_intent(intent_uid, fact_uid);

CREATE INDEX transfer_occurrence_settlement_history
    ON transfer_occurrence_settlement_slice(occurrence_uid, created_at, uid);

CREATE INDEX transfer_occurrence_dispute_history
    ON transfer_occurrence_dispute_event(
        occurrence_uid, actor_person_uid, created_at, uid
    );

CREATE INDEX transfer_delivery_outbox_due
    ON transfer_delivery_outbox(status, next_attempt_at, created_at);

CREATE INDEX transfer_delivery_pull_due
    ON transfer_delivery_pull_request(status, next_attempt_at, created_at);

CREATE INDEX transfer_remote_command_due
    ON transfer_remote_command(direction, status, next_attempt_at, created_at);

CREATE INDEX fact_remote_command_by_command ON fact_remote_command(command_uid);

CREATE INDEX transfer_application_attestation_outbox_due
    ON transfer_application_attestation_outbox(status, next_attempt_at, created_at);

CREATE INDEX karma_program_status ON karma_program(status, updated_at, record_uid);

CREATE INDEX karma_program_revision_program ON karma_program_revision(program_uid, created_at);

CREATE INDEX karma_frequency_status
    ON karma_frequency(status, updated_at, record_uid);

CREATE INDEX karma_frequency_revision_frequency
    ON karma_frequency_revision(frequency_uid, created_at);

CREATE INDEX karma_frequency_activation_resolution
    ON karma_frequency_activation(frequency_uid, effective_parameter_hash);

CREATE INDEX karma_schedule_occurrence_intended_range
    ON karma_schedule_occurrence(activation_hash, first_intended_at, last_intended_at);

CREATE INDEX karma_occurrence_replay_order
    ON karma_occurrence(cell_sequence, occurrence_hash);

CREATE INDEX karma_occurrence_logical_order
    ON karma_occurrence(logical_at, source_kind, source_identity);

CREATE INDEX karma_occurrence_parent
    ON karma_occurrence(parent_occurrence_hash, cell_sequence);

CREATE INDEX karma_schedule_occurrence_expansion_pending
    ON karma_schedule_occurrence_expansion(completed, schedule_occurrence_hash);

CREATE INDEX karma_occurrence_program_epoch_pending
    ON karma_occurrence_program_epoch(completed, cell_sequence);

CREATE INDEX karma_run_replay_order
    ON karma_run(cell_sequence, member_ordinal, run_hash);

CREATE INDEX karma_run_program_order
    ON karma_run(program_uid, cell_sequence, member_ordinal);

CREATE INDEX karma_candidate_program_order
    ON karma_candidate(program_uid, created_at, candidate_hash);

CREATE INDEX karma_candidate_occurrence_order
    ON karma_candidate(occurrence_hash, candidate_hash);

CREATE INDEX karma_grant_principal_status
ON karma_grant(principal_person_uid, status, updated_at, record_uid);

CREATE INDEX karma_grant_revision_grant
ON karma_grant_revision(grant_uid, created_at, revision_hash);

CREATE INDEX karma_intent_grant_window
    ON karma_intent(grant_uid, window_index, intent_hash);

CREATE INDEX karma_intent_candidate
    ON karma_intent(candidate_hash, intent_hash);

CREATE INDEX karma_intent_state_status
    ON karma_intent_state(status, intent_hash);

CREATE INDEX idx_fact_concept_event_fact ON fact_concept_event(fact_uid);

CREATE INDEX idx_fact_concept_concept ON fact_concept(concept_uid);

CREATE INDEX idx_entry_record ON entry(record_uid, occurred_at);

CREATE INDEX idx_entry_fact ON entry(fact_uid);

CREATE UNIQUE INDEX idx_entry_revision_request
    ON entry_revision(request_id);

CREATE INDEX idx_recurrence_record ON recurrence(record_uid, state);

CREATE UNIQUE INDEX idx_recurrence_revision_request
    ON recurrence_revision(request_id);

CREATE UNIQUE INDEX idx_sync_op_identity ON sync_op(actor_cell, hlc);

CREATE INDEX idx_sync_op_target ON sync_op(tbl, uid, field);

CREATE INDEX idx_sync_op_feed ON sync_op(organ_uid, seq);

CREATE UNIQUE INDEX idx_organ_contact_node_id
    ON organ_contact(node_id) WHERE node_id IS NOT NULL;

CREATE INDEX idx_record_replica_root
    ON record(replica_root) WHERE replica_root IS NOT NULL;

CREATE INDEX idx_sync_op_replica_root ON sync_op(replica_root, seq);

CREATE INDEX idx_replica_grant_contact ON replica_grant(contact_organ);

CREATE UNIQUE INDEX idx_door_request_node ON door_request(node_id);

CREATE INDEX reference_read_by_record ON reference_read(record_uid, last_read_at DESC);

CREATE INDEX karma_program_execution_off
    ON karma_program_execution(executes, program_uid);

CREATE INDEX mailbox_bundle_by_recipient ON mailbox_bundle (to_organ, received_at);

CREATE INDEX mailbox_bundle_by_expiry ON mailbox_bundle (expires_at);

CREATE INDEX mailbox_expiry_notice_pending ON mailbox_expiry_notice (from_node, notified_at);

CREATE INDEX contact_share_by_record ON contact_share (record_uid);

CREATE UNIQUE INDEX idx_recurrence_slug ON recurrence(slug);

CREATE INDEX record_move_by_contact ON record_move (contact_organ);

CREATE INDEX idx_record_change_record ON record_change(record_uid, seq DESC);

CREATE INDEX idx_record_change_at ON record_change(at);

CREATE INDEX offer_refusal_by_party ON offer_refusal (other_party, kind);

CREATE INDEX sync_activity_at ON sync_activity(at);

CREATE INDEX person_access_role ON person_access(role_id);

CREATE UNIQUE INDEX effect_queue_request ON effect_queue(request_id) WHERE request_id IS NOT NULL;

CREATE INDEX karma_field_readers ON karma_field_binding(field_uid);

CREATE INDEX projection_span_window ON projection_span(from_ms, until_ms);

CREATE INDEX projection_span_record ON projection_span(record_uid, from_ms, until_ms);

CREATE INDEX blob_sync_work ON blob_sync(owner, state, settled);

CREATE INDEX blob_sync_peer ON blob_sync(peer, direction, state);

CREATE INDEX transfer_exchange_public_route ON transfer_exchange_path(transfer_uid, public_exchange_uid, revision);

CREATE INDEX fact_committed_batch ON fact(commit_sequence);

CREATE INDEX transfer_open_claim_pair_source
    ON transfer_open_claim_pair(source_promise_uid, revision, uid);

CREATE INDEX transfer_parent_children ON transfer(parent_uid, record_uid);

CREATE INDEX transfer_private_effect_occurrence ON transfer_private_effect(occurrence_uid, person_uid, record_uid);

CREATE INDEX transfer_private_effect_group ON transfer_private_effect(primary_fact_uid);

CREATE INDEX transfer_sync_journal_commit ON transfer_sync_journal(cell_uid, commit_sequence, seq);

CREATE UNIQUE INDEX karma_schedule_current_boundary ON karma_schedule_boundary(schedule_uid, purpose) WHERE current = 1;

CREATE INDEX karma_schedule_boundary_rule ON karma_schedule_boundary(rule_uid);

CREATE INDEX karma_schedule_cursor_armed_deadline
ON karma_schedule_cursor(lifecycle, next_intended_at, required_resolution_ms, activation_hash);

CREATE INDEX karma_schedule_cursor_frequency
ON karma_schedule_cursor(frequency_uid, lifecycle, activation_hash);

CREATE INDEX karma_schedule_cursor_expired_lease
ON karma_schedule_cursor(lifecycle, lease_expires_at, activation_hash);

CREATE INDEX karma_transfer_command_rule ON karma_transfer_command(rule_uid, rule_revision);

CREATE INDEX mailbox_inbox_pending ON mailbox_inbox(state, next_attempt);

CREATE INDEX mailbox_outbox_pending ON mailbox_outbox(next_attempt, expires_at);

CREATE INDEX mail_left_by_carrier ON mail_left(carrier_node, expired_at);

CREATE INDEX mail_left_by_recipient ON mail_left(to_organ, expired_at);

CREATE INDEX mailbox_completion_expiry ON mailbox_completion(expires_at);

CREATE INDEX social_document_expiry ON social_document(expires_at);

CREATE INDEX karma_command_invocation_command ON karma_command_invocation(command_uid, created_at);

CREATE INDEX effect_queue_active ON effect_queue(status) WHERE status IN ('queued', 'running');

CREATE INDEX fiote_activation_pending ON fiote_activation(fiote_uid, state, created_at);

CREATE INDEX social_device_context ON social_device_state(context,kind);

CREATE INDEX social_private_seen_cipher ON social_private_seen(cipher_hash);

CREATE INDEX social_reply_expiry ON social_reply_route(expires_at);

CREATE INDEX social_reply_post ON social_reply_route(post);

CREATE INDEX social_reply_owner ON social_reply_route(owner,id);

CREATE UNIQUE INDEX social_gossip_item_identity ON social_gossip_item(kind,document_hash);

CREATE INDEX social_gossip_forward_due ON social_gossip_forward(state,next_attempt);

CREATE INDEX social_ask_query_due ON social_ask_query(state,deadline);

CREATE INDEX social_ask_seen_source ON social_ask_seen(source,deadline);

CREATE INDEX social_context_retention_check ON social_context_retention(checked_at,context);

CREATE INDEX social_report_work_actor ON social_report_work(actor,created_at);

CREATE INDEX social_report_work_due ON social_report_work(state,next_attempt);

CREATE INDEX social_subscription_due ON social_subscription_job(next_attempt,lease_until);

CREATE INDEX social_subscription_notice ON social_subscription_seen(subscription,notified,matched_at);

CREATE INDEX social_subscription_expiry ON social_subscription_seen(expires_at);

CREATE INDEX social_discovery_source_age ON social_discovery_source(checked_at,post,hash,source);

CREATE INDEX social_discovery_conflict_page ON social_document(kind,state,id);

CREATE INDEX social_revision_expiry ON social_revision(expires_at);

CREATE INDEX social_document_hash ON social_document(hash);

CREATE INDEX social_private_message ON social_private_outbox(record_uid,state);

CREATE INDEX social_service_route ON social_service_envelope(route,created_at,id);

CREATE INDEX record_move_offer_peer ON record_move_offer(peer, direction, state);

CREATE INDEX record_move_member_record ON record_move_member(record_uid);

CREATE INDEX sand_package_public ON sand_package(public, origin, id, version DESC);

CREATE INDEX sand_package_library ON sand_package(origin, id, version DESC);

CREATE INDEX projection_schedule_window ON projection_schedule(from_ms, until_ms);

CREATE INDEX workspace_change_history ON workspace_change(workspace_uid, created_at, uid);

CREATE INDEX workspace_draft_owner ON workspace_draft(actor_uid, saved_at, uid);

CREATE VIEW transfer_application_effect_handoff AS
SELECT uid, reference_uid, origin_organ_uid, participant_organ_uid, participant_person_uid,
       transfer_uid, occurrence_uid, source_promise_uid, settlement_slice_uid, origin_revision,
       canonical_quantity, canonical_unit_uid, canonical_cumulative_before, canonical_cumulative_after,
       canonical_remaining_after, application_direction, canonical_slice_hash, envelope_uid,
       envelope_payload_hash, origin_created_at, state, origin_state, local_application_uid
FROM transfer_remote_application_handoff
UNION ALL
SELECT h.uid, '', h.origin_organ_uid, h.participant_organ_uid, h.participant_person_uid,
       h.transfer_uid, h.occurrence_uid, d.source_promise_uid, h.settlement_slice_uid, h.origin_revision,
       d.canonical_quantity, d.canonical_unit_uid, d.canonical_cumulative_before, d.canonical_cumulative_after,
       d.canonical_remaining_after, d.application_direction, h.canonical_slice_hash, '', '', h.created_at,
       CASE WHEN a.uid IS NOT NULL THEN 'applied' ELSE h.state END, h.state, a.uid
FROM transfer_application_handoff h
JOIN transfer_application_handoff_detail d ON d.handoff_uid = h.uid
LEFT JOIN transfer_local_application a ON a.handoff_uid = h.uid
WHERE h.origin_organ_uid = h.participant_organ_uid;

CREATE TRIGGER transfer_agreement_event_request_collision
BEFORE INSERT ON transfer_agreement_event
WHEN EXISTS (
        SELECT 1 FROM transfer_revision
        WHERE idempotency_key = NEW.idempotency_key
    ) OR EXISTS (
        SELECT 1 FROM transfer_invitation_event
        WHERE idempotency_key = NEW.idempotency_key
    )
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by another action');
END;

CREATE TRIGGER transfer_revision_agreement_request_collision
BEFORE INSERT ON transfer_revision
WHEN EXISTS (
    SELECT 1 FROM transfer_agreement_event
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by agreement action');
END;

CREATE TRIGGER transfer_invitation_agreement_request_collision
BEFORE INSERT ON transfer_invitation_event
WHEN EXISTS (
    SELECT 1 FROM transfer_agreement_event
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by agreement action');
END;

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

CREATE TRIGGER transfer_phase5_request_collision
BEFORE INSERT ON transfer_phase5_request
WHEN EXISTS (
        SELECT 1 FROM transfer_revision
        WHERE idempotency_key = NEW.idempotency_key
    ) OR EXISTS (
        SELECT 1 FROM transfer_invitation_event
        WHERE idempotency_key = NEW.idempotency_key
    ) OR EXISTS (
        SELECT 1 FROM transfer_agreement_event
        WHERE idempotency_key = NEW.idempotency_key
    ) OR EXISTS (
        SELECT 1 FROM transfer_phase4_request
        WHERE idempotency_key = NEW.idempotency_key
    )
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by another action');
END;

CREATE TRIGGER transfer_revision_phase5_request_collision
BEFORE INSERT ON transfer_revision
WHEN EXISTS (
    SELECT 1 FROM transfer_phase5_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by settlement action');
END;

CREATE TRIGGER transfer_invitation_phase5_request_collision
BEFORE INSERT ON transfer_invitation_event
WHEN EXISTS (
    SELECT 1 FROM transfer_phase5_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by settlement action');
END;

CREATE TRIGGER transfer_agreement_phase5_request_collision
BEFORE INSERT ON transfer_agreement_event
WHEN EXISTS (
    SELECT 1 FROM transfer_phase5_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by settlement action');
END;

CREATE TRIGGER transfer_phase4_phase5_request_collision
BEFORE INSERT ON transfer_phase4_request
WHEN EXISTS (
    SELECT 1 FROM transfer_phase5_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by settlement action');
END;

CREATE TRIGGER transfer_occurrence_settlement_eligible
BEFORE INSERT ON transfer_occurrence_settlement_slice
WHEN NOT EXISTS (
    SELECT 1
    FROM transfer_occurrence occurrence
    JOIN promise source ON source.uid = occurrence.promise_uid
    WHERE occurrence.uid = NEW.occurrence_uid
      AND occurrence.transfer_uid = NEW.transfer_uid
      AND occurrence.promise_uid = NEW.promise_uid
      AND occurrence.delivery_claimed = 1
      AND occurrence.receipt_claimed = 1
      AND occurrence.disputed = 0
      AND source.party_uid = NEW.owner_person_uid
      AND source.state = 'active'
)
BEGIN
    SELECT RAISE(ABORT, 'occurrence is not eligible for this settlement');
END;

CREATE TRIGGER transfer_occurrence_settlement_progress
BEFORE INSERT ON transfer_occurrence_settlement_slice
WHEN NOT EXISTS (
    SELECT 1
    FROM transfer_occurrence occurrence
    WHERE occurrence.uid = NEW.occurrence_uid
      AND NEW.cumulative_before = COALESCE((
          SELECT MAX(existing.cumulative_after)
          FROM transfer_occurrence_settlement_slice existing
          WHERE existing.occurrence_uid = NEW.occurrence_uid
      ), 0.0)
      AND NEW.cumulative_after <= occurrence.quantity
)
BEGIN
    SELECT RAISE(ABORT, 'settlement slice does not match current occurrence progress');
END;

CREATE TRIGGER transfer_occurrence_settlement_immutable_update
BEFORE UPDATE ON transfer_occurrence_settlement_slice
BEGIN
    SELECT RAISE(ABORT, 'settlement slices are immutable');
END;

CREATE TRIGGER transfer_occurrence_settlement_immutable_delete
BEFORE DELETE ON transfer_occurrence_settlement_slice
BEGIN
    SELECT RAISE(ABORT, 'settlement slices are immutable');
END;

CREATE TRIGGER transfer_phase5_correction_request_collision
BEFORE INSERT ON transfer_phase5_correction_request
WHEN EXISTS (
        SELECT 1 FROM transfer_revision
        WHERE idempotency_key = NEW.idempotency_key
    ) OR EXISTS (
        SELECT 1 FROM transfer_invitation_event
        WHERE idempotency_key = NEW.idempotency_key
    ) OR EXISTS (
        SELECT 1 FROM transfer_agreement_event
        WHERE idempotency_key = NEW.idempotency_key
    ) OR EXISTS (
        SELECT 1 FROM transfer_phase4_request
        WHERE idempotency_key = NEW.idempotency_key
    ) OR EXISTS (
        SELECT 1 FROM transfer_phase5_request
        WHERE idempotency_key = NEW.idempotency_key
    )
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by another action');
END;

CREATE TRIGGER transfer_revision_phase5_correction_request_collision
BEFORE INSERT ON transfer_revision
WHEN EXISTS (
    SELECT 1 FROM transfer_phase5_correction_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by correction action');
END;

CREATE TRIGGER transfer_invitation_phase5_correction_request_collision
BEFORE INSERT ON transfer_invitation_event
WHEN EXISTS (
    SELECT 1 FROM transfer_phase5_correction_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by correction action');
END;

CREATE TRIGGER transfer_agreement_phase5_correction_request_collision
BEFORE INSERT ON transfer_agreement_event
WHEN EXISTS (
    SELECT 1 FROM transfer_phase5_correction_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by correction action');
END;

CREATE TRIGGER transfer_phase4_phase5_correction_request_collision
BEFORE INSERT ON transfer_phase4_request
WHEN EXISTS (
    SELECT 1 FROM transfer_phase5_correction_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by correction action');
END;

CREATE TRIGGER transfer_phase5_phase5_correction_request_collision
BEFORE INSERT ON transfer_phase5_request
WHEN EXISTS (
    SELECT 1 FROM transfer_phase5_correction_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by correction action');
END;

CREATE TRIGGER transfer_occurrence_settlement_compensation_immutable_update
BEFORE UPDATE ON transfer_occurrence_settlement_compensation
BEGIN
    SELECT RAISE(ABORT, 'settlement compensations are immutable');
END;

CREATE TRIGGER transfer_occurrence_settlement_compensation_immutable_delete
BEFORE DELETE ON transfer_occurrence_settlement_compensation
BEGIN
    SELECT RAISE(ABORT, 'settlement compensations are immutable');
END;

CREATE TRIGGER transfer_occurrence_dispute_participant
BEFORE INSERT ON transfer_occurrence_dispute_event
WHEN NOT EXISTS (
    SELECT 1
    FROM transfer_occurrence occurrence
    JOIN fact evidence ON evidence.uid = NEW.fact_uid
    WHERE occurrence.uid = NEW.occurrence_uid
      AND occurrence.transfer_uid = NEW.transfer_uid
      AND NEW.actor_person_uid IN (
          occurrence.giver_person_uid, occurrence.receiver_person_uid
      )
      AND evidence.record_uid = NEW.transfer_uid
      AND evidence.delta_mantissa = '0'
      AND evidence.actor_uid = NEW.actor_person_uid
)
BEGIN
    SELECT RAISE(ABORT, 'occurrence dispute actor is not a participant');
END;

CREATE TRIGGER transfer_occurrence_dispute_immutable_update
BEFORE UPDATE ON transfer_occurrence_dispute_event
BEGIN
    SELECT RAISE(ABORT, 'occurrence dispute events are immutable');
END;

CREATE TRIGGER transfer_occurrence_dispute_immutable_delete
BEFORE DELETE ON transfer_occurrence_dispute_event
BEGIN
    SELECT RAISE(ABORT, 'occurrence dispute events are immutable');
END;

CREATE TRIGGER transfer_open_promise_requires_proposer_insert
BEFORE INSERT ON promise
WHEN NEW.state = 'open' AND NEW.party_uid IS NULL
BEGIN
    SELECT RAISE(ABORT, 'OPEN promise requires a proposer Person');
END;

CREATE TRIGGER transfer_open_promise_requires_proposer_update
BEFORE UPDATE OF state, party_uid ON promise
WHEN NEW.state = 'open' AND NEW.party_uid IS NULL
BEGIN
    SELECT RAISE(ABORT, 'OPEN promise requires a proposer Person');
END;

CREATE TRIGGER transfer_open_promise_requires_party_insert
BEFORE INSERT ON promise
WHEN NEW.state = 'open' AND NEW.transfer_uid IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM transfer_party party
    WHERE party.transfer_uid = NEW.transfer_uid
      AND party.actor_uid = NEW.party_uid
)
BEGIN
    SELECT RAISE(ABORT, 'OPEN proposer must be a current Transfer party');
END;

CREATE TRIGGER transfer_open_promise_requires_party_update
BEFORE UPDATE OF state, party_uid, transfer_uid ON promise
WHEN NEW.state = 'open' AND NEW.transfer_uid IS NOT NULL AND NOT EXISTS (
    SELECT 1 FROM transfer_party party
    WHERE party.transfer_uid = NEW.transfer_uid
      AND party.actor_uid = NEW.party_uid
)
BEGIN
    SELECT RAISE(ABORT, 'OPEN proposer must be a current Transfer party');
END;

CREATE TRIGGER transfer_phase6_bulk_request_collision
BEFORE INSERT ON transfer_phase6_bulk_request
WHEN EXISTS (SELECT 1 FROM transfer_revision WHERE idempotency_key = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_invitation_event WHERE idempotency_key = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_agreement_event WHERE idempotency_key = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_phase4_request WHERE idempotency_key = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_phase5_request WHERE idempotency_key = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_phase5_correction_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by another action');
END;

CREATE TRIGGER transfer_revision_phase6_bulk_request_collision
BEFORE INSERT ON transfer_revision
WHEN EXISTS (
    SELECT 1 FROM transfer_phase6_bulk_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by bulk completion');
END;

CREATE TRIGGER transfer_invitation_phase6_bulk_request_collision
BEFORE INSERT ON transfer_invitation_event
WHEN EXISTS (
    SELECT 1 FROM transfer_phase6_bulk_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by bulk completion');
END;

CREATE TRIGGER transfer_agreement_phase6_bulk_request_collision
BEFORE INSERT ON transfer_agreement_event
WHEN EXISTS (
    SELECT 1 FROM transfer_phase6_bulk_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by bulk completion');
END;

CREATE TRIGGER transfer_phase4_phase6_bulk_request_collision
BEFORE INSERT ON transfer_phase4_request
WHEN EXISTS (
    SELECT 1 FROM transfer_phase6_bulk_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by bulk completion');
END;

CREATE TRIGGER transfer_phase5_phase6_bulk_request_collision
BEFORE INSERT ON transfer_phase5_request
WHEN EXISTS (
    SELECT 1 FROM transfer_phase6_bulk_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by bulk completion');
END;

CREATE TRIGGER transfer_phase5_correction_phase6_bulk_request_collision
BEFORE INSERT ON transfer_phase5_correction_request
WHEN EXISTS (
    SELECT 1 FROM transfer_phase6_bulk_request
    WHERE idempotency_key = NEW.idempotency_key
)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id already used by bulk completion');
END;

CREATE TRIGGER transfer_parent_not_self_insert
BEFORE INSERT ON transfer WHEN NEW.parent_uid = NEW.record_uid
BEGIN
    SELECT RAISE(ABORT, 'Transfer cannot be its own parent');
END;

CREATE TRIGGER transfer_parent_not_self_update
BEFORE UPDATE OF parent_uid ON transfer WHEN NEW.parent_uid = NEW.record_uid
BEGIN
    SELECT RAISE(ABORT, 'Transfer cannot be its own parent');
END;

CREATE TRIGGER transfer_correction_link_consistent
BEFORE INSERT ON transfer_correction_link
WHEN NEW.source_transfer_uid = NEW.created_transfer_uid
  OR NOT EXISTS (
      SELECT 1 FROM transfer_occurrence occurrence
      WHERE occurrence.uid = NEW.source_occurrence_uid
        AND occurrence.transfer_uid = NEW.source_transfer_uid
  )
  OR NOT EXISTS (
      SELECT 1 FROM transfer_revision revision
      WHERE revision.transfer_uid = NEW.created_transfer_uid
        AND revision.revision = 1
        AND revision.fact_uid = NEW.fact_uid
        AND revision.idempotency_key = NEW.idempotency_key
  )
BEGIN
    SELECT RAISE(ABORT, 'Transfer correction lineage is inconsistent');
END;

CREATE TRIGGER transfer_promise_successor_same_transfer
BEFORE INSERT ON transfer_promise_successor
WHEN NOT EXISTS (
    SELECT 1 FROM promise predecessor
    JOIN promise successor ON successor.uid = NEW.successor_promise_uid
    JOIN transfer_revision revision
      ON revision.transfer_uid = NEW.transfer_uid
     AND revision.revision = NEW.revision
     AND revision.fact_uid = NEW.fact_uid
     AND revision.idempotency_key = NEW.idempotency_key
    WHERE predecessor.uid = NEW.predecessor_promise_uid
      AND predecessor.transfer_uid = NEW.transfer_uid
      AND successor.transfer_uid = NEW.transfer_uid
      AND successor.source_promise_uid = predecessor.uid
      AND successor.revision = NEW.revision
)
BEGIN
    SELECT RAISE(ABORT, 'promise successor lineage does not match Transfer promises');
END;

CREATE TRIGGER transfer_delivery_policy_identity_immutable
BEFORE UPDATE ON transfer_delivery_policy
WHEN NEW.transfer_uid != OLD.transfer_uid
  OR NEW.origin_organ_uid != OLD.origin_organ_uid
  OR NEW.recipient_person_uid != OLD.recipient_person_uid
  OR NEW.recipient_organ_uid != OLD.recipient_organ_uid
BEGIN
    SELECT RAISE(ABORT, 'Transfer delivery identity is immutable');
END;

CREATE TRIGGER transfer_delivery_policy_event_immutable_update
BEFORE UPDATE ON transfer_delivery_policy_event
BEGIN
    SELECT RAISE(ABORT, 'Transfer delivery policy events are immutable');
END;

CREATE TRIGGER transfer_delivery_policy_event_immutable_delete
BEFORE DELETE ON transfer_delivery_policy_event
BEGIN
    SELECT RAISE(ABORT, 'Transfer delivery policy events are immutable');
END;

CREATE TRIGGER transfer_delivery_retry_event_immutable_update
BEFORE UPDATE ON transfer_delivery_retry_event
BEGIN SELECT RAISE(ABORT, 'Transfer delivery retry events are immutable'); END;

CREATE TRIGGER transfer_delivery_retry_event_immutable_delete
BEFORE DELETE ON transfer_delivery_retry_event
BEGIN SELECT RAISE(ABORT, 'Transfer delivery retry events are immutable'); END;

CREATE TRIGGER transfer_delivery_outbox_identity_immutable
BEFORE UPDATE ON transfer_delivery_outbox
WHEN NEW.envelope_uid != OLD.envelope_uid
  OR NEW.delivery_uid != OLD.delivery_uid
  OR NEW.cursor != OLD.cursor
  OR NEW.transfer_revision != OLD.transfer_revision
  OR NEW.payload != OLD.payload
  OR NEW.payload_hash != OLD.payload_hash
BEGIN
    SELECT RAISE(ABORT, 'Transfer delivery envelope identity is immutable');
END;

CREATE TRIGGER transfer_delivery_receipt_immutable_update
BEFORE UPDATE ON transfer_delivery_receipt
BEGIN
    SELECT RAISE(ABORT, 'Transfer package receipts are immutable');
END;

CREATE TRIGGER transfer_delivery_receipt_immutable_delete
BEFORE DELETE ON transfer_delivery_receipt
BEGIN
    SELECT RAISE(ABORT, 'Transfer package receipts are immutable');
END;

CREATE TRIGGER transfer_remote_policy_event_immutable_update
BEFORE UPDATE ON transfer_remote_policy_event
BEGIN
    SELECT RAISE(ABORT, 'Remote Transfer policy events are immutable');
END;

CREATE TRIGGER transfer_delivery_pull_identity_immutable
BEFORE UPDATE ON transfer_delivery_pull_request
WHEN NEW.reference_uid != OLD.reference_uid
  OR NEW.request_id != OLD.request_id
  OR NEW.after_cursor != OLD.after_cursor
BEGIN SELECT RAISE(ABORT, 'Transfer pull request identity is immutable'); END;

CREATE TRIGGER transfer_remote_policy_event_immutable_delete
BEFORE DELETE ON transfer_remote_policy_event
BEGIN
    SELECT RAISE(ABORT, 'Remote Transfer policy events are immutable');
END;

CREATE TRIGGER transfer_replica_envelope_immutable_update
BEFORE UPDATE ON transfer_replica_envelope
BEGIN
    SELECT RAISE(ABORT, 'Transfer replica envelopes are immutable');
END;

CREATE TRIGGER transfer_replica_envelope_immutable_delete
BEFORE DELETE ON transfer_replica_envelope
BEGIN
    SELECT RAISE(ABORT, 'Transfer replica envelopes are immutable');
END;

CREATE TRIGGER transfer_remote_conflict_immutable_update
BEFORE UPDATE ON transfer_remote_conflict
BEGIN
    SELECT RAISE(ABORT, 'Rejected remote Transfer attempts are immutable');
END;

CREATE TRIGGER transfer_remote_command_identity_immutable
BEFORE UPDATE ON transfer_remote_command
WHEN NEW.request_id != OLD.request_id
  OR NEW.direction != OLD.direction
  OR NEW.origin_organ_uid != OLD.origin_organ_uid
  OR NEW.sender_organ_uid != OLD.sender_organ_uid
  OR NEW.transfer_uid != OLD.transfer_uid
  OR NEW.actor_person_uid != OLD.actor_person_uid
  OR NEW.expected_revision IS NOT OLD.expected_revision
  OR NEW.payload != OLD.payload
  OR NEW.payload_hash != OLD.payload_hash
BEGIN
    SELECT RAISE(ABORT, 'Remote Transfer command identity is immutable');
END;

CREATE TRIGGER fact_remote_command_immutable_update
BEFORE UPDATE ON fact_remote_command
BEGIN SELECT RAISE(ABORT, 'Remote command Fact links are immutable'); END;

CREATE TRIGGER fact_remote_command_immutable_delete
BEFORE DELETE ON fact_remote_command
BEGIN SELECT RAISE(ABORT, 'Remote command Fact links are immutable'); END;

CREATE TRIGGER transfer_delivery_policy_request_collision
BEFORE INSERT ON transfer_delivery_policy_event
WHEN EXISTS (SELECT 1 FROM transfer_revision WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_invitation_event WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_agreement_event WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase4_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase5_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase5_correction_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase6_bulk_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_application_handoff_event WHERE request_id = NEW.request_id)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow');
END;

CREATE TRIGGER transfer_revision_phase8_request_collision
BEFORE INSERT ON transfer_revision
WHEN NEW.idempotency_key IS NOT NULL AND (
     EXISTS (SELECT 1 FROM transfer_delivery_policy_event WHERE request_id = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_application_handoff_event WHERE request_id = NEW.idempotency_key))
BEGIN SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow'); END;

CREATE TRIGGER transfer_invitation_phase8_request_collision
BEFORE INSERT ON transfer_invitation_event
WHEN EXISTS (SELECT 1 FROM transfer_delivery_policy_event WHERE request_id = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_application_handoff_event WHERE request_id = NEW.idempotency_key)
BEGIN SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow'); END;

CREATE TRIGGER transfer_agreement_phase8_request_collision
BEFORE INSERT ON transfer_agreement_event
WHEN EXISTS (SELECT 1 FROM transfer_delivery_policy_event WHERE request_id = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_application_handoff_event WHERE request_id = NEW.idempotency_key)
BEGIN SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow'); END;

CREATE TRIGGER transfer_phase4_phase8_request_collision
BEFORE INSERT ON transfer_phase4_request
WHEN EXISTS (SELECT 1 FROM transfer_delivery_policy_event WHERE request_id = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_application_handoff_event WHERE request_id = NEW.idempotency_key)
BEGIN SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow'); END;

CREATE TRIGGER transfer_phase5_phase8_request_collision
BEFORE INSERT ON transfer_phase5_request
WHEN EXISTS (SELECT 1 FROM transfer_delivery_policy_event WHERE request_id = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_application_handoff_event WHERE request_id = NEW.idempotency_key)
BEGIN SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow'); END;

CREATE TRIGGER transfer_phase5_correction_phase8_request_collision
BEFORE INSERT ON transfer_phase5_correction_request
WHEN EXISTS (SELECT 1 FROM transfer_delivery_policy_event WHERE request_id = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_application_handoff_event WHERE request_id = NEW.idempotency_key)
BEGIN SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow'); END;

CREATE TRIGGER transfer_phase6_phase8_request_collision
BEFORE INSERT ON transfer_phase6_bulk_request
WHEN EXISTS (SELECT 1 FROM transfer_delivery_policy_event WHERE request_id = NEW.idempotency_key)
  OR EXISTS (SELECT 1 FROM transfer_application_handoff_event WHERE request_id = NEW.idempotency_key)
BEGIN SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow'); END;

CREATE TRIGGER transfer_remote_conflict_immutable_delete
BEFORE DELETE ON transfer_remote_conflict
BEGIN
    SELECT RAISE(ABORT, 'Rejected remote Transfer attempts are immutable');
END;

CREATE TRIGGER transfer_application_attestation_immutable_update
BEFORE UPDATE ON transfer_application_attestation
BEGIN
    SELECT RAISE(ABORT, 'Transfer application attestations are immutable');
END;

CREATE TRIGGER transfer_application_attestation_immutable_delete
BEFORE DELETE ON transfer_application_attestation
BEGIN
    SELECT RAISE(ABORT, 'Transfer application attestations are immutable');
END;

CREATE TRIGGER transfer_application_handoff_event_immutable_update
BEFORE UPDATE ON transfer_application_handoff_event
BEGIN
    SELECT RAISE(ABORT, 'Transfer application handoff events are immutable');
END;

CREATE TRIGGER transfer_application_handoff_event_immutable_delete
BEFORE DELETE ON transfer_application_handoff_event
BEGIN
    SELECT RAISE(ABORT, 'Transfer application handoff events are immutable');
END;

CREATE TRIGGER transfer_application_handoff_request_collision
BEFORE INSERT ON transfer_application_handoff_event
WHEN EXISTS (SELECT 1 FROM transfer_revision WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_invitation_event WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_agreement_event WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase4_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase5_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase5_correction_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase6_bulk_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_delivery_policy_event WHERE request_id = NEW.request_id)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow');
END;

CREATE TRIGGER transfer_remote_application_handoff_identity_immutable
BEFORE UPDATE ON transfer_remote_application_handoff
WHEN NEW.uid != OLD.uid
  OR NEW.reference_uid != OLD.reference_uid
  OR NEW.origin_organ_uid != OLD.origin_organ_uid
  OR NEW.participant_organ_uid != OLD.participant_organ_uid
  OR NEW.participant_person_uid != OLD.participant_person_uid
  OR NEW.transfer_uid != OLD.transfer_uid
  OR NEW.occurrence_uid != OLD.occurrence_uid
  OR NEW.source_promise_uid != OLD.source_promise_uid
  OR NEW.settlement_slice_uid != OLD.settlement_slice_uid
  OR NEW.origin_revision != OLD.origin_revision
  OR NEW.canonical_quantity != OLD.canonical_quantity
  OR NEW.canonical_unit_uid IS NOT OLD.canonical_unit_uid
  OR NEW.canonical_cumulative_before != OLD.canonical_cumulative_before
  OR NEW.canonical_cumulative_after != OLD.canonical_cumulative_after
  OR NEW.canonical_remaining_after != OLD.canonical_remaining_after
  OR NEW.canonical_slice_hash != OLD.canonical_slice_hash
  OR NEW.envelope_uid != OLD.envelope_uid
  OR NEW.envelope_payload_hash != OLD.envelope_payload_hash
  OR NEW.origin_created_at != OLD.origin_created_at
BEGIN
    SELECT RAISE(ABORT, 'Remote Transfer application handoff identity is immutable');
END;

CREATE TRIGGER transfer_application_handoff_detail_identity_immutable
BEFORE UPDATE ON transfer_application_handoff_detail
WHEN NEW.handoff_uid != OLD.handoff_uid
  OR NEW.source_promise_uid != OLD.source_promise_uid
  OR NEW.canonical_quantity != OLD.canonical_quantity
  OR NEW.canonical_unit_uid IS NOT OLD.canonical_unit_uid
  OR NEW.canonical_cumulative_before != OLD.canonical_cumulative_before
  OR NEW.canonical_cumulative_after != OLD.canonical_cumulative_after
  OR NEW.canonical_remaining_after != OLD.canonical_remaining_after
  OR NEW.application_direction != OLD.application_direction
  OR NEW.origin_evidence_fact_uid != OLD.origin_evidence_fact_uid
  OR NEW.created_at != OLD.created_at
BEGIN SELECT RAISE(ABORT, 'Transfer application handoff details are immutable'); END;

CREATE TRIGGER transfer_application_handoff_detail_immutable_delete
BEFORE DELETE ON transfer_application_handoff_detail
BEGIN SELECT RAISE(ABORT, 'Transfer application handoff details are immutable'); END;

CREATE TRIGGER transfer_application_attestation_outbox_identity_immutable
BEFORE UPDATE ON transfer_application_attestation_outbox
WHEN NEW.attestation_uid != OLD.attestation_uid
  OR NEW.handoff_uid != OLD.handoff_uid
  OR NEW.reference_uid != OLD.reference_uid
  OR NEW.origin_organ_uid != OLD.origin_organ_uid
  OR NEW.payload != OLD.payload
  OR NEW.created_at != OLD.created_at
BEGIN SELECT RAISE(ABORT, 'Transfer application attestation identity is immutable'); END;

CREATE TRIGGER karma_request_immutable_update
BEFORE UPDATE ON karma_request
BEGIN SELECT RAISE(ABORT, 'Karma request identities are immutable'); END;

CREATE TRIGGER karma_request_immutable_delete
BEFORE DELETE ON karma_request
BEGIN SELECT RAISE(ABORT, 'Karma request identities are immutable'); END;

CREATE TRIGGER karma_program_revision_immutable_update
BEFORE UPDATE ON karma_program_revision
BEGIN SELECT RAISE(ABORT, 'Karma Program revisions are immutable'); END;

CREATE TRIGGER karma_program_revision_immutable_delete
BEFORE DELETE ON karma_program_revision
BEGIN SELECT RAISE(ABORT, 'Karma Program revisions are immutable'); END;

CREATE TRIGGER karma_program_request_immutable_update
BEFORE UPDATE ON karma_program_request
BEGIN SELECT RAISE(ABORT, 'Karma Program requests are immutable'); END;

CREATE TRIGGER karma_program_request_immutable_delete
BEFORE DELETE ON karma_program_request
BEGIN SELECT RAISE(ABORT, 'Karma Program requests are immutable'); END;

CREATE TRIGGER karma_program_revision_scope_insert
BEFORE INSERT ON karma_program
WHEN NOT EXISTS (
    SELECT 1 FROM karma_program_revision revision
    WHERE revision.revision_hash = NEW.head_revision_hash
      AND revision.program_uid = NEW.record_uid
) OR NOT EXISTS (
    SELECT 1 FROM record WHERE uid = NEW.record_uid AND kind = 'program'
) OR (
    NEW.active_revision_hash IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM karma_program_revision revision
        WHERE revision.revision_hash = NEW.active_revision_hash
          AND revision.program_uid = NEW.record_uid
          AND revision.proof_status = 'accepted'
    )
)
BEGIN SELECT RAISE(ABORT, 'Karma Program head revision belongs to another Program'); END;

CREATE TRIGGER karma_program_revision_scope_update
BEFORE UPDATE OF head_revision_hash, active_revision_hash ON karma_program
WHEN NOT EXISTS (
    SELECT 1 FROM karma_program_revision revision
    WHERE revision.revision_hash = NEW.head_revision_hash
      AND revision.program_uid = NEW.record_uid
) OR (
    NEW.active_revision_hash IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM karma_program_revision revision
        WHERE revision.revision_hash = NEW.active_revision_hash
          AND revision.program_uid = NEW.record_uid
          AND revision.proof_status = 'accepted'
    )
)
BEGIN SELECT RAISE(ABORT, 'Karma Program revision scope or Proof is invalid'); END;

CREATE TRIGGER karma_frequency_revision_immutable_update
BEFORE UPDATE ON karma_frequency_revision
BEGIN SELECT RAISE(ABORT, 'Karma Frequency revisions are immutable'); END;

CREATE TRIGGER karma_frequency_revision_immutable_delete
BEFORE DELETE ON karma_frequency_revision
BEGIN SELECT RAISE(ABORT, 'Karma Frequency revisions are immutable'); END;

CREATE TRIGGER karma_frequency_activation_immutable_update
BEFORE UPDATE ON karma_frequency_activation
BEGIN SELECT RAISE(ABORT, 'Karma Frequency activations are immutable'); END;

CREATE TRIGGER karma_frequency_activation_immutable_delete
BEFORE DELETE ON karma_frequency_activation
BEGIN SELECT RAISE(ABORT, 'Karma Frequency activations are immutable'); END;

CREATE TRIGGER karma_frequency_request_immutable_update
BEFORE UPDATE ON karma_frequency_request
BEGIN SELECT RAISE(ABORT, 'Karma Frequency requests are immutable'); END;

CREATE TRIGGER karma_frequency_request_immutable_delete
BEFORE DELETE ON karma_frequency_request
BEGIN SELECT RAISE(ABORT, 'Karma Frequency requests are immutable'); END;

CREATE TRIGGER karma_frequency_revision_scope_insert
BEFORE INSERT ON karma_frequency
WHEN NOT EXISTS (
    SELECT 1 FROM karma_frequency_revision revision
    WHERE revision.revision_hash = NEW.head_revision_hash
      AND revision.frequency_uid = NEW.record_uid
) OR NOT EXISTS (
    SELECT 1 FROM record WHERE uid = NEW.record_uid AND kind = 'frequency'
)
BEGIN SELECT RAISE(ABORT, 'Karma Frequency head revision scope is invalid'); END;

CREATE TRIGGER karma_frequency_activation_scope_insert
BEFORE INSERT ON karma_frequency_activation
WHEN NOT EXISTS (
    SELECT 1 FROM karma_frequency_revision revision
    WHERE revision.revision_hash = NEW.definition_revision_hash
      AND revision.frequency_uid = NEW.frequency_uid
) OR (
    NEW.previous_activation_hash IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM karma_frequency_activation activation
        WHERE activation.activation_hash = NEW.previous_activation_hash
          AND activation.frequency_uid = NEW.frequency_uid
    )
)
BEGIN SELECT RAISE(ABORT, 'Karma Frequency activation scope is invalid'); END;

CREATE TRIGGER karma_frequency_revision_scope_update
BEFORE UPDATE OF head_revision_hash, active_revision_hash, active_activation_hash, latest_activation_hash
ON karma_frequency
WHEN NOT EXISTS (
    SELECT 1 FROM karma_frequency_revision revision
    WHERE revision.revision_hash = NEW.head_revision_hash
      AND revision.frequency_uid = NEW.record_uid
) OR (
    NEW.active_revision_hash IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM karma_frequency_activation activation
        WHERE activation.activation_hash = NEW.active_activation_hash
          AND activation.frequency_uid = NEW.record_uid
          AND activation.definition_revision_hash = NEW.active_revision_hash
    )
) OR (
    NEW.latest_activation_hash IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM karma_frequency_activation activation
        WHERE activation.activation_hash = NEW.latest_activation_hash
          AND activation.frequency_uid = NEW.record_uid
    )
)
BEGIN SELECT RAISE(ABORT, 'Karma Frequency revision or activation scope is invalid'); END;

CREATE TRIGGER karma_schedule_occurrence_immutable_update
BEFORE UPDATE ON karma_schedule_occurrence
BEGIN SELECT RAISE(ABORT, 'Karma schedule occurrences are immutable'); END;

CREATE TRIGGER karma_schedule_occurrence_immutable_delete
BEFORE DELETE ON karma_schedule_occurrence
BEGIN SELECT RAISE(ABORT, 'Karma schedule occurrences are immutable'); END;

CREATE TRIGGER karma_occurrence_immutable_update
BEFORE UPDATE ON karma_occurrence
BEGIN SELECT RAISE(ABORT, 'Karma occurrences are immutable'); END;

CREATE TRIGGER karma_occurrence_immutable_delete
BEFORE DELETE ON karma_occurrence
BEGIN SELECT RAISE(ABORT, 'Karma occurrences are immutable'); END;

CREATE TRIGGER karma_occurrence_program_epoch_frozen
BEFORE UPDATE OF occurrence_hash, cell_sequence, epoch_hash, epoch_json, member_count
ON karma_occurrence_program_epoch
BEGIN SELECT RAISE(ABORT, 'Karma Program epoch identity is immutable'); END;

CREATE TRIGGER karma_occurrence_program_epoch_no_delete
BEFORE DELETE ON karma_occurrence_program_epoch
BEGIN SELECT RAISE(ABORT, 'Karma Program epochs are immutable'); END;

CREATE TRIGGER karma_run_immutable_update
BEFORE UPDATE ON karma_run
BEGIN SELECT RAISE(ABORT, 'Karma runs are immutable'); END;

CREATE TRIGGER karma_run_immutable_delete
BEFORE DELETE ON karma_run
BEGIN SELECT RAISE(ABORT, 'Karma runs are immutable'); END;

CREATE TRIGGER karma_program_state_event_immutable_update
BEFORE UPDATE ON karma_program_state_event
BEGIN SELECT RAISE(ABORT, 'Karma Program state events are immutable'); END;

CREATE TRIGGER karma_program_state_event_immutable_delete
BEFORE DELETE ON karma_program_state_event
BEGIN SELECT RAISE(ABORT, 'Karma Program state events are immutable'); END;

CREATE TRIGGER karma_candidate_immutable_update
BEFORE UPDATE ON karma_candidate
BEGIN SELECT RAISE(ABORT, 'Karma candidate proposals are immutable in K4.2'); END;

CREATE TRIGGER karma_candidate_immutable_delete
BEFORE DELETE ON karma_candidate
BEGIN SELECT RAISE(ABORT, 'Karma candidate proposals are immutable in K4.2'); END;

CREATE TRIGGER karma_candidate_review_event_immutable_update
BEFORE UPDATE ON karma_candidate_review_event
BEGIN SELECT RAISE(ABORT, 'Karma candidate review events are immutable'); END;

CREATE TRIGGER karma_candidate_review_event_immutable_delete
BEFORE DELETE ON karma_candidate_review_event
BEGIN SELECT RAISE(ABORT, 'Karma candidate review events are immutable'); END;

CREATE TRIGGER karma_candidate_review_request_immutable_update
BEFORE UPDATE ON karma_candidate_review_request
BEGIN SELECT RAISE(ABORT, 'Karma candidate review requests are immutable'); END;

CREATE TRIGGER karma_candidate_review_request_immutable_delete
BEFORE DELETE ON karma_candidate_review_request
BEGIN SELECT RAISE(ABORT, 'Karma candidate review requests are immutable'); END;

CREATE TRIGGER karma_grant_revision_immutable_update
BEFORE UPDATE ON karma_grant_revision
BEGIN SELECT RAISE(ABORT, 'Karma Grant revisions are immutable'); END;

CREATE TRIGGER karma_grant_revision_immutable_delete
BEFORE DELETE ON karma_grant_revision
BEGIN SELECT RAISE(ABORT, 'Karma Grant revisions are immutable'); END;

CREATE TRIGGER karma_grant_request_immutable_update
BEFORE UPDATE ON karma_grant_request
BEGIN SELECT RAISE(ABORT, 'Karma Grant requests are immutable'); END;

CREATE TRIGGER karma_grant_request_immutable_delete
BEFORE DELETE ON karma_grant_request
BEGIN SELECT RAISE(ABORT, 'Karma Grant requests are immutable'); END;

CREATE TRIGGER karma_grant_scope_insert
BEFORE INSERT ON karma_grant
WHEN NOT EXISTS (
    SELECT 1 FROM karma_grant_revision revision
    WHERE revision.revision_hash = NEW.head_revision_hash
      AND revision.grant_uid = NEW.record_uid
      AND revision.principal_person_uid = NEW.principal_person_uid
) OR NOT EXISTS (
    SELECT 1 FROM record WHERE uid = NEW.record_uid AND kind = 'grant'
)
BEGIN SELECT RAISE(ABORT, 'Karma Grant revision scope is invalid'); END;

CREATE TRIGGER karma_grant_scope_update
BEFORE UPDATE OF head_revision_hash, active_revision_hash, principal_person_uid ON karma_grant
WHEN NOT EXISTS (
    SELECT 1 FROM karma_grant_revision revision
    WHERE revision.revision_hash = NEW.head_revision_hash
      AND revision.grant_uid = NEW.record_uid
      AND revision.principal_person_uid = NEW.principal_person_uid
) OR (
    NEW.active_revision_hash IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM karma_grant_revision revision
        WHERE revision.revision_hash = NEW.active_revision_hash
          AND revision.grant_uid = NEW.record_uid
          AND revision.principal_person_uid = NEW.principal_person_uid
    )
)
BEGIN SELECT RAISE(ABORT, 'Karma Grant active revision scope is invalid'); END;

CREATE TRIGGER karma_intent_immutable_update
BEFORE UPDATE ON karma_intent
BEGIN SELECT RAISE(ABORT, 'Karma intents are immutable'); END;

CREATE TRIGGER karma_intent_immutable_delete
BEFORE DELETE ON karma_intent
BEGIN SELECT RAISE(ABORT, 'Karma intents are immutable'); END;

CREATE TRIGGER karma_intent_requires_accepted_act_candidate
BEFORE INSERT ON karma_intent
WHEN NOT EXISTS (
    SELECT 1 FROM karma_candidate candidate
    JOIN karma_candidate_state state ON state.candidate_hash = candidate.candidate_hash
    WHERE candidate.candidate_hash = NEW.candidate_hash
      AND candidate.route = 'act'
      AND state.status = 'accepted'
      AND candidate.program_uid = NEW.program_uid
      AND candidate.program_revision_hash = NEW.program_revision_hash
)
BEGIN SELECT RAISE(ABORT, 'a Karma intent requires an accepted act candidate'); END;

CREATE TRIGGER karma_intent_requires_active_grant
BEFORE INSERT ON karma_intent
WHEN NOT EXISTS (
    SELECT 1 FROM karma_grant grant_row
    WHERE grant_row.record_uid = NEW.grant_uid
      AND grant_row.status = 'active'
      AND grant_row.active_revision_hash = NEW.grant_revision_hash
      AND grant_row.handle_revision = NEW.grant_handle_revision
)
BEGIN SELECT RAISE(ABORT, 'a Karma intent requires its authorizing grant to be active'); END;

CREATE TRIGGER karma_intent_state_starts_authorized
BEFORE INSERT ON karma_intent_state
WHEN NEW.status <> 'authorized' OR NEW.state_revision <> 1
BEGIN SELECT RAISE(ABORT, 'a Karma intent begins authorized at revision 1'); END;

CREATE TRIGGER karma_intent_state_is_one_way
BEFORE UPDATE ON karma_intent_state
WHEN OLD.status <> 'authorized'
  OR NEW.status <> 'cancelled'
  OR NEW.state_revision <> OLD.state_revision + 1
BEGIN SELECT RAISE(ABORT, 'a Karma intent may only move from authorized to cancelled'); END;

CREATE TRIGGER karma_intent_state_immutable_delete
BEFORE DELETE ON karma_intent_state
BEGIN SELECT RAISE(ABORT, 'Karma intent state is append-only'); END;

CREATE TRIGGER karma_intent_state_matches_event_insert
BEFORE INSERT ON karma_intent_state
WHEN NOT EXISTS (
    SELECT 1 FROM karma_intent_event event
    WHERE event.event_hash = NEW.current_event_hash
      AND event.intent_hash = NEW.intent_hash
      AND event.state_revision = NEW.state_revision
      AND event.status = NEW.status
)
BEGIN SELECT RAISE(ABORT, 'Karma intent state must match its current event'); END;

CREATE TRIGGER karma_intent_state_matches_event_update
BEFORE UPDATE ON karma_intent_state
WHEN NOT EXISTS (
    SELECT 1 FROM karma_intent_event event
    WHERE event.event_hash = NEW.current_event_hash
      AND event.intent_hash = NEW.intent_hash
      AND event.state_revision = NEW.state_revision
      AND event.status = NEW.status
      AND event.previous_event_hash = OLD.current_event_hash
)
BEGIN SELECT RAISE(ABORT, 'Karma intent state must match its current event'); END;

CREATE TRIGGER karma_intent_event_immutable_update
BEFORE UPDATE ON karma_intent_event
BEGIN SELECT RAISE(ABORT, 'Karma intent events are immutable'); END;

CREATE TRIGGER karma_intent_event_immutable_delete
BEFORE DELETE ON karma_intent_event
BEGIN SELECT RAISE(ABORT, 'Karma intent events are immutable'); END;

CREATE TRIGGER karma_intent_status_frozen_update
BEFORE UPDATE ON karma_intent_status
BEGIN SELECT RAISE(ABORT, 'Karma intent statuses are frozen'); END;

CREATE TRIGGER karma_intent_status_frozen_delete
BEFORE DELETE ON karma_intent_status
BEGIN SELECT RAISE(ABORT, 'Karma intent statuses are frozen'); END;

CREATE TRIGGER record_origin_required_insert
BEFORE INSERT ON record
WHEN NEW.organ_uid IS NULL OR NEW.organ_uid = ''
BEGIN
    SELECT RAISE(ABORT, 'record.organ_uid is required: every Record has an origin Organ');
END;

CREATE TRIGGER record_origin_required_update
BEFORE UPDATE OF organ_uid ON record
WHEN NEW.organ_uid IS NULL OR NEW.organ_uid = ''
BEGIN
    SELECT RAISE(ABORT, 'record.organ_uid is required: every Record has an origin Organ');
END;

CREATE TRIGGER sync_op_requires_write_capability
BEFORE INSERT ON sync_op
WHEN NEW.actor_cell = (SELECT uid FROM record WHERE slug = 'local-cell' LIMIT 1)
 AND EXISTS (
       SELECT 1 FROM organ_roster
        WHERE organ_uid = (SELECT uid FROM record WHERE slug = 'local-organ' LIMIT 1)
     )
 AND NOT EXISTS (SELECT 1 FROM local_capability WHERE capability = 'write')
BEGIN
    SELECT RAISE(
        ABORT,
        'this Cell has no write capability in its Organ: a relay carries traffic and authors nothing'
    );
END;

CREATE TRIGGER record_revision_record_insert
AFTER INSERT ON record
BEGIN
    INSERT INTO record_revision (record_uid, revision) VALUES (NEW.uid, 1);
END;

CREATE TRIGGER record_revision_record_uid_immutable
BEFORE UPDATE OF uid ON record
WHEN NEW.uid <> OLD.uid
BEGIN
    SELECT RAISE(ABORT, 'Record identity is immutable');
END;

CREATE TRIGGER record_revision_record_update
AFTER UPDATE ON record
BEGIN
    SELECT RAISE(ABORT, 'Record revision is missing or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM record_revision
        WHERE record_uid = NEW.uid AND revision > 0 AND revision < 9223372036854775807
    );
    UPDATE record_revision SET revision = revision + 1
    WHERE record_uid = NEW.uid;
END;

CREATE TRIGGER record_revision_assertion_insert
AFTER INSERT ON record_assertion
BEGIN
    SELECT RAISE(ABORT, 'Record revision is missing or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM record_revision
        WHERE record_uid = NEW.subject_uid AND revision > 0 AND revision < 9223372036854775807
    );
    UPDATE record_revision SET revision = revision + 1
    WHERE record_uid = NEW.subject_uid;
END;

CREATE TRIGGER record_revision_assertion_update
AFTER UPDATE ON record_assertion
BEGIN
    SELECT RAISE(ABORT, 'Record revision is missing or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM record_revision
        WHERE record_uid = OLD.subject_uid AND revision > 0 AND revision < 9223372036854775807
    );
    SELECT RAISE(ABORT, 'Record revision is missing or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM record_revision
        WHERE record_uid = NEW.subject_uid AND revision > 0 AND revision < 9223372036854775807
    );
    UPDATE record_revision SET revision = revision + 1
    WHERE record_uid IN (OLD.subject_uid, NEW.subject_uid);
END;

CREATE TRIGGER record_revision_assertion_delete
AFTER DELETE ON record_assertion
BEGIN
    SELECT RAISE(ABORT, 'Record revision is missing or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM record_revision
        WHERE record_uid = OLD.subject_uid AND revision > 0 AND revision < 9223372036854775807
    );
    UPDATE record_revision SET revision = revision + 1
    WHERE record_uid = OLD.subject_uid;
END;

CREATE TRIGGER record_revision_extension_insert
AFTER INSERT ON record_extension
BEGIN
    SELECT RAISE(ABORT, 'Record revision is missing or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM record_revision
        WHERE record_uid = NEW.record_uid AND revision > 0 AND revision < 9223372036854775807
    );
    UPDATE record_revision SET revision = revision + 1
    WHERE record_uid = NEW.record_uid;
END;

CREATE TRIGGER record_revision_extension_update
AFTER UPDATE ON record_extension
BEGIN
    SELECT RAISE(ABORT, 'Record revision is missing or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM record_revision
        WHERE record_uid = OLD.record_uid AND revision > 0 AND revision < 9223372036854775807
    );
    SELECT RAISE(ABORT, 'Record revision is missing or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM record_revision
        WHERE record_uid = NEW.record_uid AND revision > 0 AND revision < 9223372036854775807
    );
    UPDATE record_revision SET revision = revision + 1
    WHERE record_uid IN (OLD.record_uid, NEW.record_uid);
END;

CREATE TRIGGER record_revision_extension_delete
AFTER DELETE ON record_extension
BEGIN
    SELECT RAISE(ABORT, 'Record revision is missing or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM record_revision
        WHERE record_uid = OLD.record_uid AND revision > 0 AND revision < 9223372036854775807
    );
    UPDATE record_revision SET revision = revision + 1
    WHERE record_uid = OLD.record_uid;
END;

CREATE TRIGGER operation_receipt_immutable_update
BEFORE UPDATE ON operation_receipt
BEGIN
    SELECT RAISE(ABORT, 'Operation receipt history is immutable');
END;

CREATE TRIGGER operation_receipt_immutable_delete
BEFORE DELETE ON operation_receipt
BEGIN
    SELECT RAISE(ABORT, 'Operation receipt history is immutable');
END;

CREATE TRIGGER operation_receipt_record_immutable_update
BEFORE UPDATE ON operation_receipt_record
BEGIN
    SELECT RAISE(ABORT, 'Operation receipt history is immutable');
END;

CREATE TRIGGER operation_receipt_record_immutable_delete
BEFORE DELETE ON operation_receipt_record
BEGIN
    SELECT RAISE(ABORT, 'Operation receipt history is immutable');
END;

CREATE TRIGGER person_auth_generation_immutable_delete
BEFORE DELETE ON person_auth_generation
BEGIN
    SELECT RAISE(ABORT, 'Authentication generation tombstones are permanent');
END;

CREATE TRIGGER person_auth_generation_monotonic_update
BEFORE UPDATE ON person_auth_generation
WHEN NEW.person_uid IS NOT OLD.person_uid
    OR typeof(OLD.generation) <> 'integer' OR OLD.generation <= 0
    OR typeof(NEW.generation) <> 'integer' OR NEW.generation <= OLD.generation
BEGIN
    SELECT RAISE(ABORT, 'Authentication generation must increase without changing identity');
END;

CREATE TRIGGER organ_login_generation_immutable_delete
BEFORE DELETE ON organ_login_generation
BEGIN
    SELECT RAISE(ABORT, 'Authentication generation tombstones are permanent');
END;

CREATE TRIGGER organ_login_generation_monotonic_update
BEFORE UPDATE ON organ_login_generation
WHEN NEW.organ_uid IS NOT OLD.organ_uid
    OR typeof(OLD.generation) <> 'integer' OR OLD.generation <= 0
    OR typeof(NEW.generation) <> 'integer' OR NEW.generation <= OLD.generation
BEGIN
    SELECT RAISE(ABORT, 'Authentication generation must increase without changing identity');
END;

CREATE TRIGGER person_device_immutable_delete
BEFORE DELETE ON person_device
BEGIN
    SELECT RAISE(ABORT, 'Device tombstones are permanent');
END;

CREATE TRIGGER person_device_monotonic_update
BEFORE UPDATE ON person_device
WHEN NEW.person_uid IS NOT OLD.person_uid OR NEW.node_id IS NOT OLD.node_id
    OR typeof(OLD.revision) <> 'integer' OR OLD.revision <= 0
    OR typeof(NEW.revision) <> 'integer' OR NEW.revision <= OLD.revision
    OR typeof(OLD.revoked) <> 'integer' OR OLD.revoked NOT IN (0, 1)
    OR typeof(NEW.revoked) <> 'integer' OR NEW.revoked NOT IN (0, 1)
BEGIN
    SELECT RAISE(ABORT, 'Device revision must increase without changing identity');
END;

CREATE TRIGGER session_credential_insert
AFTER INSERT ON person_credential
BEGIN
    INSERT INTO person_auth_generation (person_uid, generation)
    SELECT uid, 1 FROM (
        SELECT NEW.person_uid AS uid WHERE EXISTS (
            SELECT 1 FROM record WHERE uid = NEW.person_uid AND kind = 'person'
        )
    ) WHERE true
    ON CONFLICT(person_uid) DO UPDATE SET generation = person_auth_generation.generation + 1;
END;

CREATE TRIGGER session_standing_insert
AFTER INSERT ON record_extension
BEGIN
    INSERT INTO person_auth_generation (person_uid, generation)
    SELECT uid, 1 FROM (
        SELECT CAST(NEW.record_uid AS TEXT) AS uid
        WHERE CAST(NEW.namespace AS TEXT) = 'lince.person' AND EXISTS (
            SELECT 1 FROM record WHERE uid = CAST(NEW.record_uid AS TEXT) AND kind = 'person'
        )
    ) WHERE true
    ON CONFLICT(person_uid) DO UPDATE SET generation = person_auth_generation.generation + 1;
END;

CREATE TRIGGER session_organ_login_insert
AFTER INSERT ON organ_login
BEGIN
    INSERT INTO organ_login_generation (organ_uid, generation)
    SELECT uid, 1 FROM (
        SELECT CAST(NEW.organ_uid AS TEXT) AS uid
    ) WHERE true
    ON CONFLICT(organ_uid) DO UPDATE SET generation = organ_login_generation.generation + 1;
END;

CREATE TRIGGER session_contact_insert
AFTER INSERT ON organ_contact
BEGIN
    INSERT INTO organ_login_generation (organ_uid, generation)
    SELECT uid, 1 FROM (
        SELECT CAST(NEW.record_uid AS TEXT) AS uid
    ) WHERE true
    ON CONFLICT(organ_uid) DO UPDATE SET generation = organ_login_generation.generation + 1;
    UPDATE person_device SET revision = revision + 1
    WHERE node_id IN (
        SELECT lower(trim(CAST(NEW.node_id AS TEXT)))
    );
END;

CREATE TRIGGER session_person_insert
AFTER INSERT ON record
BEGIN
    INSERT INTO person_auth_generation (person_uid, generation)
    SELECT uid, 1 FROM (
        SELECT NEW.uid AS uid WHERE NEW.kind = 'person'
    ) WHERE true
    ON CONFLICT(person_uid) DO UPDATE SET generation = person_auth_generation.generation + 1;
END;

CREATE TRIGGER session_organ_insert
AFTER INSERT ON record
BEGIN
    INSERT INTO organ_login_generation (organ_uid, generation)
    SELECT uid, 1 FROM (
        SELECT NEW.uid AS uid WHERE NEW.kind = 'organ'
    ) WHERE true
    ON CONFLICT(organ_uid) DO UPDATE SET generation = organ_login_generation.generation + 1;
    UPDATE person_device SET revision = revision + 1
    WHERE node_id IN (
        SELECT lower(trim(CAST(node_id AS TEXT))) FROM organ_contact
        WHERE CAST(record_uid AS TEXT) IN (
            SELECT NEW.uid WHERE NEW.kind = 'organ'
        )
    );
END;

CREATE TRIGGER session_credential_update
AFTER UPDATE ON person_credential
BEGIN
    INSERT INTO person_auth_generation (person_uid, generation)
    SELECT uid, 1 FROM (
        SELECT OLD.person_uid AS uid WHERE EXISTS (
            SELECT 1 FROM record WHERE uid = OLD.person_uid AND kind = 'person'
        )
        UNION
        SELECT NEW.person_uid AS uid WHERE EXISTS (
            SELECT 1 FROM record WHERE uid = NEW.person_uid AND kind = 'person'
        )
    ) WHERE true
    ON CONFLICT(person_uid) DO UPDATE SET generation = person_auth_generation.generation + 1;
END;

CREATE TRIGGER session_standing_update
AFTER UPDATE ON record_extension
BEGIN
    INSERT INTO person_auth_generation (person_uid, generation)
    SELECT uid, 1 FROM (
        SELECT CAST(OLD.record_uid AS TEXT) AS uid
        WHERE CAST(OLD.namespace AS TEXT) = 'lince.person' AND EXISTS (
            SELECT 1 FROM record WHERE uid = CAST(OLD.record_uid AS TEXT) AND kind = 'person'
        )
        UNION
        SELECT CAST(NEW.record_uid AS TEXT) AS uid
        WHERE CAST(NEW.namespace AS TEXT) = 'lince.person' AND EXISTS (
            SELECT 1 FROM record WHERE uid = CAST(NEW.record_uid AS TEXT) AND kind = 'person'
        )
    ) WHERE true
    ON CONFLICT(person_uid) DO UPDATE SET generation = person_auth_generation.generation + 1;
END;

CREATE TRIGGER session_organ_login_update
AFTER UPDATE ON organ_login
BEGIN
    INSERT INTO organ_login_generation (organ_uid, generation)
    SELECT uid, 1 FROM (
        SELECT CAST(OLD.organ_uid AS TEXT) AS uid
        UNION
        SELECT CAST(NEW.organ_uid AS TEXT) AS uid
    ) WHERE true
    ON CONFLICT(organ_uid) DO UPDATE SET generation = organ_login_generation.generation + 1;
END;

CREATE TRIGGER session_contact_update
AFTER UPDATE ON organ_contact
WHEN NEW.record_uid IS NOT OLD.record_uid OR NEW.node_id IS NOT OLD.node_id OR NEW.trust IS NOT OLD.trust
BEGIN
    INSERT INTO organ_login_generation (organ_uid, generation)
    SELECT uid, 1 FROM (
        SELECT CAST(OLD.record_uid AS TEXT) AS uid
        UNION
        SELECT CAST(NEW.record_uid AS TEXT) AS uid
    ) WHERE true
    ON CONFLICT(organ_uid) DO UPDATE SET generation = organ_login_generation.generation + 1;
    UPDATE person_device SET revision = revision + 1
    WHERE node_id IN (
        SELECT lower(trim(CAST(OLD.node_id AS TEXT)))
        UNION
        SELECT lower(trim(CAST(NEW.node_id AS TEXT)))
    );
END;

CREATE TRIGGER session_person_update
BEFORE UPDATE ON record
WHEN NEW.uid IS NOT OLD.uid OR NEW.kind IS NOT OLD.kind OR NEW.deleted_at IS NOT OLD.deleted_at
BEGIN
    INSERT INTO person_auth_generation (person_uid, generation)
    SELECT uid, 1 FROM (
        SELECT OLD.uid AS uid WHERE OLD.kind = 'person'
        UNION
        SELECT NEW.uid AS uid WHERE NEW.kind = 'person'
    ) WHERE true
    ON CONFLICT(person_uid) DO UPDATE SET generation = person_auth_generation.generation + 1;
END;

CREATE TRIGGER session_organ_update
BEFORE UPDATE ON record
WHEN NEW.uid IS NOT OLD.uid OR NEW.kind IS NOT OLD.kind OR NEW.deleted_at IS NOT OLD.deleted_at
BEGIN
    INSERT INTO organ_login_generation (organ_uid, generation)
    SELECT uid, 1 FROM (
        SELECT OLD.uid AS uid WHERE OLD.kind = 'organ'
        UNION
        SELECT NEW.uid AS uid WHERE NEW.kind = 'organ'
    ) WHERE true
    ON CONFLICT(organ_uid) DO UPDATE SET generation = organ_login_generation.generation + 1;
    UPDATE person_device SET revision = revision + 1
    WHERE node_id IN (
        SELECT lower(trim(CAST(node_id AS TEXT))) FROM organ_contact
        WHERE CAST(record_uid AS TEXT) IN (
            SELECT OLD.uid WHERE OLD.kind = 'organ'
            UNION
            SELECT NEW.uid WHERE NEW.kind = 'organ'
        )
    );
END;

CREATE TRIGGER session_credential_delete
AFTER DELETE ON person_credential
BEGIN
    INSERT INTO person_auth_generation (person_uid, generation)
    SELECT uid, 1 FROM (
        SELECT OLD.person_uid AS uid WHERE EXISTS (
            SELECT 1 FROM record WHERE uid = OLD.person_uid AND kind = 'person'
        )
    ) WHERE true
    ON CONFLICT(person_uid) DO UPDATE SET generation = person_auth_generation.generation + 1;
END;

CREATE TRIGGER session_standing_delete
AFTER DELETE ON record_extension
BEGIN
    INSERT INTO person_auth_generation (person_uid, generation)
    SELECT uid, 1 FROM (
        SELECT CAST(OLD.record_uid AS TEXT) AS uid
        WHERE CAST(OLD.namespace AS TEXT) = 'lince.person' AND EXISTS (
            SELECT 1 FROM record WHERE uid = CAST(OLD.record_uid AS TEXT) AND kind = 'person'
        )
    ) WHERE true
    ON CONFLICT(person_uid) DO UPDATE SET generation = person_auth_generation.generation + 1;
END;

CREATE TRIGGER session_organ_login_delete
AFTER DELETE ON organ_login
BEGIN
    INSERT INTO organ_login_generation (organ_uid, generation)
    SELECT uid, 1 FROM (
        SELECT CAST(OLD.organ_uid AS TEXT) AS uid
    ) WHERE true
    ON CONFLICT(organ_uid) DO UPDATE SET generation = organ_login_generation.generation + 1;
END;

CREATE TRIGGER session_contact_delete
AFTER DELETE ON organ_contact
BEGIN
    INSERT INTO organ_login_generation (organ_uid, generation)
    SELECT uid, 1 FROM (
        SELECT CAST(OLD.record_uid AS TEXT) AS uid
    ) WHERE true
    ON CONFLICT(organ_uid) DO UPDATE SET generation = organ_login_generation.generation + 1;
    UPDATE person_device SET revision = revision + 1
    WHERE node_id IN (
        SELECT lower(trim(CAST(OLD.node_id AS TEXT)))
    );
END;

CREATE TRIGGER session_person_delete
BEFORE DELETE ON record
BEGIN
    INSERT INTO person_auth_generation (person_uid, generation)
    SELECT uid, 1 FROM (
        SELECT OLD.uid AS uid WHERE OLD.kind = 'person'
    ) WHERE true
    ON CONFLICT(person_uid) DO UPDATE SET generation = person_auth_generation.generation + 1;
END;

CREATE TRIGGER session_organ_delete
BEFORE DELETE ON record
BEGIN
    INSERT INTO organ_login_generation (organ_uid, generation)
    SELECT uid, 1 FROM (
        SELECT OLD.uid AS uid WHERE OLD.kind = 'organ'
    ) WHERE true
    ON CONFLICT(organ_uid) DO UPDATE SET generation = organ_login_generation.generation + 1;
    UPDATE person_device SET revision = revision + 1
    WHERE node_id IN (
        SELECT lower(trim(CAST(node_id AS TEXT))) FROM organ_contact
        WHERE CAST(record_uid AS TEXT) IN (
            SELECT OLD.uid WHERE OLD.kind = 'organ'
        )
    );
END;

CREATE TRIGGER role_permission_revision_retained_insert
BEFORE INSERT ON role_permission_revision
WHEN EXISTS (SELECT 1 FROM role_permission_revision WHERE role_id = NEW.role_id)
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision identity is already retained');
END;

CREATE TRIGGER role_permission_revision_retained_delete
BEFORE DELETE ON role_permission_revision
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision tombstones are permanent');
END;

CREATE TRIGGER role_permission_revision_monotonic_update
BEFORE UPDATE ON role_permission_revision
WHEN NEW.role_id IS NOT OLD.role_id
    OR typeof(OLD.role_id) <> 'integer' OR OLD.role_id <= 0
    OR typeof(NEW.role_id) <> 'integer' OR NEW.role_id <= 0
    OR typeof(OLD.revision) <> 'integer' OR OLD.revision <= 0
    OR typeof(NEW.revision) <> 'integer' OR NEW.revision <= OLD.revision
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision must increase without changing identity');
END;

CREATE TRIGGER role_permission_revision_role_insert
AFTER INSERT ON role
WHEN NEW.id > 0
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision is corrupt or exhausted')
    WHERE EXISTS (
        SELECT 1 FROM role_permission_revision WHERE role_id = NEW.id
            AND (typeof(role_id) <> 'integer' OR role_id <= 0
                OR typeof(revision) <> 'integer' OR revision <= 0
                OR revision = 9223372036854775807)
    );
    UPDATE role_permission_revision SET revision = revision + 1 WHERE role_id = NEW.id;
    INSERT INTO role_permission_revision (role_id, revision)
    SELECT NEW.id, 1
    WHERE NOT EXISTS (SELECT 1 FROM role_permission_revision WHERE role_id = NEW.id);
END;

CREATE TRIGGER role_permission_revision_role_delete
BEFORE DELETE ON role
WHEN OLD.id > 0
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision is missing, corrupt or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM role_permission_revision WHERE role_id = OLD.id
            AND typeof(role_id) = 'integer' AND role_id > 0
            AND typeof(revision) = 'integer' AND revision > 0
            AND revision < 9223372036854775807
    );
    UPDATE role_permission_revision SET revision = revision + 1 WHERE role_id = OLD.id;
END;

CREATE TRIGGER role_permission_revision_role_move
AFTER UPDATE OF id ON role
WHEN NEW.id IS NOT OLD.id
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision is missing, corrupt or exhausted')
    WHERE OLD.id > 0 AND NOT EXISTS (
        SELECT 1 FROM role_permission_revision WHERE role_id = OLD.id
            AND typeof(role_id) = 'integer' AND role_id > 0
            AND typeof(revision) = 'integer' AND revision > 0
            AND revision < 9223372036854775807
    );
    SELECT RAISE(ABORT, 'Role permission revision is corrupt or exhausted')
    WHERE NEW.id > 0 AND EXISTS (
        SELECT 1 FROM role_permission_revision WHERE role_id = NEW.id
            AND (typeof(role_id) <> 'integer' OR role_id <= 0
                OR typeof(revision) <> 'integer' OR revision <= 0
                OR revision = 9223372036854775807)
    );
    UPDATE role_permission_revision SET revision = revision + 1
    WHERE role_id IN (OLD.id, NEW.id) AND role_id > 0;
    INSERT INTO role_permission_revision (role_id, revision)
    SELECT NEW.id, 1 WHERE NEW.id > 0
        AND NOT EXISTS (SELECT 1 FROM role_permission_revision WHERE role_id = NEW.id);
END;

CREATE TRIGGER role_permission_revision_member_insert
AFTER INSERT ON role_permission
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision is missing, corrupt or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM role_permission_revision WHERE role_id = NEW.role_id
            AND typeof(role_id) = 'integer' AND role_id > 0
            AND typeof(revision) = 'integer' AND revision > 0
            AND revision < 9223372036854775807
    );
    UPDATE role_permission_revision SET revision = revision + 1 WHERE role_id = NEW.role_id;
END;

CREATE TRIGGER role_permission_revision_member_delete
AFTER DELETE ON role_permission
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision is missing, corrupt or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM role_permission_revision WHERE role_id = OLD.role_id
            AND typeof(role_id) = 'integer' AND role_id > 0
            AND typeof(revision) = 'integer' AND revision > 0
            AND revision < 9223372036854775807
    );
    UPDATE role_permission_revision SET revision = revision + 1 WHERE role_id = OLD.role_id;
END;

CREATE TRIGGER role_permission_revision_member_update
AFTER UPDATE OF role_id, permission_id ON role_permission
WHEN NEW.role_id IS NOT OLD.role_id OR NEW.permission_id IS NOT OLD.permission_id
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision is missing, corrupt or exhausted')
    WHERE EXISTS (
        SELECT 1 FROM (SELECT OLD.role_id AS id UNION SELECT NEW.role_id) AS affected
        LEFT JOIN role_permission_revision AS revision ON revision.role_id = affected.id
        WHERE typeof(revision.role_id) <> 'integer' OR revision.role_id <= 0
            OR typeof(revision.revision) <> 'integer' OR revision.revision <= 0
            OR revision.revision = 9223372036854775807
    );
    UPDATE role_permission_revision SET revision = revision + 1
    WHERE role_id IN (OLD.role_id, NEW.role_id);
END;

CREATE TRIGGER role_permission_revision_permission_update
AFTER UPDATE OF id, subject, action ON permission
WHEN NEW.id IS NOT OLD.id OR NEW.subject IS NOT OLD.subject OR NEW.action IS NOT OLD.action
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision is missing, corrupt or exhausted')
    WHERE EXISTS (
        SELECT 1 FROM (
            SELECT DISTINCT role_id FROM role_permission WHERE permission_id IN (OLD.id, NEW.id)
        ) AS affected
        LEFT JOIN role_permission_revision AS revision ON revision.role_id = affected.role_id
        WHERE typeof(revision.role_id) <> 'integer' OR revision.role_id <= 0
            OR typeof(revision.revision) <> 'integer' OR revision.revision <= 0
            OR revision.revision = 9223372036854775807
    );
    UPDATE role_permission_revision SET revision = revision + 1
    WHERE role_id IN (
        SELECT role_id FROM role_permission WHERE permission_id IN (OLD.id, NEW.id)
    );
END;

CREATE TRIGGER role_permission_revision_role_rename
AFTER UPDATE OF name ON role
WHEN NEW.name IS NOT OLD.name
BEGIN
    SELECT RAISE(ABORT, 'Role permission revision is missing, corrupt or exhausted')
    WHERE NOT EXISTS (
        SELECT 1 FROM role_permission_revision WHERE role_id = NEW.id
            AND typeof(revision) = 'integer' AND revision > 0
            AND revision < 9223372036854775807
    );
    UPDATE role_permission_revision SET revision = revision + 1 WHERE role_id = NEW.id;
END;

CREATE TRIGGER promise_item_source_insert
BEFORE INSERT ON promise
WHEN NEW.record_uid IS NULL AND NEW.concept_uid IS NULL
    AND (NEW.transfer_uid IS NULL OR NEW.item_json IS NULL)
BEGIN
    SELECT RAISE(ABORT, 'an unbound promise requires a Transfer item');
END;

CREATE TRIGGER promise_item_source_update
BEFORE UPDATE ON promise
WHEN NEW.record_uid IS NULL AND NEW.concept_uid IS NULL
    AND (NEW.transfer_uid IS NULL OR NEW.item_json IS NULL)
BEGIN
    SELECT RAISE(ABORT, 'an unbound promise requires a Transfer item');
END;

CREATE TRIGGER transfer_occurrence_item_source_insert
BEFORE INSERT ON transfer_occurrence
WHEN NEW.record_uid IS NULL AND NEW.concept_uid IS NULL
    AND NOT EXISTS(SELECT 1 FROM promise WHERE uid = NEW.promise_uid
                   AND transfer_uid = NEW.transfer_uid AND item_json IS NOT NULL)
BEGIN
    SELECT RAISE(ABORT, 'an unbound occurrence requires a Transfer item');
END;

CREATE TRIGGER transfer_occurrence_item_source_update
BEFORE UPDATE ON transfer_occurrence
WHEN NEW.record_uid IS NULL AND NEW.concept_uid IS NULL
    AND NOT EXISTS(SELECT 1 FROM promise WHERE uid = NEW.promise_uid
                   AND transfer_uid = NEW.transfer_uid AND item_json IS NOT NULL)
BEGIN
    SELECT RAISE(ABORT, 'an unbound occurrence requires a Transfer item');
END;

CREATE TRIGGER transfer_local_application_immutable_update
BEFORE UPDATE ON transfer_local_application
BEGIN
    SELECT RAISE(ABORT, 'Local Transfer applications are immutable');
END;

CREATE TRIGGER transfer_local_application_immutable_delete
BEFORE DELETE ON transfer_local_application
BEGIN
    SELECT RAISE(ABORT, 'Local Transfer applications are immutable');
END;

CREATE TRIGGER transfer_local_application_request_collision
BEFORE INSERT ON transfer_local_application
WHEN EXISTS (SELECT 1 FROM transfer_revision WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_invitation_event WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_agreement_event WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase4_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase5_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase5_correction_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_phase6_bulk_request WHERE idempotency_key = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_delivery_policy_event WHERE request_id = NEW.request_id)
  OR EXISTS (SELECT 1 FROM transfer_application_handoff_event WHERE request_id = NEW.request_id)
BEGIN
    SELECT RAISE(ABORT, 'Transfer request id belongs to another workflow');
END;

CREATE TRIGGER transfer_local_application_handoff_exists
BEFORE INSERT ON transfer_local_application
WHEN NOT EXISTS (SELECT 1 FROM transfer_application_effect_handoff WHERE uid = NEW.handoff_uid
                 AND participant_person_uid = NEW.participant_person_uid)
BEGIN
    SELECT RAISE(ABORT, 'Transfer application requires a handoff for this Person');
END;

CREATE TRIGGER transfer_open_claim_pair_matches_promises
BEFORE INSERT ON transfer_open_claim_pair
WHEN NOT EXISTS (
    SELECT 1
    FROM promise proposer
    JOIN promise claimant ON claimant.uid = NEW.claimant_promise_uid
    JOIN promise source ON source.uid = NEW.source_promise_uid
    WHERE proposer.uid = NEW.proposer_promise_uid
      AND proposer.transfer_uid = NEW.transfer_uid
      AND claimant.transfer_uid = NEW.transfer_uid
      AND proposer.party_uid = NEW.proposer_person_uid
      AND source.party_uid = NEW.proposer_person_uid
      AND NEW.source_record_uid IS source.record_uid
      AND proposer.record_uid IS NEW.source_record_uid
      AND claimant.party_uid = NEW.claimant_person_uid
      AND proposer.state = 'proposed'
      AND claimant.state = 'proposed'
      AND proposer.revision = NEW.revision
      AND claimant.revision = NEW.revision
      AND proposer.delta = -claimant.delta
      AND proposer.concept_uid IS claimant.concept_uid
      AND proposer.unit_uid IS claimant.unit_uid
      AND proposer.window_start IS claimant.window_start
      AND proposer.window_end IS claimant.window_end
      AND proposer.location_lat IS claimant.location_lat
      AND proposer.location_lon IS claimant.location_lon
      AND proposer.location_address IS claimant.location_address
      AND proposer.condition IS claimant.condition
      AND proposer.reserve_from = claimant.reserve_from
      AND (
          (NEW.reuse_policy = 'consume'
           AND proposer.uid = NEW.source_promise_uid
           AND source.state = 'proposed'
           AND claimant.source_promise_uid = NEW.source_promise_uid)
          OR
          (NEW.reuse_policy = 'duplicate'
           AND proposer.source_promise_uid = NEW.source_promise_uid
           AND source.state = 'open'
           AND claimant.source_promise_uid = NEW.source_promise_uid)
      )
)
BEGIN
    SELECT RAISE(ABORT, 'OPEN claim pair does not match its concrete promises');
END;

CREATE TRIGGER transfer_open_claim_pair_immutable_update
BEFORE UPDATE ON transfer_open_claim_pair
BEGIN
    SELECT RAISE(ABORT, 'OPEN claim pairs are immutable');
END;

CREATE TRIGGER transfer_open_claim_pair_immutable_delete
BEFORE DELETE ON transfer_open_claim_pair
BEGIN
    SELECT RAISE(ABORT, 'OPEN claim pairs are immutable');
END;

CREATE TRIGGER transfer_private_policy_event_immutable_update
BEFORE UPDATE ON transfer_private_policy_event
BEGIN
    SELECT RAISE(ABORT, 'private application policy events are immutable');
END;

CREATE TRIGGER transfer_private_policy_event_immutable_delete
BEFORE DELETE ON transfer_private_policy_event
BEGIN
    SELECT RAISE(ABORT, 'private application policy events are immutable');
END;

CREATE TRIGGER transfer_private_application_correction_immutable_update
BEFORE UPDATE ON transfer_private_application_correction
BEGIN
    SELECT RAISE(ABORT, 'private application corrections are immutable');
END;

CREATE TRIGGER transfer_private_application_correction_immutable_delete
BEFORE DELETE ON transfer_private_application_correction
BEGIN
    SELECT RAISE(ABORT, 'private application corrections are immutable');
END;

CREATE TRIGGER transfer_stock_limit_event_no_update
BEFORE UPDATE ON transfer_stock_limit_event BEGIN
    SELECT RAISE(ABORT, 'stock limit history is immutable');
END;

CREATE TRIGGER transfer_stock_limit_event_no_delete
BEFORE DELETE ON transfer_stock_limit_event BEGIN
    SELECT RAISE(ABORT, 'stock limit history is immutable');
END;

CREATE TRIGGER transfer_stock_roster_history_insert
AFTER INSERT ON organ_roster BEGIN
    INSERT OR IGNORE INTO transfer_stock_roster_history (organ_uid, version, payload, not_after)
    VALUES (NEW.organ_uid, NEW.version, NEW.payload, NEW.not_after);
END;

CREATE TRIGGER transfer_stock_roster_history_update
AFTER UPDATE ON organ_roster BEGIN
    INSERT OR IGNORE INTO transfer_stock_roster_history (organ_uid, version, payload, not_after)
    VALUES (NEW.organ_uid, NEW.version, NEW.payload, NEW.not_after);
END;

CREATE TRIGGER transfer_stock_roster_writer_insert
BEFORE INSERT ON organ_roster
WHEN EXISTS (SELECT 1 FROM transfer_stock_limit l JOIN record r ON r.uid = l.record_uid WHERE r.organ_uid = NEW.organ_uid AND
    (NOT EXISTS (SELECT 1 FROM json_each(NEW.payload, '$.cells') c, json_each(c.value, '$.capabilities') cap WHERE json_extract(c.value, '$.cell_uid') = l.writer_cell_uid AND cap.value = 'write') OR
     EXISTS (SELECT 1 FROM json_each(NEW.payload, '$.cells') c, json_each(c.value, '$.capabilities') cap WHERE json_extract(c.value, '$.cell_uid') != l.writer_cell_uid AND cap.value = 'write')))
BEGIN
    SELECT RAISE(ABORT, 'remove hard stock limits before changing the Organ writer');
END;

CREATE TRIGGER transfer_stock_roster_writer_update
BEFORE UPDATE ON organ_roster
WHEN EXISTS (SELECT 1 FROM transfer_stock_limit l JOIN record r ON r.uid = l.record_uid WHERE r.organ_uid = NEW.organ_uid AND
    (NOT EXISTS (SELECT 1 FROM json_each(NEW.payload, '$.cells') c, json_each(c.value, '$.capabilities') cap WHERE json_extract(c.value, '$.cell_uid') = l.writer_cell_uid AND cap.value = 'write') OR
     EXISTS (SELECT 1 FROM json_each(NEW.payload, '$.cells') c, json_each(c.value, '$.capabilities') cap WHERE json_extract(c.value, '$.cell_uid') != l.writer_cell_uid AND cap.value = 'write')))
BEGIN
    SELECT RAISE(ABORT, 'remove hard stock limits before changing the Organ writer');
END;

CREATE TRIGGER transfer_stock_limit_record_identity
BEFORE UPDATE OF unit_uid, organ_uid, deleted_at ON record
WHEN EXISTS (SELECT 1 FROM transfer_stock_limit WHERE record_uid = OLD.uid)
 AND (NEW.unit_uid IS NOT OLD.unit_uid OR NEW.organ_uid IS NOT OLD.organ_uid OR NEW.deleted_at IS NOT OLD.deleted_at)
BEGIN
    SELECT RAISE(ABORT, 'remove the hard stock limit before changing the Record unit, owner or deletion state');
END;

CREATE TRIGGER transfer_occurrence_cancellation_no_update
BEFORE UPDATE ON transfer_occurrence_cancellation BEGIN
    SELECT RAISE(ABORT, 'completed cancellations are immutable');
END;

CREATE TRIGGER transfer_occurrence_cancellation_no_delete
BEFORE DELETE ON transfer_occurrence_cancellation BEGIN
    SELECT RAISE(ABORT, 'completed cancellations are immutable');
END;

CREATE TRIGGER transfer_cancellation_application_no_update
BEFORE UPDATE ON transfer_cancellation_application BEGIN
    SELECT RAISE(ABORT, 'completed cancellation requests are immutable');
END;

CREATE TRIGGER transfer_cancellation_application_no_delete
BEFORE DELETE ON transfer_cancellation_application BEGIN
    SELECT RAISE(ABORT, 'completed cancellation requests are immutable');
END;

CREATE TRIGGER transfer_cancellation_no_term_change
BEFORE UPDATE ON transfer_cancellation
WHEN NEW.uid != OLD.uid OR NEW.transfer_uid != OLD.transfer_uid OR NEW.revision != OLD.revision OR NEW.exchange_path_uid != OLD.exchange_path_uid OR NEW.quantity_mantissa != OLD.quantity_mantissa OR NEW.quantity_scale != OLD.quantity_scale OR NEW.occurrences != OLD.occurrences OR NEW.required_people != OLD.required_people OR NEW.proposer_uid != OLD.proposer_uid OR NEW.request_id != OLD.request_id OR NEW.payload != OLD.payload OR NEW.created_at != OLD.created_at OR (OLD.proposal_fact_uid IS NOT NULL AND NEW.proposal_fact_uid IS NOT OLD.proposal_fact_uid) OR (OLD.applied_fact_uid IS NOT NULL AND NEW.applied_fact_uid IS NOT OLD.applied_fact_uid)
BEGIN
    SELECT RAISE(ABORT, 'cancellation terms and completed evidence are immutable');
END;

CREATE TRIGGER transfer_cancellation_no_delete
BEFORE DELETE ON transfer_cancellation BEGIN
    SELECT RAISE(ABORT, 'cancellation history is immutable');
END;

CREATE TRIGGER transfer_cancellation_application_request_unused
BEFORE INSERT ON transfer_cancellation_application
WHEN EXISTS(SELECT 1 FROM transfer_revision WHERE idempotency_key = NEW.request_id) OR EXISTS(SELECT 1 FROM transfer_invitation_event WHERE idempotency_key = NEW.request_id) OR EXISTS(SELECT 1 FROM transfer_agreement_event WHERE idempotency_key = NEW.request_id) OR EXISTS(SELECT 1 FROM transfer_open_claim_pair WHERE idempotency_key = NEW.request_id) OR EXISTS(SELECT 1 FROM transfer_phase4_request WHERE idempotency_key = NEW.request_id) OR EXISTS(SELECT 1 FROM transfer_phase5_request WHERE idempotency_key = NEW.request_id) OR EXISTS(SELECT 1 FROM transfer_phase5_correction_request WHERE idempotency_key = NEW.request_id) OR EXISTS(SELECT 1 FROM transfer_phase6_bulk_request WHERE idempotency_key = NEW.request_id) OR EXISTS(SELECT 1 FROM transfer_correction_link WHERE idempotency_key = NEW.request_id) OR EXISTS(SELECT 1 FROM transfer_promise_successor WHERE idempotency_key = NEW.request_id)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_revision_cancellation_request_unused
BEFORE INSERT ON transfer_revision
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_invitation_event_cancellation_request_unused
BEFORE INSERT ON transfer_invitation_event
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_agreement_event_cancellation_request_unused
BEFORE INSERT ON transfer_agreement_event
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_open_claim_pair_cancellation_request_unused
BEFORE INSERT ON transfer_open_claim_pair
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_phase4_request_cancellation_request_unused
BEFORE INSERT ON transfer_phase4_request
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_phase5_request_cancellation_request_unused
BEFORE INSERT ON transfer_phase5_request
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_phase5_correction_request_cancellation_request_unused
BEFORE INSERT ON transfer_phase5_correction_request
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_phase6_bulk_request_cancellation_request_unused
BEFORE INSERT ON transfer_phase6_bulk_request
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_correction_link_cancellation_request_unused
BEFORE INSERT ON transfer_correction_link
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_promise_successor_cancellation_request_unused
BEFORE INSERT ON transfer_promise_successor
WHEN EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer request id was already used by another action');
END;

CREATE TRIGGER transfer_child_request_immutable_update BEFORE UPDATE ON transfer_child_request
BEGIN SELECT RAISE(ABORT, 'Transfer child requests are immutable'); END;

CREATE TRIGGER transfer_child_request_immutable_delete BEFORE DELETE ON transfer_child_request
BEGIN SELECT RAISE(ABORT, 'Transfer child requests are immutable'); END;

CREATE TRIGGER transfer_outcome_evidence_immutable_update BEFORE UPDATE ON transfer_outcome_evidence
BEGIN SELECT RAISE(ABORT, 'Transfer outcome evidence is immutable'); END;

CREATE TRIGGER transfer_outcome_evidence_immutable_delete BEFORE DELETE ON transfer_outcome_evidence
BEGIN SELECT RAISE(ABORT, 'Transfer outcome evidence is immutable'); END;

CREATE TRIGGER transfer_private_effect_immutable_update
BEFORE UPDATE ON transfer_private_effect
BEGIN
    SELECT RAISE(ABORT, 'private effect evidence is immutable');
END;

CREATE TRIGGER transfer_private_effect_immutable_delete
BEFORE DELETE ON transfer_private_effect
BEGIN
    SELECT RAISE(ABORT, 'private effect evidence is immutable');
END;

CREATE TRIGGER transfer_loan_extension_immutable_update
BEFORE UPDATE ON transfer_loan_extension
BEGIN
    SELECT RAISE(ABORT, 'loan extension proposals are immutable');
END;

CREATE TRIGGER transfer_loan_extension_immutable_delete
BEFORE DELETE ON transfer_loan_extension
BEGIN
    SELECT RAISE(ABORT, 'loan extension proposals are immutable');
END;

CREATE TRIGGER transfer_loan_agreement_immutable_update
BEFORE UPDATE ON transfer_loan_agreement
BEGIN
    SELECT RAISE(ABORT, 'accepted loan terms are immutable');
END;

CREATE TRIGGER transfer_loan_agreement_immutable_delete
BEFORE DELETE ON transfer_loan_agreement
BEGIN
    SELECT RAISE(ABORT, 'accepted loan terms are immutable');
END;

CREATE TRIGGER fact_origin_immutable_update
BEFORE UPDATE ON fact_origin
BEGIN
    SELECT RAISE(ABORT, 'original Fact evidence is immutable');
END;

CREATE TRIGGER fact_origin_immutable_delete
BEFORE DELETE ON fact_origin
WHEN EXISTS (SELECT 1 FROM fact WHERE uid = OLD.fact_uid)
BEGIN
    SELECT RAISE(ABORT, 'original Fact evidence is immutable');
END;

CREATE TRIGGER transfer_sync_message_no_update
BEFORE UPDATE ON transfer_sync_message BEGIN
    SELECT RAISE(ABORT, 'signed Transfer sync messages are immutable');
END;

CREATE TRIGGER transfer_sync_message_no_delete
BEFORE DELETE ON transfer_sync_message BEGIN
    SELECT RAISE(ABORT, 'signed Transfer sync messages are immutable');
END;

CREATE TRIGGER transfer_occurrence_settlement_compensation_matches_slice
BEFORE INSERT ON transfer_occurrence_settlement_compensation
WHEN NOT EXISTS (
    SELECT 1
    FROM transfer_occurrence_settlement_slice slice
    JOIN fact correction ON correction.uid = NEW.compensation_fact_uid
    WHERE slice.uid = NEW.settlement_uid
      AND slice.occurrence_uid = NEW.occurrence_uid
      AND slice.transfer_uid = NEW.transfer_uid
      AND slice.owner_person_uid = NEW.owner_person_uid
      AND slice.application_fact_uid = NEW.original_application_fact_uid
      AND slice.local_record_uid = NEW.local_record_uid
      AND NEW.inverse_delta = -slice.local_delta
      AND correction.record_uid = NEW.local_record_uid
      AND CAST(correction.delta_mantissa AS REAL)
          / CAST(SUBSTR('1000000000000000000', 1, correction.delta_scale + 1) AS REAL)
          = NEW.inverse_delta
      AND correction.actor_uid = NEW.owner_person_uid
      AND ((correction.cause_kind = 'compensation' AND correction.cause_uid = NEW.original_application_fact_uid) OR EXISTS (SELECT 1 FROM fact_origin o JOIN record own ON own.uid = o.organ_uid AND own.slug = 'local-organ' WHERE o.fact_uid = correction.uid AND json_extract(o.payload, '$.cause.kind') = 'compensation' AND json_extract(o.payload, '$.cause.uid') = NEW.original_application_fact_uid))
)
BEGIN
    SELECT RAISE(ABORT, 'settlement compensation does not match its slice');
END;

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

CREATE TRIGGER karma_habit_record_created AFTER INSERT ON record BEGIN
    UPDATE karma_habit_object SET created = 1 WHERE uid = new.uid AND kind IN ('record', 'frequency');
END;

CREATE TRIGGER karma_habit_rule_created AFTER INSERT ON recurrence BEGIN
    UPDATE karma_habit_object SET created = 1 WHERE uid = new.uid AND kind = 'rule';
END;

CREATE TRIGGER transfer_agreement_clear_changed_authority
AFTER UPDATE OF level, revision, at ON transfer_agreement
WHEN NEW.last_event_uid IS OLD.last_event_uid
    AND (NEW.level <> OLD.level OR NEW.revision <> OLD.revision OR NEW.at <> OLD.at)
    AND (SELECT importing FROM transfer_sync_control WHERE id = 1) = 0
BEGIN
    UPDATE transfer_agreement SET last_event_uid = NULL WHERE uid = NEW.uid;
END;

CREATE TRIGGER transfer_agreement_target_request_unused
BEFORE INSERT ON transfer_agreement_target_request
WHEN EXISTS(SELECT 1 FROM transfer_revision WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_invitation_event WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_agreement_event WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_open_claim_pair WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_phase4_request WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_phase5_request WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_phase5_correction_request WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_phase6_bulk_request WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_correction_link WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_promise_successor WHERE idempotency_key = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_cancellation_application WHERE request_id = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_loan_extension WHERE request_id = NEW.idempotency_key) OR EXISTS(SELECT 1 FROM transfer_child_request WHERE request_id = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_revision_agreement_target_request_unused
BEFORE INSERT ON transfer_revision
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_invitation_event_agreement_target_request_unused
BEFORE INSERT ON transfer_invitation_event
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_agreement_event_agreement_target_request_unused
BEFORE INSERT ON transfer_agreement_event
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_open_claim_pair_agreement_target_request_unused
BEFORE INSERT ON transfer_open_claim_pair
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_phase4_request_agreement_target_request_unused
BEFORE INSERT ON transfer_phase4_request
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_phase5_request_agreement_target_request_unused
BEFORE INSERT ON transfer_phase5_request
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_phase5_correction_request_agreement_target_request_unused
BEFORE INSERT ON transfer_phase5_correction_request
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_phase6_bulk_request_agreement_target_request_unused
BEFORE INSERT ON transfer_phase6_bulk_request
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_correction_link_agreement_target_request_unused
BEFORE INSERT ON transfer_correction_link
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_promise_successor_agreement_target_request_unused
BEFORE INSERT ON transfer_promise_successor
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.idempotency_key)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_cancellation_application_agreement_target_request_unused
BEFORE INSERT ON transfer_cancellation_application
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.request_id)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_loan_extension_agreement_target_request_unused
BEFORE INSERT ON transfer_loan_extension
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.request_id)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER transfer_child_request_agreement_target_request_unused
BEFORE INSERT ON transfer_child_request
WHEN EXISTS(SELECT 1 FROM transfer_agreement_target_request WHERE idempotency_key = NEW.request_id)
BEGIN
    SELECT RAISE(ABORT, 'transfer_agreement_target_request_conflict');
END;

CREATE TRIGGER person_role_insert_revision AFTER INSERT ON person_role
BEGIN
    UPDATE person_access SET revision = revision + 1 WHERE person_uid = NEW.person_uid;
END;

CREATE TRIGGER person_role_delete_revision AFTER DELETE ON person_role
BEGIN
    UPDATE person_access SET revision = revision + 1 WHERE person_uid = OLD.person_uid;
END;

CREATE TRIGGER person_access_replace_roles AFTER UPDATE OF role_id ON person_access
WHEN NEW.role_id IS NOT OLD.role_id
BEGIN
    DELETE FROM person_role WHERE person_uid = NEW.person_uid;
END;

CREATE INDEX concept_parent_children ON concept_parent(parent_uid, concept_uid);

CREATE INDEX concept_name_lookup ON concept_name(name, concept_uid);

CREATE INDEX record_extension_namespace ON record_extension(namespace, record_uid);
