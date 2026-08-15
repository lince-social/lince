-- Which Cell executes a Rule (Ontology "Who does the recurring work when an
-- Organ has several Cells", C7; spec in Karma.md §14.1).
--
-- The second of the two independent axes. The first — is the Rule synced? — is
-- ordinary per-Record sync and needs no table. This one is the other: does THIS
-- Cell execute it. Executing is a property of a MACHINE, not of the rule, so
-- the answer differs per Cell over the very same synced Program.
--
-- LOCAL ONLY, and structurally so: `sync_op` carries five tables and this is
-- not one of them. If this row travelled, turning a rule off on the laptop
-- would turn it off on the always-on Cell, which is the exact opposite of what
-- the setting is for.
--
-- ABSENCE MEANS EXECUTE. The table holds only the Cells' deviations from the
-- default, which matters for two reasons. A Program that arrives from another
-- Cell has no row here and therefore runs, which is the behaviour every
-- existing single-Cell Organ already has and must keep. And a row lost with a
-- restored backup fails toward running rather than toward a rule that silently
-- stopped — a rule that runs when you did not expect it is visible in the
-- Ledger, whereas one that quietly does not run is visible nowhere.
CREATE TABLE karma_program_execution (
    program_uid TEXT PRIMARY KEY REFERENCES karma_program(record_uid),
    -- 0 = this Cell holds the rule without executing it. There is deliberately
    -- no third state: "undecided" and "yes" behave identically, so storing
    -- them apart would be a distinction nothing could act on.
    executes    INTEGER NOT NULL CHECK (executes IN (0, 1)),
    -- Free text the owner may leave for themselves. Three Cells and a rule off
    -- on two of them is a configuration nobody remembers the reason for a
    -- month later, and the reason is the part that stops it being undone by
    -- accident.
    note        TEXT,
    updated_at  TEXT NOT NULL
) STRICT;

CREATE INDEX karma_program_execution_off
    ON karma_program_execution(executes, program_uid);
