UPDATE concept_term
SET retired = 1, preferred = 0, revision = revision + 1
WHERE uid = 'ct_01JA0000000000000000000006'
  AND concept_uid = 'c_01JA0000000000000000000001'
  AND owner_uid IS NULL
  AND name = 'addres'
  AND retired = 0;
