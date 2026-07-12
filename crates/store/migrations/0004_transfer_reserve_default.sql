-- Blueprint V.3: the five old reservation policies collapse into
-- promise.reserve_from, whose default comes from the bundle's transfer.
-- NULL = inherit the global default ('active').
ALTER TABLE transfer ADD COLUMN reserve_default TEXT;
