ALTER TABLE social_publication_job ADD COLUMN receipt TEXT CHECK(receipt IS NULL OR length(CAST(receipt AS BLOB))<=512);
