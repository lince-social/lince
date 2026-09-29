ALTER TABLE recurrence ADD COLUMN bindings_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(bindings_json));
ALTER TABLE recurrence_revision ADD COLUMN bindings_json TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(bindings_json));
