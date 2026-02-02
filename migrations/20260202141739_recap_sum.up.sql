-- Add up migration script here
CREATE TABLE "sum_new" (
    id INTEGER PRIMARY KEY,
    quantity REAL NOT NULL DEFAULT 1,
    record_id INTEGER,
    interval_relative BOOLEAN,
    interval_length TEXT,
    sum_mode INTEGER,
    end_lag TEXT,
    end_date DATETIME);

INSERT INTO "sum_new" (id, quantity)
SELECT id, quantity
FROM "sum";

DROP TABLE "sum";

ALTER TABLE "sum_new" RENAME TO "sum";
