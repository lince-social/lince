CREATE TABLE IF NOT EXISTS transfer_item (
    transfer_id INTEGER NOT NULL,
    contribution_user_id INTEGER NOT NULL,
    contribution_server_id INTEGER NOT NULL,
    contribution_id INTEGER NOT NULL,
    contribution_head TEXT NOT NULL,
    contribution_quantity REAL NOT NULL,
    need_user_id INTEGER NOT NULL,
    need_server_id INTEGER NOT NULL,
    need_id INTEGER NOT NULL,
    need_head TEXT NOT NULL,
    need_quantity REAL NOT NULL,
    first_agreement INTEGER NOT NULL DEFAULT 0,
    second_agreement INTEGER NOT NULL DEFAULT 0,
    date TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    location TEXT NOT NULL
);
