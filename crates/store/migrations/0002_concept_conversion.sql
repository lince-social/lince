-- Lingua unit conversion (blueprint III.1): factor rows between unit concepts.
-- One authoritative row per unordered pair; the inverse direction is derived at
-- read time (a -> b with factor f implies b -> a with factor 1/f). Conversion
-- is only honored within a shared dimension (a common ancestor in
-- concept_parent), enforced in the repository, not here.
CREATE TABLE concept_conversion (
    a_uid  TEXT NOT NULL REFERENCES concept(uid),
    b_uid  TEXT NOT NULL REFERENCES concept(uid),
    factor REAL NOT NULL,
    UNIQUE(a_uid, b_uid)
);
