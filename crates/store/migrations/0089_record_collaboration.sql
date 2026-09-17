CREATE TABLE record_property (
    record_uid TEXT NOT NULL REFERENCES record(uid),
    property TEXT NOT NULL,
    clock INTEGER NOT NULL,
    peer TEXT NOT NULL,
    change_uid TEXT NOT NULL,
    value TEXT NOT NULL,
    PRIMARY KEY (record_uid, property)
);

CREATE TABLE record_change_receipt (
    actor TEXT NOT NULL,
    change_uid TEXT NOT NULL,
    record_uid TEXT NOT NULL,
    payload TEXT NOT NULL,
    result TEXT NOT NULL,
    PRIMARY KEY (actor, change_uid)
);

CREATE TABLE record_doc_delivery (
    record_uid TEXT NOT NULL,
    contact_organ TEXT NOT NULL,
    version TEXT NOT NULL,
    PRIMARY KEY (record_uid, contact_organ)
);

CREATE TABLE record_edit_draft (
    source TEXT NOT NULL,
    record_uid TEXT NOT NULL,
    state TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (source, record_uid)
);
