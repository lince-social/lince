CREATE TABLE social_service_budget (
    source TEXT NOT NULL,
    direction TEXT NOT NULL,
    window INTEGER NOT NULL,
    bytes INTEGER NOT NULL,
    work INTEGER NOT NULL,
    PRIMARY KEY(source,direction)
);
