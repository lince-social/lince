CREATE TEMP TABLE feature_seed (feature TEXT, name TEXT, definition TEXT, concept_uid TEXT, role_uid TEXT);
INSERT INTO feature_seed VALUES
('participant','participant','An ordinary relationship identifying a Person participating in a conversation.','c_01JA0000000000000000000120','vr_01JA0000000000000000000120'),
('tagging','tagged','An ordinary relationship connecting a Record to a tag Record.','c_01JA0000000000000000000121','vr_01JA0000000000000000000121'),
('call_session','call-session-of','An ordinary relationship connecting a call session to its conversation.','c_01JA0000000000000000000122','vr_01JA0000000000000000000122'),
('recording','call-recording','An ordinary relationship connecting a call recording to its session.','c_01JA0000000000000000000123','vr_01JA0000000000000000000123'),
('transcript','call-transcript','An ordinary relationship connecting a call transcript to its session.','c_01JA0000000000000000000124','vr_01JA0000000000000000000124');

UPDATE feature_seed SET concept_uid=COALESCE((SELECT MIN(c.uid) FROM concept c JOIN concept_source s ON s.concept_uid=c.uid WHERE s.original_name=feature_seed.name HAVING COUNT(*)=1),concept_uid);
INSERT OR IGNORE INTO concept(uid,canonical_name,created_at) SELECT concept_uid,name,'2026-10-13T00:00:00Z' FROM feature_seed;
INSERT OR IGNORE INTO concept_source(concept_uid,lingua_uid,original_name,original_language,definition) SELECT concept_uid,'g_01JA0000000000000000000000',name,'en',definition FROM feature_seed;
INSERT OR IGNORE INTO lingua_concept(lingua_uid,concept_uid,adopted_at) SELECT 'g_01JA0000000000000000000000',concept_uid,'2026-10-13T00:00:00Z' FROM feature_seed;
INSERT OR IGNORE INTO concept_term(uid,concept_uid,language,name,preferred) SELECT 'ct_' || substr(concept_uid,3),concept_uid,'en',name,1 FROM feature_seed WHERE NOT EXISTS(SELECT 1 FROM concept_term WHERE concept_uid=feature_seed.concept_uid AND language='en' AND preferred=1 AND retired=0);
INSERT INTO vocabulary_role(uid,write_concept_uid) SELECT role_uid,concept_uid FROM feature_seed;
INSERT INTO vocabulary_role_concept(role_uid,concept_uid) SELECT role_uid,concept_uid FROM feature_seed;
INSERT INTO vocabulary_feature SELECT feature,role_uid FROM feature_seed;
DROP TABLE feature_seed;

