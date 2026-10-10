CREATE TABLE location_settings (
    record_uid TEXT PRIMARY KEY REFERENCES record(uid) ON DELETE CASCADE,
    controller_uid TEXT NOT NULL REFERENCES record(uid) ON DELETE CASCADE,
    settings_json TEXT NOT NULL CHECK(json_valid(settings_json)),
    updated_at TEXT NOT NULL
);
CREATE INDEX location_settings_controller ON location_settings(controller_uid);
