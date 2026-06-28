CREATE TABLE IF NOT EXISTS record (
    id INTEGER PRIMARY KEY,
    quantity REAL NOT NULL DEFAULT 1,
    head TEXT,
    body TEXT,
    owner_organ_id INTEGER REFERENCES organ(id),
    sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0),
    origin_organ_id INTEGER REFERENCES organ(id),
    created_at TEXT,
    updated_at TEXT
);
CREATE TABLE IF NOT EXISTS view (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL,
    query TEXT NOT NULL DEFAULT 'SELECT * FROM record'
);
CREATE TABLE IF NOT EXISTS collection (
    id INTEGER PRIMARY KEY,
    quantity INTEGER,
    name TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS configuration (
    id INTEGER PRIMARY KEY,
    quantity INTEGER,
    name TEXT NOT NULL,
    language TEXT,
    timezone INTEGER,
    style TEXT,
    show_command_notifications INTEGER NOT NULL DEFAULT 0,
    command_notification_seconds REAL NOT NULL DEFAULT -1,
    delete_confirmation INTEGER NOT NULL DEFAULT 1,
    error_toast_seconds REAL NOT NULL DEFAULT 5,
    keybinding_mode INTEGER NOT NULL DEFAULT 0,
    bucket_enabled INTEGER NOT NULL DEFAULT 0,
    bucket_username TEXT,
    bucket_password TEXT,
    bucket_uri TEXT,
    bucket_name TEXT,
    bucket_region TEXT,
    file_sync_enabled INTEGER NOT NULL DEFAULT 0,
    file_sync_path TEXT,
    transfer_public_proposals_enabled INTEGER NOT NULL DEFAULT 0,
    transfer_known_peer_polling_enabled INTEGER NOT NULL DEFAULT 1 CHECK (transfer_known_peer_polling_enabled IN (0, 1)),
    transfer_reservation_policy TEXT NOT NULL DEFAULT 'soft' CHECK (transfer_reservation_policy IN ('none', 'soft', 'hard_on_proposal', 'hard_on_consume', 'hard_on_lock')),
    transfer_send_received_receipts INTEGER NOT NULL DEFAULT 1 CHECK (transfer_send_received_receipts IN (0, 1)),
    transfer_send_seen_receipts INTEGER NOT NULL DEFAULT 1 CHECK (transfer_send_seen_receipts IN (0, 1)),
    transfer_anonymous_package_viewing INTEGER NOT NULL DEFAULT 0 CHECK (transfer_anonymous_package_viewing IN (0, 1)),
    transfer_share_quantity_projections INTEGER NOT NULL DEFAULT 0 CHECK (transfer_share_quantity_projections IN (0, 1)),
    desktop_start_on_login INTEGER,
    desktop_start_silent INTEGER,
    automatic_update_channel TEXT NOT NULL DEFAULT 'rolling',
    automatic_update_notify_enabled INTEGER NOT NULL DEFAULT 1,
    automatic_update_install_enabled INTEGER NOT NULL DEFAULT 1,
    automatic_update_last_seen_revision TEXT
);
CREATE TABLE IF NOT EXISTS collection_view (
    id INTEGER PRIMARY KEY,
    quantity INTEGER NOT NULL DEFAULT 1,
    collection_id INTEGER REFERENCES collection(id),
    view_id INTEGER REFERENCES view(id),
    column_sizes TEXT NOT NULL DEFAULT '{}'
);
CREATE TABLE IF NOT EXISTS karma_condition (
    id INTEGER PRIMARY KEY,
    quantity INTEGER NOT NULL DEFAULT 1,
    name TEXT NOT NULL DEFAULT 'Condition',
    condition TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS karma_consequence (
    id INTEGER PRIMARY KEY,
    quantity INTEGER NOT NULL DEFAULT 1,
    name TEXT NOT NULL DEFAULT 'Consequence',
    consequence TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS karma (
    id INTEGER PRIMARY KEY,
    quantity INTEGER NOT NULL DEFAULT 1,
    name TEXT NOT NULL DEFAULT 'Karma',
    condition_id INTEGER NOT NULL,
    operator TEXT NOT NULL,
    consequence_id INTEGER NOT NULL,
    parallel INTEGER NOT NULL DEFAULT 0,
    timeout_seconds REAL NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS frequency (
    id INTEGER PRIMARY KEY,
    quantity REAL NOT NULL DEFAULT 1,
    name TEXT NOT NULL DEFAULT 'Frequency',
    day_week REAL,
    months REAL NOT NULL DEFAULT 0,
    days REAL NOT NULL DEFAULT 0,
    seconds REAL NOT NULL DEFAULT 0,
    next_date TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    finish_date DATETIME,
    catch_up_sum INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE IF NOT EXISTS command (
    id INTEGER PRIMARY KEY,
    quantity REAL NOT NULL DEFAULT 1,
    name TEXT NOT NULL DEFAULT 'Command',
    command TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS transfer (
    id INTEGER PRIMARY KEY,
    quantity REAL NOT NULL DEFAULT 1
);
CREATE TABLE IF NOT EXISTS transfer_node_identity (
    id INTEGER PRIMARY KEY,
    label TEXT NOT NULL CHECK (length(trim(label)) > 0),
    public_key TEXT NOT NULL CHECK (length(trim(public_key)) > 0),
    secret_key TEXT NOT NULL CHECK (length(trim(secret_key)) > 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE TABLE IF NOT EXISTS transfer_identity (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL UNIQUE REFERENCES transfer(id) ON DELETE CASCADE,
    transfer_uid TEXT NOT NULL CHECK (length(trim(transfer_uid)) > 0),
    parent_transfer_uid TEXT,
    source_transfer_uid TEXT,
    state TEXT NOT NULL CHECK (length(trim(state)) > 0),
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    coordinator_label TEXT NOT NULL CHECK (length(trim(coordinator_label)) > 0),
    proposer_label TEXT NOT NULL CHECK (length(trim(proposer_label)) > 0),
    counterparty_label TEXT NOT NULL CHECK (length(trim(counterparty_label)) > 0),
    contribution_actor_label TEXT NOT NULL CHECK (length(trim(contribution_actor_label)) > 0),
    contribution_public_key TEXT,
    need_actor_label TEXT NOT NULL CHECK (length(trim(need_actor_label)) > 0),
    need_public_key TEXT,
    target_organ_id INTEGER,
    target_organ_name TEXT,
    target_base_url TEXT,
    source_base_url TEXT,
    topic_text TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_transfer_identity_uid ON transfer_identity(transfer_uid);
CREATE TABLE IF NOT EXISTS transfer_relation (
    id INTEGER PRIMARY KEY,
    transfer_uid TEXT NOT NULL CHECK (length(trim(transfer_uid)) > 0),
    relation_type TEXT NOT NULL CHECK (relation_type IN ('parent', 'depends_on')),
    target_transfer_uid TEXT NOT NULL CHECK (length(trim(target_transfer_uid)) > 0),
    position REAL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE INDEX IF NOT EXISTS idx_transfer_relation_transfer_type ON transfer_relation(transfer_uid, relation_type);
CREATE INDEX IF NOT EXISTS idx_transfer_relation_target_type ON transfer_relation(target_transfer_uid, relation_type);
CREATE UNIQUE INDEX IF NOT EXISTS uq_transfer_relation_identity ON transfer_relation(transfer_uid, relation_type, target_transfer_uid);
CREATE TABLE IF NOT EXISTS transfer_tree_config (
    id INTEGER PRIMARY KEY,
    transfer_uid TEXT NOT NULL CHECK (length(trim(transfer_uid)) > 0),
    branch_mode TEXT NOT NULL CHECK (branch_mode IN ('inherit', 'duplicated', 'greedy')),
    record_sync_mode TEXT NOT NULL CHECK (record_sync_mode IN ('none', 'copy_once', 'live')),
    reservation_policy TEXT CHECK (reservation_policy IS NULL OR reservation_policy IN ('none', 'soft', 'hard_on_proposal', 'hard_on_consume', 'hard_on_lock')),
    source_record_id INTEGER REFERENCES record(id),
    sync_role TEXT CHECK (sync_role IS NULL OR sync_role IN ('need', 'contribution')),
    sync_quantity REAL,
    sync_counterparty_label TEXT,
    sync_target_organ_id INTEGER,
    last_synced_record_head TEXT,
    sync_enabled INTEGER NOT NULL DEFAULT 0,
    last_synced_at TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_transfer_tree_config_uid ON transfer_tree_config(transfer_uid);
CREATE TABLE IF NOT EXISTS transfer_party (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    party_uid TEXT CHECK (party_uid IS NULL OR length(trim(party_uid)) > 0),
    participation_kind TEXT NOT NULL CHECK (participation_kind IN ('participant', 'coordinator', 'observer', 'placeholder')),
    role_hint TEXT CHECK (role_hint IS NULL OR role_hint IN ('need', 'contribution', 'support', 'task', 'information', 'reservation')),
    actor_label TEXT NOT NULL CHECK (length(trim(actor_label)) > 0),
    public_key TEXT,
    organ_id INTEGER,
    user_id INTEGER,
    placeholder INTEGER NOT NULL DEFAULT 0,
    replaced_by_party_id INTEGER,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE INDEX IF NOT EXISTS idx_transfer_party_transfer_role ON transfer_party(transfer_id, participation_kind);
CREATE INDEX IF NOT EXISTS idx_transfer_party_public_key ON transfer_party(public_key);
CREATE UNIQUE INDEX IF NOT EXISTS uq_transfer_party_uid ON transfer_party(transfer_id, party_uid);
CREATE TABLE IF NOT EXISTS transfer_structured_item (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    item_uid TEXT CHECK (item_uid IS NULL OR length(trim(item_uid)) > 0),
    role TEXT NOT NULL CHECK (role IN ('need', 'contribution', 'support', 'task', 'information', 'reservation')),
    source_record_id INTEGER REFERENCES record(id),
    owner_party_id INTEGER,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    description TEXT,
    record_head_snapshot TEXT,
    record_body_snapshot TEXT,
    quantity REAL,
    unit TEXT,
    location TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE INDEX IF NOT EXISTS idx_transfer_structured_item_transfer_role ON transfer_structured_item(transfer_id, role);
CREATE INDEX IF NOT EXISTS idx_transfer_structured_item_source_record ON transfer_structured_item(source_record_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_transfer_structured_item_uid ON transfer_structured_item(transfer_id, item_uid);
CREATE TABLE IF NOT EXISTS transfer_interaction (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    interaction_uid TEXT CHECK (interaction_uid IS NULL OR length(trim(interaction_uid)) > 0),
    interaction_kind TEXT NOT NULL CHECK (interaction_kind IN ('contributes_to', 'depends_on', 'unblocks', 'replaces', 'informs')),
    direction TEXT NOT NULL CHECK (direction IN ('incoming', 'outgoing', 'mutual', 'informational')),
    from_item_id INTEGER,
    to_item_id INTEGER,
    from_party_id INTEGER,
    to_party_id INTEGER,
    quantity REAL,
    state TEXT NOT NULL,
    dependency_kind TEXT CHECK (dependency_kind IS NULL OR dependency_kind IN ('must_agree', 'must_activate', 'must_deliver', 'must_receive', 'must_settle')),
    metadata_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(metadata_json)),
    version INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE INDEX IF NOT EXISTS idx_transfer_interaction_transfer_kind ON transfer_interaction(transfer_id, interaction_kind);
CREATE INDEX IF NOT EXISTS idx_transfer_interaction_from_item ON transfer_interaction(from_item_id);
CREATE INDEX IF NOT EXISTS idx_transfer_interaction_to_item ON transfer_interaction(to_item_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_transfer_interaction_uid ON transfer_interaction(transfer_id, interaction_uid);
CREATE TABLE IF NOT EXISTS transfer_agreement (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    party_id INTEGER,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('transfer', 'item', 'interaction')),
    scope_id INTEGER,
    agreement_level INTEGER NOT NULL DEFAULT 0 CHECK (agreement_level IN (0, 1, 2)),
    agreed_item_version INTEGER,
    agreed_interaction_version INTEGER,
    event_id INTEGER,
    agreed_at TEXT,
    invalidated_at TEXT,
    invalidated_by_event_id INTEGER,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE INDEX IF NOT EXISTS idx_transfer_agreement_scope ON transfer_agreement(transfer_id, scope_kind, scope_id);
CREATE INDEX IF NOT EXISTS idx_transfer_agreement_party ON transfer_agreement(party_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_transfer_agreement_party_scope ON transfer_agreement(transfer_id, party_id, scope_kind, scope_id);
CREATE TABLE IF NOT EXISTS transfer_confirmation (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    party_id INTEGER,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('transfer', 'item', 'interaction')),
    scope_id INTEGER,
    confirmation_kind TEXT NOT NULL CHECK (confirmation_kind IN ('delivery', 'receipt')),
    event_id INTEGER,
    confirmed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE INDEX IF NOT EXISTS idx_transfer_confirmation_scope ON transfer_confirmation(transfer_id, scope_kind, scope_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_transfer_confirmation_party_scope_kind ON transfer_confirmation(transfer_id, party_id, scope_kind, scope_id, confirmation_kind);
CREATE TABLE IF NOT EXISTS transfer_event (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    transfer_uid TEXT,
    event_uid TEXT,
    actor_label TEXT NOT NULL CHECK (length(trim(actor_label)) > 0),
    actor_public_key TEXT,
    event_kind TEXT NOT NULL CHECK (event_kind IN ('transfer_created', 'transfer_quantity_changed', 'transfer_inactivated', 'item_created', 'item_edited', 'interaction_created', 'interaction_edited', 'visibility_changed', 'agreement_changed', 'message_sent', 'delivery_confirmed', 'receipt_confirmed', 'package_received', 'package_seen', 'settlement_applied', 'settlement_reverted', 'dispute_opened', 'dispute_resolved')),
    payload_json TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload_json)),
    previous_event_id INTEGER REFERENCES transfer_event(id),
    previous_event_uid TEXT,
    previous_event_hash TEXT,
    event_hash TEXT,
    signature TEXT,
    validation_state TEXT NOT NULL DEFAULT 'pending' CHECK (validation_state IN ('pending', 'valid', 'invalid')),
    validation_error TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE TABLE IF NOT EXISTS transfer_structured_settlement (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    party_id INTEGER,
    item_id INTEGER,
    interaction_id INTEGER,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('transfer', 'item', 'interaction')),
    scope_id INTEGER,
    local_record_id INTEGER NOT NULL REFERENCES record(id),
    quantity_delta REAL NOT NULL,
    event_id INTEGER,
    settled_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE INDEX IF NOT EXISTS idx_transfer_structured_settlement_scope ON transfer_structured_settlement(transfer_id, scope_kind, scope_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_transfer_structured_settlement_record_scope ON transfer_structured_settlement(transfer_id, party_id, local_record_id, scope_kind, scope_id);
CREATE TABLE IF NOT EXISTS transfer_quantity_influence (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    item_id INTEGER,
    interaction_id INTEGER,
    record_id INTEGER NOT NULL REFERENCES record(id),
    influence REAL NOT NULL,
    influence_state TEXT NOT NULL DEFAULT 'planned' CHECK (influence_state IN ('planned', 'active', 'consumed', 'released', 'invalidated')),
    policy TEXT NOT NULL DEFAULT 'manual' CHECK (policy IN ('protect_transfer', 'surplus_transfer', 'proportional', 'manual')),
    event_id INTEGER,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    consumed_at TEXT
) STRICT;
CREATE INDEX IF NOT EXISTS idx_transfer_quantity_influence_record_state ON transfer_quantity_influence(record_id, influence_state);
CREATE INDEX IF NOT EXISTS idx_transfer_quantity_influence_transfer ON transfer_quantity_influence(transfer_id);
CREATE TABLE IF NOT EXISTS transfer_message (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    interaction_id INTEGER,
    party_id INTEGER,
    body TEXT NOT NULL CHECK (length(trim(body)) > 0),
    event_id INTEGER,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE INDEX IF NOT EXISTS idx_transfer_message_transfer_created ON transfer_message(transfer_id, created_at);
CREATE TABLE IF NOT EXISTS transfer_visibility_subject (
    id INTEGER PRIMARY KEY,
    subject_kind TEXT NOT NULL CHECK (subject_kind IN ('user', 'organ', 'party', 'public')),
    user_id INTEGER,
    organ_id INTEGER,
    party_id INTEGER,
    display_name_snapshot TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE INDEX IF NOT EXISTS idx_transfer_visibility_subject_kind ON transfer_visibility_subject(subject_kind);
CREATE TABLE IF NOT EXISTS transfer_visibility_rule (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    subject_id INTEGER NOT NULL,
    scope_kind TEXT NOT NULL CHECK (scope_kind IN ('transfer', 'item', 'interaction', 'event', 'record', 'message')),
    scope_id INTEGER,
    can_discover INTEGER NOT NULL DEFAULT 0,
    can_view INTEGER NOT NULL DEFAULT 0,
    can_edit INTEGER NOT NULL DEFAULT 0,
    can_agree INTEGER NOT NULL DEFAULT 0,
    can_confirm_delivery INTEGER NOT NULL DEFAULT 0,
    can_confirm_receipt INTEGER NOT NULL DEFAULT 0,
    can_settle INTEGER NOT NULL DEFAULT 0,
    can_message INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE INDEX IF NOT EXISTS idx_transfer_visibility_rule_subject ON transfer_visibility_rule(subject_id);
CREATE INDEX IF NOT EXISTS idx_transfer_visibility_rule_scope ON transfer_visibility_rule(transfer_id, scope_kind, scope_id);
CREATE TABLE IF NOT EXISTS transfer_visibility_field (
    id INTEGER PRIMARY KEY,
    visibility_rule_id INTEGER NOT NULL,
    field_name TEXT NOT NULL CHECK (length(trim(field_name)) > 0),
    visible INTEGER NOT NULL DEFAULT 0,
    editable INTEGER NOT NULL DEFAULT 0,
    redaction_label TEXT
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS uq_transfer_visibility_field_rule_name ON transfer_visibility_field(visibility_rule_id, field_name);
CREATE TABLE IF NOT EXISTS transfer_settlement (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL UNIQUE REFERENCES transfer(id) ON DELETE CASCADE,
    my_record_id INTEGER NOT NULL REFERENCES record(id),
    server_record_id INTEGER NOT NULL REFERENCES record(id),
    my_quantity_delta REAL NOT NULL,
    server_quantity_delta REAL NOT NULL,
    event_id INTEGER REFERENCES transfer_event(id),
    settled_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE TABLE IF NOT EXISTS transfer_local_settlement (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    local_record_id INTEGER NOT NULL REFERENCES record(id),
    local_actor_label TEXT NOT NULL CHECK (length(trim(local_actor_label)) > 0),
    local_quantity_delta REAL NOT NULL,
    event_id INTEGER REFERENCES transfer_event(id),
    settled_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_transfer_local_settlement_transfer_actor ON transfer_local_settlement(transfer_id, local_actor_label);
CREATE TABLE IF NOT EXISTS transfer_sync_cursor (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    peer_label TEXT NOT NULL CHECK (length(trim(peer_label)) > 0),
    last_event_id INTEGER REFERENCES transfer_event(id),
    last_synced_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS idx_transfer_sync_cursor_transfer_peer ON transfer_sync_cursor(transfer_id, peer_label);
CREATE TABLE IF NOT EXISTS sum (
    id INTEGER PRIMARY KEY,
    quantity REAL NOT NULL DEFAULT 1,
    record_id INTEGER,
    interval_relative BOOLEAN,
    interval_length TEXT,
    sum_mode INTEGER,
    end_lag TEXT,
    end_date DATETIME
);
CREATE TABLE IF NOT EXISTS history (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL,
    change_time TEXT DEFAULT CURRENT_TIMESTAMP,
    old_quantity REAL NOT NULL,
    new_quantity REAL NOT NULL
);
CREATE TABLE IF NOT EXISTS query (
    id INTEGER PRIMARY KEY,
    name TEXT,
    query TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS role (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE CHECK (length(trim(name)) > 0)
) STRICT;
CREATE TABLE IF NOT EXISTS permission (
    id INTEGER PRIMARY KEY,
    subject TEXT NOT NULL CHECK (length(trim(subject)) > 0),
    action TEXT NOT NULL CHECK (length(trim(action)) > 0),
    description TEXT CHECK (description IS NULL OR length(trim(description)) > 0)
) STRICT;
CREATE TABLE IF NOT EXISTS role_permission (
    role_id INTEGER NOT NULL REFERENCES role(id) ON DELETE CASCADE CHECK (role_id > 0),
    permission_id INTEGER NOT NULL REFERENCES permission(id) ON DELETE CASCADE CHECK (permission_id > 0),
    PRIMARY KEY (role_id, permission_id)
) STRICT;
CREATE TABLE IF NOT EXISTS app_user (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    username TEXT NOT NULL UNIQUE CHECK (length(trim(username)) > 0),
    password_hash TEXT NOT NULL CHECK (length(trim(password_hash)) > 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(created_at) IS NOT NULL),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(updated_at) IS NOT NULL),
    role_id INTEGER REFERENCES role(id) CHECK (role_id IS NULL OR role_id > 0)
) STRICT;
CREATE TABLE IF NOT EXISTS organ (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL CHECK (length(trim(name)) > 0),
    base_url TEXT NOT NULL CHECK (length(trim(base_url)) > 0),
    trust_state TEXT NOT NULL DEFAULT 'known' CHECK (trust_state IN ('unknown', 'known', 'blocked')),
    contact_discovery_enabled INTEGER NOT NULL DEFAULT 0 CHECK (contact_discovery_enabled IN (0, 1)),
    last_seen_at TEXT CHECK (last_seen_at IS NULL OR julianday(last_seen_at) IS NOT NULL),
    last_transfer_polled_at TEXT CHECK (last_transfer_polled_at IS NULL OR julianday(last_transfer_polled_at) IS NOT NULL),
    proximity INTEGER NOT NULL DEFAULT 100 CHECK (proximity >= 0),
    transfer_send_received_receipts INTEGER NOT NULL DEFAULT 1 CHECK (transfer_send_received_receipts IN (0, 1)),
    transfer_send_seen_receipts INTEGER NOT NULL DEFAULT 1 CHECK (transfer_send_seen_receipts IN (0, 1)),
    file_sync_enabled INTEGER NOT NULL DEFAULT 0 CHECK (file_sync_enabled IN (0, 1)),
    file_sync_path TEXT
) STRICT;
CREATE TABLE IF NOT EXISTS view_dependency (
    view_id INTEGER NOT NULL REFERENCES view(id) ON DELETE CASCADE CHECK (view_id > 0),
    table_name TEXT NOT NULL CHECK (length(trim(table_name)) > 0),
    PRIMARY KEY (view_id, table_name)
) STRICT;
CREATE TABLE IF NOT EXISTS record_extension (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE CHECK (record_id > 0),
    namespace TEXT NOT NULL CHECK (length(trim(namespace)) > 0),
    version INTEGER NOT NULL DEFAULT 1 CHECK (version >= 1),
    freestyle_data_structure TEXT NOT NULL CHECK (json_valid(freestyle_data_structure)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(created_at) IS NOT NULL),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(updated_at) IS NOT NULL),
    sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0),
    origin_organ_id INTEGER REFERENCES organ(id)
) STRICT;
CREATE INDEX IF NOT EXISTS idx_record_extension_namespace_record ON record_extension(namespace, record_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_record_extension_record_namespace ON record_extension(record_id, namespace);
CREATE TABLE IF NOT EXISTS record_link (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE CHECK (record_id > 0),
    link_type TEXT NOT NULL CHECK (length(trim(link_type)) > 0),
    target_table TEXT NOT NULL CHECK (length(trim(target_table)) > 0),
    target_id INTEGER NOT NULL CHECK (target_id > 0),
    position REAL,
    freestyle_data_structure TEXT CHECK (freestyle_data_structure IS NULL OR json_valid(freestyle_data_structure)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(created_at) IS NOT NULL),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(updated_at) IS NOT NULL),
    sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0),
    origin_organ_id INTEGER REFERENCES organ(id)
) STRICT;
CREATE INDEX IF NOT EXISTS idx_record_link_record_type ON record_link(record_id, link_type, target_table);
CREATE INDEX IF NOT EXISTS idx_record_link_target_type ON record_link(target_table, target_id, link_type);
CREATE UNIQUE INDEX IF NOT EXISTS uq_record_link_identity ON record_link(record_id, link_type, target_table, target_id);
CREATE TABLE IF NOT EXISTS record_comment (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE CHECK (record_id > 0),
    author_user_id INTEGER REFERENCES app_user(id) ON DELETE SET NULL CHECK (author_user_id IS NULL OR author_user_id > 0),
    body TEXT NOT NULL CHECK (length(trim(body)) > 0),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(created_at) IS NOT NULL),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(updated_at) IS NOT NULL),
    deleted_at TEXT CHECK (deleted_at IS NULL OR julianday(deleted_at) IS NOT NULL),
    sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0),
    origin_organ_id INTEGER REFERENCES organ(id)
) STRICT;
CREATE INDEX IF NOT EXISTS idx_record_comment_record_created ON record_comment(record_id, created_at DESC);
CREATE TABLE IF NOT EXISTS record_worklog (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE CHECK (record_id > 0),
    author_user_id INTEGER NOT NULL REFERENCES app_user(id) ON DELETE CASCADE CHECK (author_user_id > 0),
    started_at TEXT NOT NULL,
    ended_at TEXT,
    last_heartbeat_at TEXT,
    seconds REAL CHECK (seconds IS NULL OR seconds >= 0),
    note TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(created_at) IS NOT NULL),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(updated_at) IS NOT NULL),
    sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0),
    origin_organ_id INTEGER REFERENCES organ(id),
    CHECK (length(trim(started_at)) > 0 AND julianday(started_at) IS NOT NULL),
    CHECK (ended_at IS NULL OR (length(trim(ended_at)) > 0 AND julianday(ended_at) IS NOT NULL)),
    CHECK (last_heartbeat_at IS NULL OR (length(trim(last_heartbeat_at)) > 0 AND julianday(last_heartbeat_at) IS NOT NULL)),
    CHECK (ended_at IS NULL OR julianday(ended_at) >= julianday(started_at)),
    CHECK (last_heartbeat_at IS NULL OR julianday(last_heartbeat_at) >= julianday(started_at))
) STRICT;
CREATE INDEX IF NOT EXISTS idx_record_worklog_record_started ON record_worklog(record_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_record_worklog_author_started ON record_worklog(author_user_id, started_at DESC);
CREATE UNIQUE INDEX IF NOT EXISTS idx_record_worklog_one_open_interval ON record_worklog(record_id, author_user_id) WHERE ended_at IS NULL;
CREATE TABLE IF NOT EXISTS record_resource_ref (
    id INTEGER PRIMARY KEY,
    record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE CHECK (record_id > 0),
    provider TEXT NOT NULL CHECK (length(trim(provider)) > 0),
    resource_kind TEXT NOT NULL CHECK (length(trim(resource_kind)) > 0),
    resource_path TEXT NOT NULL CHECK (length(trim(resource_path)) > 0),
    title TEXT,
    position REAL,
    freestyle_data_structure TEXT CHECK (freestyle_data_structure IS NULL OR json_valid(freestyle_data_structure)),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(created_at) IS NOT NULL),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(updated_at) IS NOT NULL),
    sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0),
    origin_organ_id INTEGER REFERENCES organ(id)
) STRICT;
CREATE INDEX IF NOT EXISTS idx_record_resource_ref_record_position ON record_resource_ref(record_id, position, id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_record_resource_ref_identity ON record_resource_ref(record_id, provider, resource_path);
CREATE TABLE IF NOT EXISTS work_metadata (
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
    sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0),
    origin_organ_id INTEGER REFERENCES organ(id)
) STRICT;
CREATE INDEX IF NOT EXISTS idx_work_metadata_owner ON work_metadata(owner_kind, owner_id);
CREATE INDEX IF NOT EXISTS idx_work_metadata_status ON work_metadata(status);
CREATE UNIQUE INDEX IF NOT EXISTS uq_work_metadata_owner ON work_metadata(owner_kind, owner_id);
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
    sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0),
    origin_organ_id INTEGER REFERENCES organ(id)
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS uq_work_subject_app_user ON work_subject(app_user_id) WHERE subject_kind = 'app_user' AND app_user_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS uq_work_subject_organ ON work_subject(organ_id) WHERE subject_kind = 'organ' AND organ_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS uq_work_subject_transfer_party ON work_subject(transfer_party_id) WHERE subject_kind = 'transfer_party' AND transfer_party_id IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS uq_work_subject_remote ON work_subject(subject_kind, remote_base_url, remote_subject_uid) WHERE remote_base_url IS NOT NULL AND remote_subject_uid IS NOT NULL;
CREATE TABLE IF NOT EXISTS work_assignment (
    id INTEGER PRIMARY KEY,
    work_metadata_id INTEGER NOT NULL REFERENCES work_metadata(id) ON DELETE CASCADE,
    work_subject_id INTEGER NOT NULL REFERENCES work_subject(id) ON DELETE CASCADE,
    assignment_kind TEXT NOT NULL DEFAULT 'responsible' CHECK (assignment_kind IN ('responsible', 'observer', 'helper')),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(created_at) IS NOT NULL),
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP CHECK (julianday(updated_at) IS NOT NULL),
    sync_uid TEXT CHECK (sync_uid IS NULL OR length(trim(sync_uid)) > 0),
    origin_organ_id INTEGER REFERENCES organ(id)
) STRICT;
CREATE INDEX IF NOT EXISTS idx_work_assignment_metadata ON work_assignment(work_metadata_id);
CREATE INDEX IF NOT EXISTS idx_work_assignment_subject ON work_assignment(work_subject_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_work_assignment_identity ON work_assignment(work_metadata_id, work_subject_id, assignment_kind);
