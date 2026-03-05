-- Add down migration script here
CREATE TABLE "sum_old" (
    id INTEGER PRIMARY KEY,
    quantity REAL NOT NULL DEFAULT 1
);

INSERT INTO "sum_old" (id, quantity)
SELECT id, quantity
FROM "sum";

DROP TABLE "sum";

ALTER TABLE "sum_old" RENAME TO "sum";
