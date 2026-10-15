CREATE TEMP TABLE feature_seed (feature TEXT, name TEXT, definition TEXT, concept_uid TEXT, role_uid TEXT);
INSERT INTO feature_seed VALUES
('assignment','assigned-to','An ordinary relationship assigning a Record to a Person.','c_01JA0000000000000000000100','vr_01JA0000000000000000000100'),
('containment','part-of','An ordinary relationship placing a Record inside another Record; an optional amount describes its position.','c_01JA0000000000000000000101','vr_01JA0000000000000000000101'),
('thread','thread-of','An ordinary relationship connecting a discussion thread to the Record it discusses.','c_01JA0000000000000000000102','vr_01JA0000000000000000000102'),
('message','message-in','An ordinary relationship connecting a message to its discussion thread.','c_01JA0000000000000000000103','vr_01JA0000000000000000000103'),
('reply','reply-to','An ordinary relationship connecting a reply to an earlier message.','c_01JA0000000000000000000104','vr_01JA0000000000000000000104'),
('reference','references','An ordinary relationship identifying a Record referred to by another Record.','c_01JA0000000000000000000105','vr_01JA0000000000000000000105'),
('call','call-in','An ordinary relationship connecting a call to its discussion thread.','c_01JA0000000000000000000106','vr_01JA0000000000000000000106'),
('group','group-of','An ordinary relationship connecting a group discussion to its owning Record.','c_01JA0000000000000000000107','vr_01JA0000000000000000000107'),
('membership','member-of','An ordinary relationship identifying membership in a group.','c_01JA0000000000000000000108','vr_01JA0000000000000000000108'),
('operator','operated-by','An ordinary relationship identifying the Person operating a Record.','c_01JA0000000000000000000109','vr_01JA0000000000000000000109'),
('ancestry','descendant-of','An ordinary relationship identifying the parent Agent of an Agent.','c_01JA0000000000000000000110','vr_01JA0000000000000000000110'),
('agent','agent','The identity of an automated actor.','c_01JA0000000000000000000111','vr_01JA0000000000000000000111'),
('actor','actor','The identity of an actor.','c_01JA0000000000000000000112','vr_01JA0000000000000000000112'),
('person','person','The identity of a Person.','c_01JA0000000000000000000113','vr_01JA0000000000000000000113'),
('organ','organ','The identity of an Organ.','c_01JA0000000000000000000114','vr_01JA0000000000000000000114');

UPDATE feature_seed SET concept_uid=COALESCE((SELECT MIN(c.uid) FROM concept c JOIN concept_source s ON s.concept_uid=c.uid WHERE s.original_name=feature_seed.name HAVING COUNT(*)=1),concept_uid);
INSERT OR IGNORE INTO concept(uid,canonical_name,created_at) SELECT concept_uid,name,'2026-10-13T00:00:00Z' FROM feature_seed;
INSERT OR IGNORE INTO concept_source(concept_uid,lingua_uid,original_name,original_language,definition) SELECT concept_uid,'g_01JA0000000000000000000000',name,'en',definition FROM feature_seed;
INSERT OR IGNORE INTO lingua_concept(lingua_uid,concept_uid,adopted_at) SELECT 'g_01JA0000000000000000000000',concept_uid,'2026-10-13T00:00:00Z' FROM feature_seed;
INSERT OR IGNORE INTO concept_term(uid,concept_uid,language,name,preferred) SELECT 'ct_' || substr(concept_uid,3),concept_uid,'en',name,1 FROM feature_seed WHERE NOT EXISTS(SELECT 1 FROM concept_term WHERE concept_uid=feature_seed.concept_uid AND language='en' AND preferred=1 AND retired=0);
INSERT INTO vocabulary_role(uid,write_concept_uid) SELECT role_uid,concept_uid FROM feature_seed;
INSERT INTO vocabulary_role_concept(role_uid,concept_uid) SELECT role_uid,concept_uid FROM feature_seed;
CREATE TABLE vocabulary_feature (feature TEXT PRIMARY KEY,role_uid TEXT NOT NULL UNIQUE REFERENCES vocabulary_role(uid));
INSERT INTO vocabulary_feature SELECT feature,role_uid FROM feature_seed;
INSERT INTO vocabulary_feature VALUES('location','vr_01JA0000000000000000000002');
DROP TABLE feature_seed;

