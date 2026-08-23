# Tips

- Lince documentation, declarations, and its task plan are `.lingua` Records
  in the repository-root `anicca/`. Before reading `.lingua`, use
  `lince-lingua-crud` from `.agents/skills/lince-lingua-crud/`. Writing one is
  the owner's — see "`.lingua` is the owner's" below.
- When `anicca/<Subject>.lingua` exists, consult it before planning from or
  editing `anicca/<Subject>.md`. The owner-authored `.lingua` Record is the
  higher source of truth; the adjacent Markdown holds agent reasoning and must
  be reconciled to the Record, never used to override it silently. If they
  conflict, preserve the `.lingua` decision and call out the Markdown conflict.
- `crates/anicca/src/grammar.rs` is the sole syntax authority. It is a typed
  `rust-sitter` grammar used by the parser, formatter, and checker. The living
  explanation is `anicca/Lingua.lingua`; do not copy its changing schema into
  `AGENTS.md`.
- Instinct ingests the repository-root `anicca/` tree. File Sync is generic:
  its directory is selected through the UI and it has no repository-directory
  special cases.

# Working alongside other agents

Agents and harness sessions coordinate through Lince task Records. Before
coding, use `lince-lingua-crud` to inspect task state, confirm assignment, and
claim work without replacing another assignee. That skill is the canonical
workflow; do not restate it here.

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

# Business Rules
The web version is a base canvas with building block components called 'Sand'. Whenever you are to create a new Sand that uses a vendored embedded library you must include the required LICENSE and credits, that is to be bundled together with the Sand.


# Lince

## No comments in code

Source files carry **no comments**. None — not `//`, not `///`, not `//!`, not
`/* */`. Do not add one, do not restore one, do not "just this once" for a
tricky line.

Naming and structure carry what the code does. Everything else — the reasoning,
the history, the rejected alternative, the cluster a change belongs to — goes in
`anicca/`, which is the project's actual memory and is versioned, linked and
searchable. A comment is a second place for that, and the second place is the
one that goes stale.

Never put planning vocabulary (cluster names, task ids, dates, decisions) into a
source file.

## `.lingua` is the owner's. `.md` beside it is yours

**Never write into a `.lingua` file.** Not prose, not a heading, not a typo
fix, not a checkbox, not a `quantity` — nothing, unless the owner asks for that
exact edit in that exact file. Read them freely.

The reason is Instinct. `anicca/*.lingua` is what Instinct ingests, so it is
read by people who were not in this conversation. It has to sound like one
person's words throughout. Text an agent wrote into it arrives at a reader as
the owner's decision, and a plan half in their voice and half in ours is one
nobody can fully trust.

When the owner wants your words in a Record, they ask, you put the text in
chat, and they paste it. The editing stays theirs.

### Where your own writing goes

`anicca/<Same-Name>.md`, beside the `.lingua` it belongs to. Create them
whenever you want one; they are yours to write and rewrite freely.

Nothing ingests them. `crates/anicca/src/lib.rs` and `crates/engine/build.rs`
each collect `.lingua` and nothing else, so a `.md` in `anicca/` is never
parsed, never a Record, and never reaches Instinct. It sits next to the file it
is about instead of in some far-off folder. (File Sync does have a Markdown
mode; this holds because no synced folder points at `anicca/`.)

Use them the way the banned comment used to be used, and for more than that:
the reasoning, the rejected alternative, an investigation that is still
half-finished, notes to yourself across sessions.

The owner gives one line — one feature. We discuss it and the implications of
implementing it. The owner does the editing. The Records are the truth of the
plan and are decided entirely by them.

When discussing, separate what is **needed for it to work** from what is
**needed for it to be fast**, and say which is which. A slowness nobody has
measured is not yet a problem.
