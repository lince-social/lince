-- Blueprint XIII.2: the attention budget is a hard user-owned config. Notify
-- effects beyond the daily budget park in the digest instead of interrupting.
ALTER TABLE configuration ADD COLUMN attention_budget_per_day INTEGER NOT NULL DEFAULT 12;
