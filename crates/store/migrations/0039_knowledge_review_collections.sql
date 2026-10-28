ALTER TABLE knowledge_review RENAME TO knowledge_review_limited;
CREATE TABLE knowledge_review (
    uid TEXT PRIMARY KEY,
    owner_key TEXT NOT NULL,
    kind TEXT NOT NULL CHECK(kind IN ('save','update')),
    content TEXT NOT NULL CHECK(json_valid(content)),
    created_at TEXT NOT NULL,
    applied TEXT CHECK(applied IS NULL OR json_valid(applied))
);
INSERT INTO knowledge_review(uid,owner_key,kind,content,created_at,applied)
SELECT uid,owner_key,kind,content,created_at,applied FROM knowledge_review_limited;
DROP TABLE knowledge_review_limited;
