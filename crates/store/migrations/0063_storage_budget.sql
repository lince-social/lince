-- C2c: a stated total storage budget per Cell.
--
-- One number, on the singleton configuration row, because the budget is a
-- property of THIS Cell and not of the Organ: a phone and a VPS have no reason
-- to agree, and syncing the number would make the smaller device's limit
-- everybody's limit. The per-area shares are constants in `store::budget`
-- rather than columns — an owner sets how much of their disk Lince may use, not
-- how to divide it internally, and three tunables nobody understands is how a
-- budget becomes an unanswerable question instead of an answer to one.
--
-- Zero means unlimited. It has to be expressible: the whole point of a budget
-- is that the owner decides, and "no ceiling" is one of the decisions.
ALTER TABLE configuration
    ADD COLUMN storage_budget_bytes INTEGER NOT NULL DEFAULT 2147483648;
