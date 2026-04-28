ALTER TABLE organ RENAME TO organ__integer;
CREATE TABLE organ (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    base_url TEXT NOT NULL
) STRICT;
INSERT INTO organ (id, name, base_url)
SELECT CAST(id AS TEXT), name, base_url
FROM organ__integer
ORDER BY id;
DROP TABLE organ__integer;
