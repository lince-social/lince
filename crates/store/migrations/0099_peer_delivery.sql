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
