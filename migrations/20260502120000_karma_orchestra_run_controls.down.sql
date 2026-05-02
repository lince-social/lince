CREATE TABLE karma_old (
    id INTEGER PRIMARY KEY,
    quantity INTEGER NOT NULL DEFAULT 1,
    name TEXT NOT NULL DEFAULT 'Karma',
    condition_id INTEGER NOT NULL,
    operator TEXT NOT NULL,
    consequence_id INTEGER NOT NULL
);

INSERT INTO karma_old (id, quantity, name, condition_id, operator, consequence_id)
SELECT id, quantity, name, condition_id, operator, consequence_id FROM karma;

DROP TABLE karma;
ALTER TABLE karma_old RENAME TO karma;
