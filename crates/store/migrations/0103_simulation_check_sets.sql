CREATE TABLE simulation_check_set (
    uid TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    revision INTEGER NOT NULL CHECK(revision > 0),
    checks_json TEXT NOT NULL CHECK(json_valid(checks_json))
) STRICT;
