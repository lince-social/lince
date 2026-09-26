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
