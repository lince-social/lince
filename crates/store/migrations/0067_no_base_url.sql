UPDATE record SET body = '' WHERE kind = 'organ';

DELETE FROM record_extension WHERE namespace = 'lince.organ';

ALTER TABLE organ_contact DROP COLUMN last_seen_addr;
