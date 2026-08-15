# Tips

- The docs are markdown files in docs/ directory. 

# Programming Rules

- Instead of cargo build use cargo check.
- Warnings are treated as errors.
- Do not use worktrees.

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

- Clusters close BEHIND you, not ahead of you. The remaining work is
  organised into clusters (`docs/Ontology.md`, "Implementation clusters, in
  order"), and the rule while building them is:
  - **A bug found in a later cluster is fixed in the cluster that owns it.**
    Go back, fix it there, land it there, and only then carry on. Never
    work around an earlier cluster's defect from inside a later one — a
    workaround makes the earlier cluster look finished while leaving the
    defect to be rediscovered by whoever trusts the checkbox.
  - **Expand a cluster freely when completing it demands it.** If a cluster
    cannot honestly be called done without work nobody listed, add the work
    and say so in the doc. The cluster list is a plan, not a contract, and a
    cluster that ships incomplete costs far more than one that grew.
  - **Advance only when the clusters behind are clear.** Moving on is a claim
    that everything before is done, not merely started. If something was
    split out or deferred, name it in the doc with the reason — a ticked box
    that quietly means "mostly" is the thing this rule exists to prevent.
  - **Write down what you learned where the next person will hit it**, next
    to the code or in the cluster entry, not only in a commit message.

- A feature is not done until a HUMAN CAN USE IT. Every box that adds a
  capability carries the surface that reaches it — a sand panel, a button, a
  screen — in the same box, not in a later one. Tests prove a mechanism is
  correct; they do not let the owner of this project try it, and a backend that
  can only be exercised by `cargo test` cannot be human-tested at all.
  - **The UI ships with the mechanism, not after it.** If a box would land a
    verb, a key, a queue or a policy with no way to reach it from the running
    app, the box is not finished. Split it only if the surface genuinely
    belongs to a different cluster, and then say so in both places.
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
