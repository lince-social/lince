**Access Control: remaining concerns**

Updated 2026-09-16. These are deferred concerns and recommendations, not implemented behavior. Each Actor has at most one assigned Role per Organ; replacing it must remove the previous Role's authority. The owner's decision supersedes the older additive-Role wording in Lince.lingua. Do not add Role inheritance or multiple assigned Roles.

**Management gaps**

- Login management is not the whole Actor model. Keep Person identity, credentials, active standing and Role assignment distinct. Deleting a login keeps its Person Record. Define management of Actors without passwords, devices and other supported Actor kinds before expanding the UI.
- Remove authority tied to the name `admin` before allowing that Role to be renamed or deleted. Protect actual recovery access instead of counting users with that Role name. Restoring an Actor must not revive revoked credentials or devices.
- Define delegated management with an explicit list of assignable Roles; do not attempt to prove arbitrary filter containment. A manager must not strengthen an assignable Role to bypass their own limits.
- The current Auth query does not return editable read rules, their revisions, scoped write grants or effective-access explanations. Expose those through the existing access machinery before building their editors. Permission checkboxes alone do not describe scoped access.
- Save policy changes with expected revisions, dependency information and audit entries together. Authorize them using the authority before the change. Show conflicts and require a fresh review rather than overwriting another manager's edit.

**Consistent enforcement**

Use the same backend checks for direct requests, ordinary Record edits, collaboration and Area effects. Check current authority, attempted properties and assertions, and the complete resulting Record before committing. Reject an entire effect if any part is forbidden. Protect credential, assignment and domain-owned fields from generic editing shortcuts.

The Area apply path currently checks coarse `record:update`, readability, expected state and duplicate requests, then writes quantity and assertions directly. It still needs the scoped before/after authorization used by other Record changes. Review the current source again before implementation; a checkbox or hidden control cannot close that gap.

The existing scoped authorizer requires one grant to cover a complete Record change in both its old and new states. Recommendation: keep that simpler rule initially. Combining grants within one Role is a separate, unresolved proposal; introduce it only for a concrete workflow. It must never mean combining several assigned Roles.

Validate every policy branch, referenced identity and work limit. Missing references, including under negation, must fail closed. Distinguish exact Concept matching from descendant matching. Display sorting, pagination and selected fields are not access rules. Time-dependent rules need explicit expiry behavior.

**Cost of permission checks**

The Auth query still loads all users and Roles and performs extra lookups per user. Add backend paging and batch those lookups if account lists grow; paging the interface alone cannot bound that database work.

Write preparation currently depends on whole metadata snapshots, a default 4,096-Record limit, and loading unrelated policies and personal filters. Do not fix this by merely raising limits or removing validation.

Cache validated rule structure by revision with bounded memory. Use indexed, batched lookups for affected Records and relevant relationships. Read current Role assignment and session state when applying an operation; avoid caching final allow decisions initially. Bound request size and graph work rather than the Organ's total Record count.

Track policy dependencies, including new edges, removed links and changed ancestry. Structural edits can change access to other Records and must be authorized before and after; cache invalidation alone is insufficient. Keep transactions short, measure lock contention, and avoid scanning every Actor or building an Actor-by-Record permission table.

**Restricted and unprivileged workspaces**

The accepted unprivileged workspace grants no extra authority. Separate reading its definition, editing the shared layout and changing its Records. Adding an Area cannot grant its viewers new Record permissions.

Recommend an authorized shared template, a view adapted to the current Actor, and personal layout changes saved only when customized. Keep original Record references and source Organ identity. Do not create a saved clone on every visit or permission change, or silently copy active behavior into personal layouts.

The native workspace model is local, and the current Cell bridge uses a local owner session. Shared workspace identities, admission, revisions and authenticated restricted sessions still need backend support. Choosing a user in the management Sand must not be treated as signing in as that user.

Use an explicit entry rule for the original workspace, with the adapted view as a fallback. Entry cannot promise that every future Area action will succeed: Records, filters and permissions change. Authorize actual operations without comparing everyone's permissions or solving arbitrary filter containment.

A template can reveal private information through literal text, names, assets, query definitions and history. Permission to discover it is not permission to copy its contents. Publish literal template content to an explicit audience, protect Record bindings separately, and filter private data before delivery. Never share the entire local multi-workspace document as a shortcut.

**Area effects and authority**

Sharing an Area does not delegate its author's authority. If Bob moves a Sand while Alice watches, Alice's client must not execute Bob's effect using Alice's greater permissions. Bind effects to an authenticated initiating action. Remote movement, restored layouts and replayed events must not generate new mutations under another viewer.

Keep copied, imported and restored Areas disarmed. Bind arming and pending effects to the Actor, Organ, workspace and configuration revision. Revoke that binding on login, permission or configuration changes. Preserve the existing preview-and-arm interaction without requiring confirmation on every crossing.

One intended transition needs one request identity reused on retry; only its origin submits it. Multiple viewers can otherwise apply successive increments using separate fresh previews. Client geometry is not proof of authority, and server-wide physics simulation is unnecessary for authorizing the requested change.

Keep pending work attached to its original session. Never fall back to the local owner when remote login expires. Preserve source Organs when copying bindings; exclude credentials, drafts, queued changes and armed state. Defer effects spanning several Organs until partial-failure behavior is defined. Show rejected effects clearly and pause them instead of retrying every frame.

**Read privacy and revocation**

Whole-Record reads and property-level writes do not establish property-level read privacy. Queries, document snapshots, collaboration, exports, history, attachments and presence all need the appropriate filtering. Shared title/body documents require a deliberate storage and sync decision before claiming independent field privacy.

Authorize before pagination, counts and aggregates. Use the real evaluator for previews, with bounded samples and explanations that do not disclose hidden data. Recheck when saving.

After revocation, new reads, writes and queued deliveries must use current authority. Invalidate stale subscriptions and pending decisions, including across reconnects. Already delivered bytes cannot be recalled.

**Verification still needed for those changes**

- Compare scoped decisions with a simple reference evaluator. Cover broad reads with narrow writes, Role replacement, category changes, missing references under negation, indirect graph changes, stale policy saves and rejected partial effects.
- Measure checks with 1,000, 10,000 and 100,000 unrelated Records, then grow relevant relationships separately. Track database work, memory, latency and contention; include databases above the current snapshot limit.
- Test revoked access between preview and apply, queued delivery after revocation, Bob's movement observed by Alice, duplicate viewers, disarmed copies, preserved source Organs, and direct requests that bypass the UI.
- Inspect delivered workspace and subscription payloads for private content. Measure entry and idle work while increasing unrelated Actors and Records. Repeated entry must not create extra saved copies.
