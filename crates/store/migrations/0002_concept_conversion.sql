-- Lingua unit conversion (blueprint III.1): factor rows between unit concepts.
-- One authoritative row per unordered pair; the inverse direction is derived at
-- read time (a -> b with factor n/d implies b -> a with factor d/n). Conversion
-- is only honored within a shared dimension (a common ancestor in
-- concept_parent), enforced in the repository, not here.
--
-- The factor is an exact RATIONAL, not a REAL (blueprint E0.1). `kg -> g` is
-- 1000/1 and stays exact; `kg -> lb` is 45359237/100000000 and stays exact
-- until something asks for a decimal at a declared scale. A REAL factor would
-- make every converted amount approximate the moment it was read, which defeats
-- the exact Ledger it feeds. Both halves are TEXT because they are i128, and
-- both are positive: direction is which side of the pair you read from, never
-- a sign.
CREATE TABLE concept_conversion (
    a_uid       TEXT NOT NULL REFERENCES concept(uid),
    b_uid       TEXT NOT NULL REFERENCES concept(uid),
    numerator   TEXT NOT NULL,
    denominator TEXT NOT NULL,
    UNIQUE(a_uid, b_uid)
);
