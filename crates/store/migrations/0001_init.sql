-- Lince core schema (blueprint Parts I–V, VI, XIII, XV skeletons).
-- Everything is a Record; sidecar tables key on record.uid by kind.

-- Quantities are EXACT decimals, never REAL (blueprint E0.0). A quantity is the
-- pair (mantissa, scale): mantissa is a base-10 integer as TEXT because it is an
-- i128 and SQLite's INTEGER is 64-bit; scale is how many of its digits are
-- fractional. `-10.00` is mantissa '-1000', scale 2. The mantissa is canonical
-- (no leading zeros, no negative zero), so `mantissa != '0'` is an exact
-- is-nonzero test and `mantissa LIKE '-%'` is an exact is-negative test.
-- Never SUM() these in SQL: numeric affinity would turn them back into floats.
CREATE TABLE record (
    uid         TEXT PRIMARY KEY,
    slug        TEXT UNIQUE,
    kind        TEXT NOT NULL DEFAULT 'plain',
    head        TEXT NOT NULL DEFAULT '',
    body        TEXT NOT NULL DEFAULT '',
    -- CACHE of the fold over this record's fact chain. Single writer: engine append().
    quantity_mantissa TEXT NOT NULL DEFAULT '0',
    quantity_scale    INTEGER NOT NULL DEFAULT 0,
    concept_uid TEXT REFERENCES concept(uid),
    unit_uid    TEXT REFERENCES concept(uid),
    place_uid   TEXT REFERENCES place(uid),
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX idx_record_concept ON record(concept_uid);
CREATE INDEX idx_record_kind ON record(kind);

CREATE TABLE fact (
    uid        TEXT PRIMARY KEY,
    record_uid TEXT NOT NULL REFERENCES record(uid),
    -- The exact movement (see the note on `record`). Both halves are inside the
    -- hash preimage, so an amount is covered by the signature.
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
);
CREATE INDEX idx_fact_record_at ON fact(record_uid, at);
CREATE INDEX idx_fact_cause ON fact(cause_kind, cause_uid);

CREATE TABLE concept (
    uid            TEXT PRIMARY KEY,
    canonical_name TEXT NOT NULL UNIQUE,
    origin_organ   TEXT,
    instinct       TEXT,
    created_at     TEXT NOT NULL
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

CREATE TABLE link (
    uid        TEXT PRIMARY KEY,
    from_uid   TEXT NOT NULL REFERENCES record(uid),
    kind_uid   TEXT NOT NULL REFERENCES concept(uid),
    to_uid     TEXT NOT NULL REFERENCES record(uid),
    quantity   REAL,
    created_at TEXT NOT NULL,
    UNIQUE(from_uid, kind_uid, to_uid)          -- identity is the TRIPLE
);
CREATE INDEX idx_link_from ON link(from_uid, kind_uid);
CREATE INDEX idx_link_to ON link(to_uid, kind_uid);

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
    updated_at   TEXT NOT NULL,
    CHECK (record_uid IS NOT NULL OR concept_uid IS NOT NULL)
);
CREATE INDEX idx_promise_record_state ON promise(record_uid, state);
CREATE INDEX idx_promise_transfer ON promise(transfer_uid);

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

-- Karma sidecars (Part VI)
CREATE TABLE rule (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid),
    condition  TEXT NOT NULL,
    gate       TEXT NOT NULL DEFAULT '!=0',
    carry      TEXT NOT NULL DEFAULT 'value',
    debounce   TEXT
);
CREATE TABLE rule_consequence (
    uid      TEXT PRIMARY KEY,
    rule_uid TEXT NOT NULL REFERENCES rule(record_uid),
    position INTEGER NOT NULL DEFAULT 0,
    kind     TEXT NOT NULL,
    target   TEXT,
    params   TEXT CHECK (params IS NULL OR json_valid(params))
);
CREATE TABLE signal (
    record_uid  TEXT PRIMARY KEY REFERENCES record(uid),
    source_kind TEXT NOT NULL,
    source      TEXT NOT NULL,
    schedule    TEXT NOT NULL,
    parse       TEXT NOT NULL DEFAULT 'number',
    last_sampled_at TEXT
);
CREATE TABLE frequency (
    record_uid  TEXT PRIMARY KEY REFERENCES record(uid),
    seconds     INTEGER NOT NULL DEFAULT 0,
    days        INTEGER NOT NULL DEFAULT 0,
    months      INTEGER NOT NULL DEFAULT 0,
    day_of_week INTEGER,
    next_at     TEXT NOT NULL,
    finish_at   TEXT,
    catch_up    INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE effect_queue (
    uid        TEXT PRIMARY KEY,
    kind       TEXT NOT NULL,                   -- command | notify
    payload    TEXT NOT NULL,
    origin_uid TEXT,                            -- the rule record that queued it
    status     TEXT NOT NULL DEFAULT 'queued',  -- queued | running | done | failed
    attempts   INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    finished_at TEXT,
    result     TEXT
);

-- Attention (Part XIII)
CREATE TABLE decision (
    record_uid  TEXT PRIMARY KEY REFERENCES record(uid),
    subject_uid TEXT NOT NULL,
    kind        TEXT NOT NULL,                  -- proposal | agreement | ask | crossing | draft
    options     TEXT NOT NULL CHECK (json_valid(options)),
    default_opt TEXT,
    expires_at  TEXT,
    decided_at  TEXT,
    answer      TEXT
);

-- Transfer (Part VIII)
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
);
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
    at           TEXT NOT NULL,
    UNIQUE(transfer_uid, party_uid)
);

-- Visibility & Trust skeletons (Parts XV, XI)
CREATE TABLE visibility_rule (
    uid          TEXT PRIMARY KEY,
    subject_kind TEXT NOT NULL,                 -- organ | actor | role | public | fiote
    subject_uid  TEXT,
    target_uid   TEXT NOT NULL,
    field        TEXT,
    grant_level  TEXT NOT NULL                  -- visible | hidden
);
CREATE TABLE identity_key (
    actor_uid  TEXT NOT NULL,
    key_id     TEXT NOT NULL,
    public_key TEXT NOT NULL,
    UNIQUE(actor_uid, key_id)
);

-- Native app tables (not Ledger records): singleton settings + the
-- permission/role/user workflow. Configuration is as native and structured as
-- the Transfer/Rule sidecars, so it gets a real typed table rather than a JSON
-- extension; column DEFAULTs ARE the default policy, an UPDATE is the override.
CREATE TABLE configuration (
    id                           INTEGER PRIMARY KEY CHECK (id = 1), -- singleton
    quantity                     INTEGER NOT NULL DEFAULT 0,
    name                         TEXT NOT NULL DEFAULT 'Default',
    language                     TEXT NOT NULL DEFAULT 'en',
    timezone                     INTEGER NOT NULL DEFAULT 0,
    style                        TEXT NOT NULL DEFAULT 'catppuccin_macchiato',
    command_notification_seconds REAL NOT NULL DEFAULT 0, -- If its zero we dont show, any positive number enables it and uses the value.
    delete_confirmation          INTEGER NOT NULL DEFAULT 1,
    error_toast_seconds          REAL NOT NULL DEFAULT 5,
    keybinding_mode              INTEGER NOT NULL DEFAULT 0,
    created_at                   TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at                   TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP

    -- Features that are wanted later.
    -- 
    -- desktop_start_on_login INTEGER,
    -- desktop_start_silent INTEGER,
    -- automatic_update_channel TEXT NOT NULL DEFAULT 'rolling',
    -- automatic_update_notify_enabled INTEGER NOT NULL DEFAULT 1,
    -- automatic_update_install_enabled INTEGER NOT NULL DEFAULT 1,
    -- automatic_update_last_seen_revision TEXT
    -- 
    -- file_sync_enabled INTEGER NOT NULL DEFAULT 0,
    -- file_sync_path TEXT,
    -- 
    -- bucket_enabled INTEGER NOT NULL DEFAULT 0,
    -- bucket_username TEXT,
    -- bucket_password TEXT,
    -- bucket_uri TEXT,
    -- bucket_name TEXT,
    -- bucket_region TEXT,
    -- 
    -- Since these involve the new transfer system we need to refactor them, these configurations where transfer specific, now if visibility policies are for record-wide then the anonymous package is not transfer specific and should be more generic policy, same for other properties, only do this looking into how they where used and porting the intention of each of these features (if they are still needed and not superseeded by new features) to new system if asked to.
    -- transfer_public_proposals_enabled INTEGER NOT NULL DEFAULT 0,
    -- transfer_known_peer_polling_enabled INTEGER NOT NULL DEFAULT 1 CHECK (transfer_known_peer_polling_enabled IN (0, 1)),
    -- transfer_reservation_policy TEXT NOT NULL DEFAULT 'soft' CHECK (transfer_reservation_policy IN ('none', 'soft', 'hard_on_proposal', 'hard_on_consume', 'hard_on_lock')),
    -- transfer_send_received_receipts INTEGER NOT NULL DEFAULT 1 CHECK (transfer_send_received_receipts IN (0, 1)),
    -- transfer_send_seen_receipts INTEGER NOT NULL DEFAULT 1 CHECK (transfer_send_seen_receipts IN (0, 1)),
    -- transfer_anonymous_package_viewing INTEGER NOT NULL DEFAULT 0 CHECK (transfer_anonymous_package_viewing IN (0, 1)),
    -- transfer_share_quantity_projections INTEGER NOT NULL DEFAULT 0 CHECK (transfer_share_quantity_projections IN (0, 1))
) STRICT;


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
CREATE TABLE app_user (
    id            INTEGER PRIMARY KEY,
    name          TEXT NOT NULL CHECK (length(trim(name)) > 0),
    username      TEXT NOT NULL UNIQUE CHECK (length(trim(username)) > 0),
    password_hash TEXT NOT NULL CHECK (length(trim(password_hash)) > 0),
    created_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at    TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    role_id       INTEGER REFERENCES role(id) CHECK (role_id IS NULL OR role_id > 0)
) STRICT;
