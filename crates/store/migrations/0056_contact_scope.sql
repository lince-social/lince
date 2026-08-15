-- Per-contact field narrowing (Ontology §12, cluster C5).
--
-- `sync_out` was a boolean: send this contact everything, or nothing. This is
-- the scope that turns it into a question with a useful answer — WHICH columns
-- of the Records they can already see.
--
-- NULL means unnarrowed, which is what every existing contact means and what
-- the boolean meant before. It is deliberately NOT the empty list: an empty
-- list is a real and different answer ("nothing but the identifying columns"),
-- and conflating "not configured" with "configured to nothing" is how a
-- migration silently stops someone's sync.
--
-- The value is a JSON array of column names, in the vocabulary
-- `protein::Protein.fields` uses — the SAME selector that decides what a sand
-- renders. One language, learned once, rather than a query language and a
-- sharing language kept laboriously in step.
ALTER TABLE organ_contact ADD COLUMN scope_fields TEXT;

-- Bumped whenever `scope_fields` changes, so a WIDENING is detectable.
--
-- This exists because the catch-up cursor never goes back. Adding a column to
-- a contact's scope leaves every op for it already below their version vector,
-- so the field would stay permanently blank for them — the change would look
-- applied and do nothing. A version that moves is what lets the serve path
-- notice and re-snapshot.
ALTER TABLE organ_contact ADD COLUMN scope_version INTEGER NOT NULL DEFAULT 0;

-- The same question in the other direction: `sync_in` was a blank cheque —
-- accept everything this contact sends, or nothing. This is WHICH columns we
-- are willing to take from them.
--
-- It is not a duplicate of `scope_fields` and must never be collapsed into
-- one setting. Outbound is a privacy control (what they learn about us);
-- inbound is an integrity one (what they can change about our copy of the
-- world). The two have different reasons to be narrow and no reason to agree,
-- and a contact we send everything to is routinely one we accept little from.
--
-- Same encoding, same vocabulary, same three states: NULL is unnarrowed, the
-- empty list is a real answer, and both are read by the one predicate in
-- `sync_ops::op_in_scope` so the two directions cannot drift apart on the
-- edges that matter (deletes always pass; the Loro document carries head and
-- body together).
ALTER TABLE organ_contact ADD COLUMN accept_fields TEXT;
ALTER TABLE organ_contact ADD COLUMN accept_version INTEGER NOT NULL DEFAULT 0;
