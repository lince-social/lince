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
CREATE INDEX social_ask_query_due ON social_ask_query(state,deadline);
CREATE TABLE social_ask_seen (
    id TEXT PRIMARY KEY,
    binding TEXT NOT NULL,
    source TEXT NOT NULL,
    deadline INTEGER NOT NULL,
    reserved INTEGER NOT NULL CHECK(reserved BETWEEN 1024 AND 197632),
    reply TEXT CHECK(length(CAST(reply AS BLOB)) <= 262144)
);
CREATE INDEX social_ask_seen_source ON social_ask_seen(source,deadline);
