CREATE TABLE role_policy (
    role_id INTEGER NOT NULL PRIMARY KEY REFERENCES role(id) ON DELETE CASCADE,
    policy TEXT CHECK (
        CASE
            WHEN policy IS NULL THEN 1
            WHEN json_valid(policy) THEN json_type(policy) = 'object'
            ELSE 0
        END
    ),
    revision INTEGER NOT NULL DEFAULT 1 CHECK (revision > 0)
) STRICT;
