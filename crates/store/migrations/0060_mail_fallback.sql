-- When a contact has been unreachable long enough to be worth mailing
-- (Ontology C4, "fall back only after a short retry window").
--
-- A failed dial almost always means "not right now" — a NAT that has not
-- opened, a relay hiccup, a laptop lid — and not "offline". Falling back on
-- the first failure routes every batch through somebody else's disk, stops
-- the direct path from ever being tried in earnest, and bills a carrier for
-- traffic that would have connected a moment later. So the fallback needs to
-- know HOW LONG, and how long is a wall-clock question that no per-pass
-- counter answers: `sync_outbox.attempts` counts passes, and a Cell syncing
-- every thirty seconds and one syncing every ten minutes mean entirely
-- different things by "three attempts".
--
-- Both columns live on the contact rather than on the queued rows because
-- reachability is a property of the PEER, not of any particular op: a contact
-- we could not dial owes nothing per-row, and every batch bound for them
-- shares one answer.
ALTER TABLE organ_contact ADD COLUMN unreachable_since TEXT;

-- When we last left mail for this contact, so a peer that stays down is not
-- re-sealed and re-deposited on every pass. Cleared by the same success that
-- clears `unreachable_since`: once they answer, both facts are stale.
ALTER TABLE organ_contact ADD COLUMN mailed_at TEXT;
