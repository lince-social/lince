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
CREATE INDEX social_gossip_item_document ON social_gossip_item(kind,document_hash);
CREATE UNIQUE INDEX social_gossip_item_identity ON social_gossip_item(kind,document_hash);
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
CREATE INDEX social_gossip_forward_due ON social_gossip_forward(state,next_attempt);
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
INSERT INTO social_gossip_scan(id,after) VALUES(1,'');
