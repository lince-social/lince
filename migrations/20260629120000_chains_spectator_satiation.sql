-- Transfer chains: private cross-Transfer quantity flow links (organ-local, never in packages)
CREATE TABLE IF NOT EXISTS transfer_chain_link (
    id INTEGER PRIMARY KEY,
    organ_id INTEGER REFERENCES organ(id) ON DELETE CASCADE,
    upstream_transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    upstream_item_id INTEGER REFERENCES transfer_structured_item(id) ON DELETE SET NULL,
    downstream_transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    downstream_item_id INTEGER REFERENCES transfer_structured_item(id) ON DELETE SET NULL,
    amount_kind TEXT NOT NULL CHECK (amount_kind IN ('constant', 'percentage')),
    amount_value REAL NOT NULL,
    state TEXT NOT NULL DEFAULT 'pending' CHECK (state IN ('pending', 'triggered', 'canceled')),
    triggered_at TEXT CHECK (triggered_at IS NULL OR julianday(triggered_at) IS NOT NULL),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    sync_uid TEXT UNIQUE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_chain_link_upstream ON transfer_chain_link(upstream_transfer_id, state);
CREATE INDEX IF NOT EXISTS idx_chain_link_downstream ON transfer_chain_link(downstream_transfer_id);
CREATE INDEX IF NOT EXISTS idx_chain_link_organ ON transfer_chain_link(organ_id);

-- Spectator: watch a source Transfer's role, get satiated when any instance settles
CREATE TABLE IF NOT EXISTS transfer_spectator (
    id INTEGER PRIMARY KEY,
    watcher_organ_id INTEGER REFERENCES organ(id) ON DELETE CASCADE,
    watcher_record_id INTEGER NOT NULL REFERENCES record(id) ON DELETE CASCADE,
    watched_source_transfer_uid TEXT NOT NULL,
    watched_role TEXT NOT NULL CHECK (watched_role IN ('contribution', 'need')),
    amount_kind TEXT NOT NULL CHECK (amount_kind IN ('constant', 'percentage')),
    amount_value REAL NOT NULL,
    state TEXT NOT NULL DEFAULT 'active' CHECK (state IN ('active', 'triggered', 'canceled')),
    triggered_at TEXT CHECK (triggered_at IS NULL OR julianday(triggered_at) IS NOT NULL),
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    sync_uid TEXT UNIQUE
) STRICT;

CREATE INDEX IF NOT EXISTS idx_spectator_source ON transfer_spectator(watched_source_transfer_uid, watched_role, state);
CREATE INDEX IF NOT EXISTS idx_spectator_organ ON transfer_spectator(watcher_organ_id);

-- Satiation policy on configuration (global default)
ALTER TABLE configuration ADD COLUMN transfer_satiation_policy TEXT NOT NULL DEFAULT 'none'
    CHECK (transfer_satiation_policy IN ('none', 'first_completes'));

-- Satiation policy on individual Transfers (NULL = inherit from parent, then global)
ALTER TABLE transfer_identity ADD COLUMN satiation_policy TEXT
    CHECK (satiation_policy IS NULL OR satiation_policy IN ('none', 'first_completes'));
