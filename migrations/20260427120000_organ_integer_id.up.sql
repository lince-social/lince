ALTER TABLE organ RENAME TO organ__old;
CREATE TABLE organ (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    base_url TEXT NOT NULL,
    CHECK (length(trim(name)) > 0),
    CHECK (length(trim(base_url)) > 0)
) STRICT;
INSERT INTO organ (id, name, base_url)
SELECT 1, name, base_url
FROM organ__old
WHERE lower(trim(id)) = 'local-dev';
INSERT INTO organ (name, base_url)
SELECT name, base_url
FROM organ__old
WHERE lower(trim(id)) != 'local-dev'
ORDER BY id;
DROP TABLE organ__old;
