CREATE TABLE role (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

INSERT INTO role(name)
SELECT 'admin'
WHERE NOT EXISTS (
    SELECT 1 FROM role WHERE name = 'admin'
);

INSERT INTO role(name)
SELECT 'lince'
WHERE NOT EXISTS (
    SELECT 1 FROM role WHERE name = 'lince'
);

ALTER TABLE app_user
ADD COLUMN role_id INTEGER REFERENCES role(id);

UPDATE app_user
SET role_id = (
    SELECT id FROM role WHERE name = 'lince'
)
WHERE role_id IS NULL;
