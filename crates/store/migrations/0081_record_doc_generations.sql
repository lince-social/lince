ALTER TABLE record_revision
ADD COLUMN document_generation INTEGER NOT NULL DEFAULT 1
CHECK(typeof(document_generation) = 'integer' AND document_generation > 0);

ALTER TABLE record_doc
ADD COLUMN generation INTEGER
CHECK(generation IS NULL OR (typeof(generation) = 'integer' AND generation > 0));

ALTER TABLE record_doc
ADD COLUMN base_revision INTEGER
CHECK(base_revision IS NULL OR (typeof(base_revision) = 'integer' AND base_revision > 0));
