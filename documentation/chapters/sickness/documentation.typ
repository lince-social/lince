This is a document about how documentation should function. How the data is to exist in db in seed, how a Sand should function and how much Web or Backend should change for this idea to be implemented. Below is theory that should be very human readable, then below that should be implementation details low in phrases content and high in technical specification and todo tasks -[ ].

=== Theory
I want to make sure there is a Sand component that attends to the following need. My guess is that the Relation Sand is the best use of this, with some tweaks. Maybe the backend needs to be envolved, or maybe i need only a change in the data of the db in the case of a view, know nows.

Documentation should be several Records with category Documentation, and negative Quantity. At the start we have only a Record with head 'Record'. The person will interact with that to learn more by reading the 'Body' of the Record. In that interaction they may signal that this concept has been learned. That will be the changing of the 'Quantity' of that Record to 1. Then all the children of that Record will appear now. The user has unlocked new Records to be seen because their father is now 1. If those children now showed (-1 quantity) are marked as understood their quantity goes to 1.

I think it might be good to receive the full structure in question according to the categories like Documentation, Project1 and the interaction to change the quantities is optimally made in the frontend and updated in the backend. Or not, because the user has a different understanding of another in relation to the documentation, they are in different phases of the process, they should have different data shown to them. If we put that information in the frontend only and they for some reason delete frontend state their progress is lost. So it should probably be stored in db. It should be one for every user. Im thinking of doing now a feature I thought about a long ago. The feature of copy and sync with Karma. Currently Karma can only change quantities of records, i think for this task we need to make sure karma can set a head or a body of a record to be the head or body of another. If we can do that for the backend. In the frontend what i think we need to do is when someone starts their process we grab the original documentation tree of records and duplicate it, assigning all those duplicated records to someone else. The graph Sand will then show the records that have Documentation category and that assignee that is starting. In the duplication process we made the fatherless nodes shown (quantity negative). I think the hidden children should be 0, its not the time to need an understanding of greater concepts when you havent mastered or built the basic ones. I think the best way to do this is having all the data arriving from the view and hiding it or showing it based on their quantities. That way the interaction is very fast and snappy and the updates are optimistic.

I think this envolves making the Relation graph Sand different in these ways:
1. Filter by assignee.
2. Having a Sand to configure the duplication. You filter them set of actions that happen to create new Records in the same quantity as

We can easily do a code like rh for 'Record head' as a Karma Consequence, so that a Karma that is rh30, =, rh10 will make the head of record with id 10 receive the head of record with id 30 just like we do now with rq for record quantity. That can be done to sync the head and body of the original Records that are the official documentation, every N seconds when Karma evals the conditions and consequences the copies that have that karma attached to it will receive the new head and body of the original. In the process of duplication we can create the Karma to point the head and body of the original to the created ones. The assignee then becomes just an information we set, the head and body may change a million ways both in the origin record and in the one that receives its head and body, that will affect by id and not affect assignee. The problem becomes then how do we make the newly created documentation Records duplicate themselves? Because if i copy lets say the original documentation into my own and then make Sand analyze all the Records it received and then make it so all the fatherless are -1 and the rest is 0 i have now my very own trail i can follow to understand the documentation at the starting point, but how do i make the next cards in the documentation appear to me? Do i need to look at a starting original node and getting all the karma that exists that sync with it (a starting node that exists in the copies of the documentation for many users)? But then i need to have karma consequence of 1: Creating a Record (sync can be done with rh head and rb body, rc for category needs to exist too). 2: setting the assignee of that record to be the one from the starting copied record.

We can apply this do the process of doing tasks too, when we do one we unlock new ones. Each person could receive their part of it and only see the tasks they are assigned so its not something overwhelming, so they can focus a lot on making each part well, so they could see the needs as steps, but they could turn off me-mode and see everything.

Just a thought, no need to make a task for this: If record of documentation has quantity i can make a karma to schedule learning them bc they are always 0 when i dont have to see then -1 in friday. Documentation has subjects and areas to learn like production engineering roadmaps and computer science and design.

*Modeled part below, dont touch above*

== Documentation

This document is the agreement note for documentation-as-records, per-user trails, the split between `Trail Relation` and `General Relation`, and the Karma/SQL changes that may support them.

Read `relations.typ` first for the graph widget direction.
Read `aging/karma.typ` first for the current Karma model and its present limits.
Current code is more authoritative than this document when there is conflict.

=== Theory

The canonical documentation should exist as one seeded source tree of records.
Those origin records are not learner state.
They should stay normal records, and for this feature they should stay at quantity `1`.

A learner trail is a duplicated tree assigned to one user.
That trail is their personal progress, so it belongs in the database and not only in frontend state.

For a first version, one learner trail can use three quantity states:

- `-1`: visible and not yet understood
- `0`: hidden or not yet unlocked
- `1`: understood

That means the origin tree can stay at `1`, while copied learner roots start at `-1` and locked copied descendants start at `0`.

The source should own the synced content.
The copied trail should receive synced `head`, `body`, and inherited tree structure from its sync source.
For the trail case, category and assignee should come from the copied root and be spread through the copied descendants.
On creation, categories should be copied from the chosen sync source into the copied root, and the copied root should also carry the `copy` category marker.
If a copied node or copied relation is deleted, sync should repopulate it from the sync source.
That makes the copied tree a synced, source-owned trail rather than a freely editable fork.

This implies two sands with similar looks but different behavior:

- `General Relation`: the generic relation graph/editor
- `Trail Relation`: the per-user documentation trail graph, filtered by assignee and focused on progression instead of free editing

For the first version, the duplicate and sync flow should live inside `Trail Relation`.
The sand should start with no selected root.
It should allow filtering existing trails by name, category, and assignee, then selecting one to inspect or edit.
It should also allow creating a new trail by choosing a sync source root and choosing an assignee.

=== Theory Critique

- Do not hide ownership rules in the UI. `Trail Relation` may allow edits, but it must make clear which fields are source-owned and will be overwritten by sync.
- Do not implement the `sr` Karma family as a pile of one-off token cases. Parse it generically as prefix, scope segment, and field segment.

=== Theory Agreement

- The canonical documentation should be seeded as ordinary `record` rows arranged by parent relations.
- The official origin documentation should stay at quantity `1`.
- Each learner should have an independent duplicated trail stored in the database.
- `Trail Relation` and `General Relation` should be separate sands.
- `Trail Relation` should filter by assignee using the existing `assigned_to` link model.
- `Trail Relation` should start empty, support filtering by name, category, and assignee, and then either open an existing trail or create a new one.
- The frontend may receive one user-specific tree and update optimistically, but the backend remains authoritative.
- The copied trail should be source-owned for synced `head` and `body`, while copied `quantity` remains learner-owned.
- Sync should repair deleted copied nodes and deleted copied relations by recreating them from the sync source tree.
- The assignee and category of the copied root should be treated as trail-owned metadata and should be spread to descendants during sync.
- The user should be allowed to create copy sync from another copied trail as well, not only from the canonical origin tree.
- The chosen trail root does not need to be an absolute graph root. It is only the selected entry root for that trail instance, even if it has parents above it.
- The sync UI should let the user choose scope `node`, `tree`, or `both`, and choose which properties are synced: `quantity`, `head`, and `body`.
- The user should not need to know a raw `view_id` or view name in order to open a trail.
- If categories such as `Documentation` or `Project1` matter, they should use the current category metadata model already exposed by views.
- This same pattern may later help with tasks or onboarding, but that should not distort the first implementation.

=== Implementation Critique

- Do not implement optimistic unlock only in the frontend. The UI should show it instantly, but the backend must persist the parent and child quantity updates together.
- Do not load only graph data for a copied trail. Also load the Karma related to the copied root so the UI can explain the sync source and which fields will or will not be overwritten.
- Do not special-case only `srthb`. The evaluator should understand the generic `sr` combinations.

=== Implementation Agreement

- Use `record_link` with `parent -> record` for hierarchy.
- Use `record_link` with `assigned_to -> app_user` for ownership and filtering.
- Allow one child to have multiple `parent -> record` links.
- Make copied parent-child relations ordinary Lince `record_link:parent->record` rows that mirror the sync source structure, including multiple parents when they exist in the sync source.
- Store direct sync-source mapping explicitly, most likely in `record_extension`, so each copied record knows its `sync_source_record_id`.
- Store enough trail metadata to know which copied tree belongs together. A root trail id or trail root record id is enough if the rest is discoverable from hierarchy.
- Seed the canonical documentation records, their parent relations, their category metadata, and the supporting SQL and Karma rows required by the feature.
- Let `Trail Relation` create a copied root by choosing a sync source root and an assignee.
- On creation, make the copied root start with the same `head` and `body` as the chosen sync source root, but force the copied root quantity to `-1`.
- On creation, copy categories from the chosen sync source to the copied root and add the `copy` category marker.
- During trail sync, derive assignee and category from the copied root and spread them to descendants.
- For the trail case, use `srthb` so content syncs but learner quantity remains owned by the copy.
- When `srthb` creates missing non-root descendants for a trail, initialize them as `0` unless a more explicit rule is later defined.
- If a copied node or relation disappears, tree sync should recreate it from the source.
- Keep learner progress separate from source sync. Sync may update source-owned fields and repair structure, but it should not erase learner `quantity` state except when initializing missing copied nodes.
- `Trail Relation` should allow editing trails, but the UI should disclose that synced `head` and `body` will be overwritten by the sync source.
- `Trail Relation` should handle the optimistic progression action where a node goes from `-1` to `1` and its copied children go from `0` to `-1`, but the backend should persist that coupled change atomically.
- After creating the copied root and the Karma row, `Trail Relation` should call an endpoint that executes that one Karma immediately so the tree appears without waiting for the normal cycle.
- When loading one trail, also load the Karma rows related to that copied root so the UI can show the sync source and the overwritten or preserved fields.
- `Trail Relation` should resolve trails by root record identity, assignee, and categories, not by asking the user for view ids.

=== Karma Direction

The chosen direction is to do this with Karma, but with a tree-scoped consequence family instead of one field consequence per node.

The token grammar should be:

- `sr` prefix
- middle scope segment: `t` for tree, `n` for node, `nt` for both
- trailing field segment in `qhb` order

Examples:

- `srtN`: whole tree using the default field meaning
- `srtqhbN`: explicit whole-tree form
- `srtqN`: quantity-only tree sync
- `srthbN`: head-and-body tree sync for a tree
- `srnqN`: quantity-only sync for one node
- `srntqhbN`: node and tree with explicit field set

For the trail case, the intended token is `srthbN`.
That lets content come from source while copied `quantity` stays controlled by the learner trail.
The chooser in the UI should still be generic enough to construct `srn...`, `srt...`, or `srnt...` variants with any valid `qhb` property subset.

The intended use is:

- one source root
- one shared condition row that points at the source root
- one Karma row per copied trail root
- one `srthb...` consequence per copied trail root

If one source root has three copied trails, that means three Karma rows reusing the same source-side condition and three individual `srthb` consequences.

If the same `sr` family is used on both sides of Karma, the condition-side record id means the sync source root and the consequence-side record id means the copied root.

This is easier than per-node `rh` and `rb` consequences because the tree matcher can own the hard part in one place:

- walking descendants
- matching source nodes to copied nodes
- creating missing copied nodes
- recreating missing copied relations
- syncing `head` and `body` from source to copy
- deriving assignee and category from the copied root and spreading them through the copied tree
- preserving learner `quantity` progress when the copied node already exists

The exact parser and executor semantics still need to be defined carefully, but the architectural point is clear:
if Karma is the chosen mechanism, the consequence should operate on the rooted tree, not on one field at a time.

Field-scoped Karma tokens for record and extension editing may still come later as part of the broader Karma refactor, but the `sr` family is the better first fit for this feature.

=== Sand Direction

- `General Relation` remains the generic graph/editor surface.
- `Trail Relation` is the learner-facing graph for synced documentation trails.
- `Trail Relation` should begin with no selected node.
- `Trail Relation` should allow filtering by name, category, and assignee.
- `Trail Relation` should allow selecting an existing trail and editing it, even if it is someone else's trail.
- `Trail Relation` should show assignee-filtered copied trees, synced source-owned content, learner quantities, and the progression action.
- `Trail Relation` should also contain the creation flow for a new trail: choose the sync source root, choose the assignee, choose sync scope and properties, create the copied root, create the Karma row, then execute that Karma immediately.
- `Trail Relation` should load the Karma related to the copied root and show what is synced and what will be overwritten.
- `Trail Relation` should search records and trail roots, not raw views.
- After the user chooses or creates a trail root, the backend or host should resolve or provision the derived SSE view for that widget instance and return the resolved linkage plus sync-overwrite metadata.
- The trail stream endpoint should return the related Karma metadata when a sync Karma exists for that chosen trail root.
- A broader Karma CRUD sand can still come later, but it is not required for the first version of this feature.

=== View Discovery

The user should not need to know the correct trail `view_id` or view name.
That is host or backend linkage, not user-facing identity.

The stable user-facing identity should be the chosen trail root record.
That chosen root does not need to be an absolute root in the whole graph.

The recommended flow is:

- `Trail Relation` loads in an unconfigured state.
- The sand searches candidate trail roots through the record CRUD endpoint, using assignee, category, and optional head text filters.
- The search returns root records and enough metadata to distinguish them.
- The user chooses one root or creates a new one.
- The backend stores the chosen trail root identity in widget state or in the widget's resolved linkage.
- A trail-specific provision or bind step then resolves or creates the correct derived view internally.
- `/host/widgets/{instance_id}/contract` and `/host/widgets/{instance_id}/stream` then expose and use that resolved linkage.

That means the widget does not discover views directly.
It discovers trail roots, and the host turns that into streamable view linkage.

If the implementation still wants a persistent concrete `view_id`, that id should be returned by the backend after trail selection or provisioning.
It should be diagnostics or linkage data, not something the user has to know ahead of time.

The resources required for this should be explicit:

- record CRUD search support with the filter properties needed for candidate trail-root discovery; if the current endpoint cannot filter by the needed assignee and category shapes, extend it
- one trail-aware resolve or provision step that accepts a chosen trail root record and returns or persists the streamable view linkage
- one contract or diagnostics shape that tells the widget which trail root is bound, which derived `view_id` was resolved, which sync source is active, and what sync scope or fields will overwrite local edits
- the trail stream resource itself should include the related sync Karma metadata when one exists for the chosen trail root

The important point is that these resources are root-record based.
The user discovers roots.
The backend discovers or creates views.

=== End-To-End Spec

This section is the implementation spec from sand to database.

=== Trail Relation Sand

- `Trail Relation` starts in search mode with no bound trail.
- Search uses the record CRUD endpoint and filters by assignee, category, and optional head text.
- The user chooses an existing trail root or creates a new copied root.
- Creation UI lets the user choose sync source root, assignee, sync scope, and synced properties.
- The overwrite warning in the UI is driven by backend sync metadata, not only by local assumptions.
- When a trail is opened, the sand subscribes to the official trail stream and reads the returned rows plus related Karma sync metadata.
- When a node is marked understood, the UI updates optimistically at once, but also sends one backend action that persists the parent and child quantity changes together.

=== Widget Contract And Stream

- `/host/widgets/{instance_id}/contract` should expose:
- bound `trail_root_record_id`
- resolved `view_id` when available
- sync source root identity
- sync scope and synced property set
- overwrite or preserve metadata for `quantity`, `head`, and `body`
- `/host/widgets/{instance_id}/stream` should expose:
- the trail graph rows derived from the chosen trail root
- the related sync Karma metadata for that trail root when such Karma exists
- enough diagnostics to explain current linkage and mismatch states when configuration is incomplete
- The stream payload should stay widget-facing and normalized. It should not require the sand to reverse-engineer raw database internals.

=== Trail Actions

- one action to bind or provision a trail from a chosen root record
- one action to create a copied root, create or reuse sync Karma, and optionally run that Karma once immediately
- one action to run one Karma immediately by id if that generic operation route does not already exist
- one action to progress a copied node from `-1` to `1` and reveal eligible children from `0` to `-1`
- trail actions should return enough metadata for the sand to reconcile local optimistic state with backend truth

=== Application Services

- a trail search service should query records through the record CRUD path using assignee, category, and head filters
- a trail binding or provisioning service should accept a chosen root record and resolve or create the derived view for that widget instance
- a trail stream service should load the derived rows and also load the related sync Karma metadata for the bound trail root
- a trail creation service should:
- create the copied root
- assign the chosen user
- copy categories from the sync source and add the `copy` marker category
- create parent and child copy structure through normal `record_link` rows
- create or reuse the matching Karma rows
- execute that Karma once so descendants appear immediately
- a trail progression service should persist the quantity changes atomically, respecting the rule that a child unlocks only when all of its copied parents are `1`

=== Persistence And Queries

- record search must support filtering by head text, category membership, and assignee membership
- trail resolution must map one chosen trail root record to one derived view linkage for the widget instance
- trail stream loading must join or otherwise fetch the Karma tied to the chosen trail root
- sync Karma lookup must return enough parsed information for the UI to explain:
- which source root is active
- which copied root is active
- which scope is synced
- which properties are synced
- multiple-parent relations must be handled as normal `record_link` rows without reintroducing one-parent assumptions in queries

=== Database Shape

- `record` remains the core row for source and copied nodes
- `record_link` stores:
- `parent -> record` relations, now allowing multiple parents per child
- `assigned_to -> app_user` ownership
- `record_extension` should store copied-node sync metadata such as `sync_source_record_id`
- categories should continue using the existing category metadata model already used by views
- Karma rows should remain ordinary `karma_condition`, `karma_consequence`, and `karma` records, but the `sr` parser and executor should understand generic scope and field combinations
- if a chosen trail root is not an absolute graph root, sync should still walk only downward from that chosen root

=== Implementation Steps

- [ ] Name the two graph sands explicitly in docs and code: `General Relation` and `Trail Relation`.
- [ ] Seed the canonical documentation tree as ordinary `record` rows with parent relations, category metadata, and quantity `1`.
- [ ] Define the copied trail metadata so each copied node stores its `sync_source_record_id`.
- [ ] Define the trail grouping metadata so one copied tree can be treated as one assignee-owned trail.
- [ ] Reuse `record_link:assigned_to->app_user` for trail ownership.
- [ ] Allow one child to have multiple `parent -> record` links across the relevant web contracts and storage assumptions.
- [ ] Create copied parent-child relations as ordinary `record_link:parent->record` rows mirroring the sync source tree, including multiple parents when present.
- [ ] Make `Trail Relation` start with no selected root and support filtering by name, category, and assignee.
- [ ] Add sync configuration controls that let the user choose scope: `node`, `tree`, or `both`.
- [ ] Add sync configuration controls that let the user choose synced properties in `qhb` order: `quantity`, `head`, and `body`.
- [ ] Let `Trail Relation` duplicate a chosen sync source root hereditarily for one assignee.
- [ ] Allow that sync source root to be either a canonical documentation root or an already-synced copied root.
- [ ] In that create flow, create the copied root first with source `head` and `body`, copied quantity `-1`, and the chosen assignee.
- [ ] In that create flow, copy categories from the sync source root to the copied root and add the `copy` marker category.
- [ ] During creation, create or reuse the Karma rows needed to keep that copied trail synced from the source root.
- [ ] Add the first tree-scoped Karma token and consequence family around the generic `sr` grammar.
- [ ] Define the generic `sr` parser as prefix + scope segment + field segment in canonical order.
- [ ] Define `srt`, `srtqhb`, `srtq`, and `srthb` as concrete first variants, with `srthb` as the initial trail-sync consequence.
- [ ] Define the `sr` executor family so it is idempotent, hereditary, and repair-capable.
- [ ] Make `srthb` recreate missing copied nodes and missing copied parent relations from the source tree.
- [ ] Make `srthb` sync source `head` and `body` to the copied tree.
- [ ] Make `srthb` derive assignee and category from the copied root and spread them through the copied tree.
- [ ] Make `srthb` preserve learner `quantity` progress for already-existing copied nodes.
- [ ] Make `srthb` initialize newly created non-root copied descendants to `0`.
- [ ] Decide the exact condition/consequence string semantics for the `sr` family, including how one source root condition fans out into many copied trail consequences.
- [ ] Define multi-parent progression semantics for `Trail Relation`: a child is revealed only when all of its parents in the copied trail have quantity `1`.
- [ ] Refactor `Trail Relation` to filter by assignee and operate on one copied trail.
- [ ] Make `Trail Relation` show clearly which fields are source-owned and will be overwritten by sync.
- [ ] Add the progression action so when one copied node goes from `-1` to `1`, its copied children go from `0` to `-1`.
- [ ] Persist that progression action as one backend operation even if the UI performs it optimistically.
- [ ] If no operation endpoint exists for running one Karma immediately by id, add one and call it right after trail creation.
- [ ] Make the trail load include the Karma related to the copied root so the UI can show sync source and overwritten versus preserved fields.
- [ ] Make the trail stream endpoint return the related sync Karma metadata when such Karma exists for the bound trail root.
- [ ] Make trail discovery root-record based: search candidate roots by assignee, category, and head through record CRUD filters, not by raw view id.
- [ ] Refactor the record get or list endpoint so it can filter by the required category, assignee, and head shapes for trail discovery; if the current endpoint cannot do that, extend it.
- [ ] Add one trail-aware resolve or provisioning step that accepts a chosen trail root record and resolves or creates the derived SSE view for that trail.
- [ ] After a trail root is chosen, have the backend or host resolve or provision the derived SSE view for that widget instance.
- [ ] If a concrete `view_id` is persisted after resolution, expose it through contract or diagnostics instead of asking the user for it.
- [ ] Expose bound `trail_root_record_id`, resolved `view_id`, sync source, and overwritten-versus-preserved sync fields through contract or diagnostics so the widget can explain its current linkage.
- [ ] Expose the same sync metadata through the trail stream response so the sand can render overwrite warnings from backend truth.
- [ ] Make the view contract expose the fields the trail graph needs: `id`, `head`, `body`, `quantity`, category metadata, `parent_id`, `children_json`, assignee summary, and `sync_source_record_id`.
- [ ] Revisit any current one-parent assumptions in `Trail Relation`, `General Relation`, Kanban, and shared web contracts so they do not conflict with the new multi-parent rule.
