PRAGMA defer_foreign_keys = ON;

CREATE TEMP TABLE vocabulary_concept_backup AS SELECT * FROM concept;
DELETE FROM concept;
DROP TABLE concept;
CREATE TABLE concept (
    uid TEXT PRIMARY KEY,
    canonical_name TEXT NOT NULL,
    origin_organ TEXT,
    instinct TEXT,
    created_at TEXT NOT NULL
);
INSERT INTO concept SELECT * FROM vocabulary_concept_backup;
DROP TABLE vocabulary_concept_backup;
CREATE INDEX concept_name_lookup_canonical ON concept(canonical_name, uid);

CREATE TABLE concept_term (
    uid TEXT PRIMARY KEY,
    concept_uid TEXT NOT NULL REFERENCES concept(uid) ON DELETE CASCADE,
    language TEXT NOT NULL DEFAULT '',
    name TEXT NOT NULL CHECK(length(name) BETWEEN 1 AND 256),
    owner_uid TEXT REFERENCES record(uid),
    preferred INTEGER NOT NULL DEFAULT 0 CHECK(preferred IN (0, 1)),
    retired INTEGER NOT NULL DEFAULT 0 CHECK(retired IN (0, 1)),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0)
);
CREATE UNIQUE INDEX concept_term_shared_name ON concept_term(concept_uid, language, name) WHERE owner_uid IS NULL AND retired = 0;
CREATE UNIQUE INDEX concept_term_personal_name ON concept_term(concept_uid, language, name, owner_uid) WHERE owner_uid IS NOT NULL AND retired = 0;
CREATE UNIQUE INDEX concept_term_shared_preference ON concept_term(concept_uid, language) WHERE owner_uid IS NULL AND preferred = 1 AND retired = 0;
CREATE UNIQUE INDEX concept_term_personal_preference ON concept_term(concept_uid, language, owner_uid) WHERE owner_uid IS NOT NULL AND preferred = 1 AND retired = 0;
CREATE INDEX concept_term_input ON concept_term(name, owner_uid, retired, concept_uid);

CREATE TABLE concept_term_history (
    term_uid TEXT NOT NULL REFERENCES concept_term(uid) ON DELETE CASCADE,
    revision INTEGER NOT NULL,
    language TEXT NOT NULL,
    name TEXT NOT NULL,
    preferred INTEGER NOT NULL,
    retired INTEGER NOT NULL,
    PRIMARY KEY(term_uid, revision)
);

INSERT INTO concept_term(uid, concept_uid, language, name, preferred)
SELECT 'ct_0' || substr(hex(randomblob(16)), 1, 25), uid, '', canonical_name, 1 FROM concept;
INSERT OR IGNORE INTO concept_term(uid, concept_uid, language, name)
SELECT 'ct_0' || substr(hex(randomblob(16)), 1, 25), concept_uid, lang, name FROM concept_name;
DROP TABLE concept_name;
CREATE VIEW concept_name AS SELECT concept_uid, language AS lang, name FROM concept_term WHERE owner_uid IS NULL AND retired = 0;
CREATE TRIGGER concept_name_insert INSTEAD OF INSERT ON concept_name BEGIN
    INSERT OR IGNORE INTO concept_term(uid, concept_uid, language, name)
    VALUES('ct_0' || substr(hex(randomblob(16)), 1, 25), NEW.concept_uid, NEW.lang, NEW.name);
END;
CREATE TRIGGER concept_name_delete INSTEAD OF DELETE ON concept_name BEGIN
    UPDATE concept_term SET retired = 1, preferred = 0, revision = revision + 1
    WHERE concept_uid = OLD.concept_uid AND language = OLD.lang AND name = OLD.name AND owner_uid IS NULL AND retired = 0;
END;
CREATE TRIGGER concept_term_insert_history AFTER INSERT ON concept_term BEGIN
    INSERT INTO concept_term_history VALUES(NEW.uid, NEW.revision, NEW.language, NEW.name, NEW.preferred, NEW.retired);
END;
CREATE TRIGGER concept_term_update_history AFTER UPDATE ON concept_term WHEN NEW.revision != OLD.revision BEGIN
    INSERT INTO concept_term_history VALUES(NEW.uid, NEW.revision, NEW.language, NEW.name, NEW.preferred, NEW.retired);
END;
INSERT INTO concept_term_history SELECT uid, revision, language, name, preferred, retired FROM concept_term;

CREATE TABLE concept_source (
    concept_uid TEXT PRIMARY KEY REFERENCES concept(uid) ON DELETE CASCADE,
    lingua_uid TEXT NOT NULL REFERENCES lingua(uid),
    original_name TEXT NOT NULL,
    original_language TEXT NOT NULL DEFAULT '',
    definition TEXT NOT NULL DEFAULT '',
    derived_from TEXT REFERENCES concept(uid)
);
INSERT INTO concept_source(concept_uid, lingua_uid, original_name)
SELECT c.uid, COALESCE((SELECT lingua_uid FROM lingua_concept WHERE concept_uid = c.uid ORDER BY adopted_at, lingua_uid LIMIT 1), 'g_local'), c.canonical_name FROM concept c;

CREATE TABLE vocabulary_role (
    uid TEXT PRIMARY KEY,
    write_concept_uid TEXT NOT NULL REFERENCES concept(uid),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0)
);
CREATE TABLE vocabulary_role_concept (
    role_uid TEXT NOT NULL REFERENCES vocabulary_role(uid),
    concept_uid TEXT NOT NULL REFERENCES concept(uid),
    PRIMARY KEY(role_uid, concept_uid)
);

CREATE TABLE record_location_choice (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid) ON DELETE CASCADE,
    source TEXT NOT NULL CHECK(source IN ('direct', 'assertion')),
    assertion_uid TEXT REFERENCES record_assertion(uid),
    revision INTEGER NOT NULL DEFAULT 1 CHECK(revision > 0),
    CHECK((source = 'direct' AND assertion_uid IS NULL) OR (source = 'assertion' AND assertion_uid IS NOT NULL))
);

INSERT OR IGNORE INTO lingua(uid, name, visibility, created_at) VALUES('g_local', 'Local', 'private', '2026-10-11T00:00:00Z');
INSERT INTO lingua(uid, name, visibility, created_at) VALUES('g_01JA0000000000000000000000', 'Lince', 'public', '2026-10-11T00:00:00Z');
INSERT INTO concept(uid, canonical_name, created_at) VALUES('c_01JA0000000000000000000001', 'tópos', '2026-10-11T00:00:00Z');
INSERT INTO lingua_concept VALUES('g_01JA0000000000000000000000', 'c_01JA0000000000000000000001', '2026-10-11T00:00:00Z');
INSERT INTO concept_source VALUES('c_01JA0000000000000000000001', 'g_01JA0000000000000000000000', 'tópos', 'el-Latn', 'The saved geographic location of a Record, expressed by an outgoing ordinary relationship to a named Record with saved coordinates.', NULL);
INSERT INTO concept_term(uid, concept_uid, language, name, preferred) VALUES
('ct_01JA0000000000000000000003', 'c_01JA0000000000000000000001', 'el-Latn', 'tópos', 1),
('ct_01JA0000000000000000000004', 'c_01JA0000000000000000000001', 'en', 'location', 1),
('ct_01JA0000000000000000000005', 'c_01JA0000000000000000000001', 'en', 'address', 0),
('ct_01JA0000000000000000000006', 'c_01JA0000000000000000000001', 'en', 'addres', 0),
('ct_01JA0000000000000000000007', 'c_01JA0000000000000000000001', 'pt', 'localização', 1),
('ct_01JA0000000000000000000008', 'c_01JA0000000000000000000001', 'pt', 'endereço', 0),
('ct_01JA0000000000000000000009', 'c_01JA0000000000000000000001', 'el', 'τοποθεσία', 1);
INSERT INTO vocabulary_role(uid, write_concept_uid) VALUES('vr_01JA0000000000000000000002', 'c_01JA0000000000000000000001');
INSERT INTO vocabulary_role_concept VALUES('vr_01JA0000000000000000000002', 'c_01JA0000000000000000000001');
