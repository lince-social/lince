-- no-transaction

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
CREATE INDEX IF NOT EXISTS idx_transfer_party_transfer_role
ON transfer_party(transfer_id, participation_kind);
CREATE INDEX IF NOT EXISTS idx_transfer_party_public_key
ON transfer_party(public_key);

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
CREATE INDEX IF NOT EXISTS idx_transfer_structured_item_transfer_role
ON transfer_structured_item(transfer_id, role);
CREATE INDEX IF NOT EXISTS idx_transfer_structured_item_source_record
ON transfer_structured_item(source_record_id);

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
CREATE INDEX IF NOT EXISTS idx_transfer_interaction_transfer_kind
ON transfer_interaction(transfer_id, interaction_kind);
CREATE INDEX IF NOT EXISTS idx_transfer_interaction_from_item
ON transfer_interaction(from_item_id);
CREATE INDEX IF NOT EXISTS idx_transfer_interaction_to_item
ON transfer_interaction(to_item_id);

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
CREATE INDEX IF NOT EXISTS idx_transfer_agreement_scope
ON transfer_agreement(transfer_id, scope_kind, scope_id);
CREATE INDEX IF NOT EXISTS idx_transfer_agreement_party
ON transfer_agreement(party_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_transfer_agreement_party_scope
ON transfer_agreement(transfer_id, party_id, scope_kind, scope_id);

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
CREATE INDEX IF NOT EXISTS idx_transfer_confirmation_scope
ON transfer_confirmation(transfer_id, scope_kind, scope_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_transfer_confirmation_party_scope_kind
ON transfer_confirmation(transfer_id, party_id, scope_kind, scope_id, confirmation_kind);

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
CREATE INDEX IF NOT EXISTS idx_transfer_structured_settlement_scope
ON transfer_structured_settlement(transfer_id, scope_kind, scope_id);
CREATE UNIQUE INDEX IF NOT EXISTS uq_transfer_structured_settlement_record_scope
ON transfer_structured_settlement(transfer_id, party_id, local_record_id, scope_kind, scope_id);

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
CREATE INDEX IF NOT EXISTS idx_transfer_quantity_influence_record_state
ON transfer_quantity_influence(record_id, influence_state);
CREATE INDEX IF NOT EXISTS idx_transfer_quantity_influence_transfer
ON transfer_quantity_influence(transfer_id);

CREATE TABLE IF NOT EXISTS transfer_message (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    interaction_id INTEGER,
    party_id INTEGER,
    body TEXT NOT NULL CHECK (length(trim(body)) > 0),
    event_id INTEGER,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
) STRICT;
CREATE INDEX IF NOT EXISTS idx_transfer_message_transfer_created
ON transfer_message(transfer_id, created_at);

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
CREATE INDEX IF NOT EXISTS idx_transfer_visibility_subject_kind
ON transfer_visibility_subject(subject_kind);

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
CREATE INDEX IF NOT EXISTS idx_transfer_visibility_rule_subject
ON transfer_visibility_rule(subject_id);
CREATE INDEX IF NOT EXISTS idx_transfer_visibility_rule_scope
ON transfer_visibility_rule(transfer_id, scope_kind, scope_id);

CREATE TABLE IF NOT EXISTS transfer_visibility_field (
    id INTEGER PRIMARY KEY,
    visibility_rule_id INTEGER NOT NULL,
    field_name TEXT NOT NULL CHECK (length(trim(field_name)) > 0),
    visible INTEGER NOT NULL DEFAULT 0,
    editable INTEGER NOT NULL DEFAULT 0,
    redaction_label TEXT
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS uq_transfer_visibility_field_rule_name
ON transfer_visibility_field(visibility_rule_id, field_name);

DELETE FROM transfer_party
WHERE NOT EXISTS (
    SELECT 1 FROM transfer transfer_header
    WHERE transfer_header.id = transfer_party.transfer_id
);
DELETE FROM transfer_structured_item
WHERE NOT EXISTS (
    SELECT 1 FROM transfer transfer_header
    WHERE transfer_header.id = transfer_structured_item.transfer_id
);
DELETE FROM transfer_interaction
WHERE NOT EXISTS (
    SELECT 1 FROM transfer transfer_header
    WHERE transfer_header.id = transfer_interaction.transfer_id
);
DELETE FROM transfer_agreement
WHERE NOT EXISTS (
    SELECT 1 FROM transfer transfer_header
    WHERE transfer_header.id = transfer_agreement.transfer_id
);
DELETE FROM transfer_confirmation
WHERE NOT EXISTS (
    SELECT 1 FROM transfer transfer_header
    WHERE transfer_header.id = transfer_confirmation.transfer_id
);
DELETE FROM transfer_structured_settlement
WHERE NOT EXISTS (
    SELECT 1 FROM transfer transfer_header
    WHERE transfer_header.id = transfer_structured_settlement.transfer_id
);
DELETE FROM transfer_quantity_influence
WHERE NOT EXISTS (
    SELECT 1 FROM transfer transfer_header
    WHERE transfer_header.id = transfer_quantity_influence.transfer_id
);
DELETE FROM transfer_message
WHERE NOT EXISTS (
    SELECT 1 FROM transfer transfer_header
    WHERE transfer_header.id = transfer_message.transfer_id
);
DELETE FROM transfer_visibility_rule
WHERE NOT EXISTS (
    SELECT 1 FROM transfer transfer_header
    WHERE transfer_header.id = transfer_visibility_rule.transfer_id
);

INSERT INTO transfer_party (
    transfer_id,
    participation_kind,
    role_hint,
    actor_label,
    public_key,
    organ_id,
    placeholder,
    created_at,
    updated_at
)
SELECT
    ti.transfer_id,
    'participant',
    'contribution',
    ti.contribution_actor_label,
    ti.contribution_public_key,
    ti.target_organ_id,
    CASE WHEN ti.contribution_public_key IS NULL THEN 1 ELSE 0 END,
    ti.created_at,
    ti.updated_at
FROM transfer_identity ti
JOIN transfer transfer_header ON transfer_header.id = ti.transfer_id
WHERE NOT EXISTS (
    SELECT 1 FROM transfer_party tp
    WHERE tp.transfer_id = ti.transfer_id
      AND tp.role_hint = 'contribution'
      AND tp.actor_label = ti.contribution_actor_label
);

INSERT INTO transfer_party (
    transfer_id,
    participation_kind,
    role_hint,
    actor_label,
    public_key,
    organ_id,
    placeholder,
    created_at,
    updated_at
)
SELECT
    ti.transfer_id,
    'participant',
    'need',
    ti.need_actor_label,
    ti.need_public_key,
    ti.target_organ_id,
    CASE WHEN ti.need_public_key IS NULL THEN 1 ELSE 0 END,
    ti.created_at,
    ti.updated_at
FROM transfer_identity ti
JOIN transfer transfer_header ON transfer_header.id = ti.transfer_id
WHERE NOT EXISTS (
    SELECT 1 FROM transfer_party tp
    WHERE tp.transfer_id = ti.transfer_id
      AND tp.role_hint = 'need'
      AND tp.actor_label = ti.need_actor_label
);

INSERT INTO transfer_structured_item (
    transfer_id,
    role,
    source_record_id,
    owner_party_id,
    title,
    record_head_snapshot,
    quantity,
    location,
    created_at,
    updated_at
)
SELECT
    legacy.transfer_id,
    'contribution',
    CASE WHEN contribution_record.id IS NULL THEN NULL ELSE legacy.contribution_id END,
    party.id,
    legacy.contribution_head,
    legacy.contribution_head,
    legacy.contribution_quantity,
    legacy.location,
    legacy.date,
    legacy.date
FROM transfer_item legacy
JOIN transfer transfer_header ON transfer_header.id = legacy.transfer_id
JOIN transfer_identity ti ON ti.transfer_id = legacy.transfer_id
LEFT JOIN record contribution_record
    ON contribution_record.id = NULLIF(legacy.contribution_id, 0)
LEFT JOIN transfer_party party
    ON party.transfer_id = legacy.transfer_id
   AND party.role_hint = 'contribution'
   AND party.actor_label = ti.contribution_actor_label
WHERE NOT EXISTS (
    SELECT 1 FROM transfer_structured_item item
    WHERE item.transfer_id = legacy.transfer_id
      AND item.role = 'contribution'
      AND (
          (item.source_record_id IS NULL AND contribution_record.id IS NULL)
          OR item.source_record_id = contribution_record.id
      )
);

INSERT INTO transfer_structured_item (
    transfer_id,
    role,
    source_record_id,
    owner_party_id,
    title,
    record_head_snapshot,
    quantity,
    location,
    created_at,
    updated_at
)
SELECT
    legacy.transfer_id,
    'need',
    CASE WHEN need_record.id IS NULL THEN NULL ELSE legacy.need_id END,
    party.id,
    legacy.need_head,
    legacy.need_head,
    legacy.need_quantity,
    legacy.location,
    legacy.date,
    legacy.date
FROM transfer_item legacy
JOIN transfer transfer_header ON transfer_header.id = legacy.transfer_id
JOIN transfer_identity ti ON ti.transfer_id = legacy.transfer_id
LEFT JOIN record need_record
    ON need_record.id = NULLIF(legacy.need_id, 0)
LEFT JOIN transfer_party party
    ON party.transfer_id = legacy.transfer_id
   AND party.role_hint = 'need'
   AND party.actor_label = ti.need_actor_label
WHERE NOT EXISTS (
    SELECT 1 FROM transfer_structured_item item
    WHERE item.transfer_id = legacy.transfer_id
      AND item.role = 'need'
      AND (
          (item.source_record_id IS NULL AND need_record.id IS NULL)
          OR item.source_record_id = need_record.id
      )
);

INSERT INTO transfer_interaction (
    transfer_id,
    interaction_kind,
    direction,
    from_item_id,
    to_item_id,
    from_party_id,
    to_party_id,
    quantity,
    state,
    created_at,
    updated_at
)
SELECT
    legacy.transfer_id,
    'contributes_to',
    'outgoing',
    contribution_item.id,
    need_item.id,
    contribution_party.id,
    need_party.id,
    ABS(legacy.contribution_quantity),
    'proposed',
    legacy.date,
    legacy.date
FROM transfer_item legacy
JOIN transfer transfer_header ON transfer_header.id = legacy.transfer_id
JOIN transfer_identity ti ON ti.transfer_id = legacy.transfer_id
LEFT JOIN transfer_structured_item contribution_item
    ON contribution_item.transfer_id = legacy.transfer_id
   AND contribution_item.role = 'contribution'
LEFT JOIN transfer_structured_item need_item
    ON need_item.transfer_id = legacy.transfer_id
   AND need_item.role = 'need'
LEFT JOIN transfer_party contribution_party
    ON contribution_party.transfer_id = legacy.transfer_id
   AND contribution_party.role_hint = 'contribution'
   AND contribution_party.actor_label = ti.contribution_actor_label
LEFT JOIN transfer_party need_party
    ON need_party.transfer_id = legacy.transfer_id
   AND need_party.role_hint = 'need'
   AND need_party.actor_label = ti.need_actor_label
WHERE NOT EXISTS (
    SELECT 1 FROM transfer_interaction interaction
    WHERE interaction.transfer_id = legacy.transfer_id
      AND interaction.interaction_kind = 'contributes_to'
      AND interaction.from_item_id = contribution_item.id
      AND interaction.to_item_id = need_item.id
);

INSERT INTO transfer_agreement (
    transfer_id,
    party_id,
    scope_kind,
    scope_id,
    agreement_level,
    agreed_item_version,
    created_at,
    updated_at
)
SELECT
    legacy.transfer_id,
    party.id,
    'item',
    item.id,
    legacy.first_agreement,
    item.version,
    legacy.date,
    legacy.date
FROM transfer_item legacy
JOIN transfer transfer_header ON transfer_header.id = legacy.transfer_id
JOIN transfer_identity ti ON ti.transfer_id = legacy.transfer_id
JOIN transfer_party party
    ON party.transfer_id = legacy.transfer_id
   AND party.role_hint = 'contribution'
   AND party.actor_label = ti.contribution_actor_label
JOIN transfer_structured_item item
    ON item.transfer_id = legacy.transfer_id
   AND item.role = 'contribution'
WHERE legacy.first_agreement > 0
  AND NOT EXISTS (
      SELECT 1 FROM transfer_agreement agreement
      WHERE agreement.transfer_id = legacy.transfer_id
        AND agreement.party_id = party.id
        AND agreement.scope_kind = 'item'
        AND agreement.scope_id = item.id
  );

INSERT INTO transfer_agreement (
    transfer_id,
    party_id,
    scope_kind,
    scope_id,
    agreement_level,
    agreed_item_version,
    created_at,
    updated_at
)
SELECT
    legacy.transfer_id,
    party.id,
    'item',
    item.id,
    legacy.second_agreement,
    item.version,
    legacy.date,
    legacy.date
FROM transfer_item legacy
JOIN transfer transfer_header ON transfer_header.id = legacy.transfer_id
JOIN transfer_identity ti ON ti.transfer_id = legacy.transfer_id
JOIN transfer_party party
    ON party.transfer_id = legacy.transfer_id
   AND party.role_hint = 'need'
   AND party.actor_label = ti.need_actor_label
JOIN transfer_structured_item item
    ON item.transfer_id = legacy.transfer_id
   AND item.role = 'need'
WHERE legacy.second_agreement > 0
  AND NOT EXISTS (
      SELECT 1 FROM transfer_agreement agreement
      WHERE agreement.transfer_id = legacy.transfer_id
        AND agreement.party_id = party.id
        AND agreement.scope_kind = 'item'
        AND agreement.scope_id = item.id
  );

PRAGMA foreign_keys=OFF;

DROP TABLE IF EXISTS transfer_event_rebuild;

CREATE TABLE IF NOT EXISTS transfer_event_rebuild (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    transfer_uid TEXT,
    event_uid TEXT,
    actor_label TEXT NOT NULL CHECK (length(trim(actor_label)) > 0),
    actor_public_key TEXT,
    event_kind TEXT NOT NULL CHECK (event_kind IN ('transfer_created', 'transfer_inactivated', 'transfer_quantity_changed', 'item_created', 'item_edited', 'interaction_created', 'interaction_edited', 'visibility_changed', 'agreement_changed', 'message_sent', 'delivery_confirmed', 'receipt_confirmed', 'settlement_applied', 'settlement_reverted', 'dispute_opened', 'dispute_resolved')),
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

INSERT INTO transfer_event_rebuild (
    id,
    transfer_id,
    transfer_uid,
    event_uid,
    actor_label,
    actor_public_key,
    event_kind,
    payload_json,
    previous_event_id,
    previous_event_uid,
    signature,
    validation_state,
    created_at
)
SELECT
    id,
    transfer_id,
    transfer_uid,
    event_uid,
    actor_label,
    actor_public_key,
    event_kind,
    payload_json,
    previous_event_id,
    previous_event_uid,
    signature,
    CASE WHEN signature IS NULL OR actor_public_key IS NULL OR event_uid IS NULL THEN 'pending' ELSE 'valid' END,
    created_at
FROM transfer_event
WHERE EXISTS (
    SELECT 1 FROM transfer transfer_header
    WHERE transfer_header.id = transfer_event.transfer_id
);

DROP TABLE transfer_event;
ALTER TABLE transfer_event_rebuild RENAME TO transfer_event;

PRAGMA foreign_keys=ON;
