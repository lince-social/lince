CREATE TABLE terminology_context (
    owner_key TEXT PRIMARY KEY,
    context TEXT NOT NULL CHECK(length(context) BETWEEN 1 AND 64)
);

CREATE TABLE terminology_choice (
    owner_key TEXT NOT NULL,
    context TEXT NOT NULL CHECK(length(context) BETWEEN 1 AND 64),
    concept_uid TEXT NOT NULL REFERENCES concept(uid) ON DELETE CASCADE,
    term_uid TEXT REFERENCES concept_term(uid) ON DELETE SET NULL,
    wording TEXT CHECK(wording IS NULL OR length(wording) BETWEEN 1 AND 256),
    original INTEGER NOT NULL DEFAULT 0 CHECK(original IN (0,1)),
    PRIMARY KEY(owner_key,context,concept_uid),
    CHECK((original=1 AND wording IS NULL) OR (original=0 AND wording IS NOT NULL))
);

CREATE TABLE terminology_hidden (
    owner_key TEXT NOT NULL,
    context TEXT NOT NULL CHECK(length(context) BETWEEN 1 AND 64),
    term_uid TEXT NOT NULL REFERENCES concept_term(uid) ON DELETE CASCADE,
    PRIMARY KEY(owner_key,context,term_uid)
);
