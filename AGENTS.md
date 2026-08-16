# Tips

- Lince documentation and its task plan are `.lingua` Records in
  `docs/records/`; `docs/` contains no Markdown. Before reading, creating,
  updating, renaming, or deleting `.lingua`, use the `lince-lingua-crud` skill
  in `.agents/skills/lince-lingua-crud/`. It is the shared workflow for Fiote
  and every other agent or harness.
- For this repository's shipped documentation bundle, the live bundle contract
  is `tools/instinct/CONTRACT.txt`; the parser and renderer in
  `crates/engine/src/lingua_file.rs` remain the format authority. Do not copy a
  changing `.lingua` schema into `AGENTS.md`.

# Working alongside other agents

Several agents work in this codebase at once. Nothing coordinates them except
the task Records, so:

- **A task you are given, you assign to yourself** — add
  `@assigned-to [[<your name>|<uid>]]` and `@wip` to its `.lingua` file. That
  edit is a real assignment: File Sync writes the block back to the database.
- **Never remove or reassign somebody else's assignee.** Not enforced by code,
  on purpose — treat it as absolute anyway. If a task is held by another agent
  and you were not told to take it, leave it alone.
- **Before starting, read the folder.** A task with an assignee and `@wip`, or
  a quantity below `-1`, is being worked on by someone else. Note it, skip it,
  and keep to your own part; the tests you run are yours to keep green.
- Quantity is the state, the same ladder the Kanban board reads: `1` done or
  stable, `0` unplanned, `-1` todo, `-2 @wip` in progress.

# Programming Rules

- Instead of cargo build use cargo check.
- Warnings are treated as errors.
- Do not use worktrees.

- **NOBODY USES LINCE YET.** Restated 2026-08-15 as the general rule the one
  below is a special case of. There is no installed base — not of peers, not of
  databases, not of published key material, not of file formats — so
  compatibility is never a constraint on any decision, and it must never be
  offered as a trade-off when presenting design options. Where the cheap
  option and the BEST long-term architecture differ, the reason to pick cheap
  does not exist here. Pick the better one and change whatever it requires.

- No compatibility with older peers. Decided 2026-08-08. Every Lince on the
  network is expected to be on the current build, so nothing gets a transition
  path: bump an ALPN (`lince/sync/1` → `/2`) and old peers hard-cut, which is
  the intended behaviour, not a regression to soften. Do NOT serve two ALPN
  versions side by side, do not add a fallback branch for an older frame or op
  shape, and do not keep a field alive only so an old build can still read it.
  What DOES stay is failing closed: an unrecognised op `kind`, grant version or
  frame type must refuse rather than crash or half-apply. That is already how
  it works — `WireRequest`/`WireResponse` are internally tagged
  (`#[serde(tag = "op")]`), so an unknown verb fails to deserialize and is
  answered with an error instead of being misread as a neighbouring variant.
  Fail closed, then move on; never negotiate down.

- Work closes BEHIND you, not ahead of you. The remaining work lives in
  `docs/records/` as Records — `TODO - Ontology - *.lingua` and the same for
  the other documents, each `@@task` with `quantity: -1`. There are no
  clusters, no codes and no numbering. Take the next open task. `docs/` holds
  no Markdown at all as of 2026-08-16; regenerating any of it is
  `tools/docs/md_to_lingua.js`, and the two checks in `tools/` are what say
  whether the folder is intact. The rules while building:
  - **A bug found while building a later task is fixed where it BELONGS.**
    Go back, fix it there, land it there, and only then carry on. Never work
    around an earlier defect from inside later work — a workaround makes the
    earlier work look finished while leaving the defect to be rediscovered by
    whoever trusts the checkbox.
  - **Expand freely when finishing something honestly demands it.** If a task
    cannot be called done without work nobody listed, add the work and say so
    in the doc. The list is a plan, not a contract.
  - **Advance only when what is behind is clear.** Moving on is a claim that
    everything before is done, not merely started. If something was split out
    or deferred, name it in the doc with the reason — a ticked box that
    quietly means "mostly" is the thing this rule exists to prevent.
  - **Landing a task DELETES its entry**, and what was learned goes into the
    prose above it. A stale entry for built work is worse than no entry.
  - **Write down what you learned where the next person will hit it**, next
    to the code or in the prose above the task, not only in a commit message.

- A feature is not done until a HUMAN CAN USE IT. Every task that adds a
  capability carries the surface that reaches it — a sand panel, a button, a
  screen — in the same task, not in a later one. Tests prove a mechanism is
  correct; they do not let the owner of this project try it, and a backend that
  can only be exercised by `cargo test` cannot be human-tested at all.
  - **The UI ships with the mechanism, not after it.** If a task would land a
    verb, a key, a queue or a policy with no way to reach it from the running
    app, the task is not finished. Split it only if the surface genuinely
    belongs to a different group in the list, and then say so in both places.
  - **"Obvious from the API" is not a surface.** A person opening Lince should
    be able to find the thing without reading Rust.
  - **State the honest empty case.** A panel that shows nothing has several
    meanings ("none yet", "not switched on", "cannot reach anyone") and must
    say which, or it reads as broken — the mistake already made and fixed once
    with the nearby list.
  - Backend-only work is allowed only where there is genuinely nothing to
    show — a schema migration, an index, an internal invariant — and then it
    should be visible through whatever surface its feature already has.

# Business Rules
The web version is a base canvas with building block components called 'Sand'. Whenever you are to create a new Sand that uses a vendored embedded library you must include the required LICENSE and credits, that is to be bundled together with the Sand.
