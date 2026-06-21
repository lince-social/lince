CREATE TABLE IF NOT EXISTS permission (
    id INTEGER PRIMARY KEY,
    subject TEXT NOT NULL,
    action TEXT NOT NULL,
    description TEXT,
    UNIQUE (subject, action),
    CHECK (length(trim(subject)) > 0),
    CHECK (length(trim(action)) > 0),
    CHECK (description IS NULL OR length(trim(description)) > 0)
) STRICT;

CREATE TABLE IF NOT EXISTS role_permission (
    role_id INTEGER NOT NULL REFERENCES role(id) ON DELETE CASCADE,
    permission_id INTEGER NOT NULL REFERENCES permission(id) ON DELETE CASCADE,
    PRIMARY KEY (role_id, permission_id),
    CHECK (role_id > 0),
    CHECK (permission_id > 0)
) STRICT;

INSERT INTO permission(subject, action, description)
VALUES
    ('record', 'create', 'Create records'),
    ('record', 'read', 'Read records'),
    ('record', 'update', 'Update records'),
    ('record', 'delete', 'Delete records'),
    ('view', 'create', 'Create views'),
    ('view', 'read', 'Read views'),
    ('view', 'update', 'Update views'),
    ('view', 'delete', 'Delete views'),
    ('view', 'stream', 'Stream views'),
    ('terminal', 'execute', 'Use terminal sessions'),
    ('transfer', 'create', 'Create transfer data'),
    ('transfer', 'read', 'Read transfer data'),
    ('transfer', 'update', 'Update transfer data'),
    ('transfer', 'delete', 'Delete transfer data'),
    ('organ', 'create', 'Create organs'),
    ('organ', 'read', 'Read organs'),
    ('organ', 'update', 'Update organs'),
    ('organ', 'delete', 'Delete organs'),
    ('karma', 'create', 'Create karma rules'),
    ('karma', 'read', 'Read karma rules'),
    ('karma', 'update', 'Update karma rules'),
    ('karma', 'delete', 'Delete karma rules'),
    ('karma', 'execute', 'Execute karma rules'),
    ('user', 'create', 'Create users'),
    ('user', 'read', 'Read users'),
    ('user', 'update', 'Update users'),
    ('user', 'update_self', 'Update own user'),
    ('user', 'delete', 'Delete users'),
    ('user', 'assign_role', 'Assign user roles'),
    ('role', 'create', 'Create roles'),
    ('role', 'read', 'Read roles'),
    ('role', 'update', 'Update roles'),
    ('role', 'delete', 'Delete roles'),
    ('permission', 'read', 'Read permissions'),
    ('permission', 'assign', 'Assign permissions to roles'),
    ('file', 'read', 'List files'),
    ('file', 'upload', 'Upload files'),
    ('file', 'download', 'Download files'),
    ('file', 'delete', 'Delete files'),
    ('configuration', 'create', 'Create configuration'),
    ('configuration', 'read', 'Read configuration'),
    ('configuration', 'update', 'Update configuration'),
    ('configuration', 'delete', 'Delete configuration'),
    ('command', 'create', 'Create commands'),
    ('command', 'read', 'Read commands'),
    ('command', 'update', 'Update commands'),
    ('command', 'delete', 'Delete commands'),
    ('query', 'create', 'Create queries'),
    ('query', 'read', 'Read queries'),
    ('query', 'update', 'Update queries'),
    ('query', 'delete', 'Delete queries'),
    ('frequency', 'create', 'Create frequencies'),
    ('frequency', 'read', 'Read frequencies'),
    ('frequency', 'update', 'Update frequencies'),
    ('frequency', 'delete', 'Delete frequencies'),
    ('package', 'create', 'Create packages'),
    ('package', 'read', 'Read packages'),
    ('package', 'update', 'Update packages'),
    ('package', 'delete', 'Delete packages'),
    ('board', 'read', 'Read board state'),
    ('board', 'update', 'Update board state'),
    ('sand', 'read', 'Read sands'),
    ('sand', 'create', 'Create sands'),
    ('sand', 'update', 'Update sands'),
    ('sand', 'delete', 'Delete sands')
ON CONFLICT(subject, action) DO UPDATE SET description = excluded.description;

INSERT OR IGNORE INTO role_permission(role_id, permission_id)
SELECT role.id, permission.id
FROM role
CROSS JOIN permission
WHERE role.name = 'admin';
