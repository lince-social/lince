# Karma Orchestra Sand Implementation Plan

## Goal

Add an official Sand named `Karma Orchestra` that visualizes Karma rules as a D3 graph.

The Sand should not work from a normal SQL-backed View. It should require a special `Karma Orchestra View`, matching the existing special view query marker already recognized by the codebase:

```text
karma_orchestra
```

The widget itself will let the user find, select, create, and bind one of those special Views from inside the canvas.

## Current Codebase Facts

Relevant existing pieces:

- Official Sands are registered in `crates/web/src/sand/mod.rs`.
- Relations is packaged at `crates/web/src/sand/relations/*` and vendors `d3.v7.min.js` plus `LICENSE.txt`.
- Trail Relation is packaged at `crates/web/src/sand/trail/*` and has a widget service at `crates/web/src/application/trail_widget.rs`.
- Generic widget routing is in `crates/web/src/presentation/http/api/widgets.rs`.
- Generic package configuration shows the host View picker only when the package manifest includes `read_view_stream`.
- Special View queries are already recognized in:
  - `crates/persistence/src/repositories/view.rs`
  - `crates/persistence/src/repositories/collection.rs`
  - `crates/tui/src/app/logic.rs`
- `karma_orchestra` is already listed as a special View query, and special Views cannot be read through normal SQL `read_snapshot`.
- Karma tables are schema-owned by Rust structs:
  - `karma`
  - `karma_condition`
  - `karma_consequence`
- Current Karma delivery logic is in `crates/application/src/karma.rs`.
- Current delivery evaluates conditions by replacing tokens and calling the expression engine from `crates/application/src/engine.rs`.
- Current delivery is mostly deterministic inside one `karma_deliver` call: it prepares/evaluates the batch, then applies matched consequences in order. Command consequences are the exception because they are spawned with `tokio::spawn`.
- The DB write coordinator serializes writes through one worker, so parallelism today mostly comes from spawned command processes and from multiple external callers, not from one rule-per-task Karma execution.

This means the smallest complete implementation should use `view.query = 'karma_orchestra'` as the View identity and should not try to make this a normal SQL View.

## Proposed Smallest Complete Change Set

### 1. Reserved Karma run controls

Do not add category tables or category columns in this feature. Category work is intentionally out of scope for Karma Orchestra.

Add run-control columns to `karma`:

```text
karma.parallel INTEGER NOT NULL DEFAULT 0
karma.timeout_seconds REAL NOT NULL DEFAULT 0
```

Semantics:

- `parallel` is reserved for future execution control and is not used by this feature.
- `timeout_seconds` is reserved for future execution control and is not used by this feature.
- The execution refactor in this plan must not branch on `parallel` or enforce `timeout_seconds` yet.

Files:

```text
crates/persistence/src/models/karma.rs
crates/domain/src/clean/karma.rs
crates/domain/src/dirty/karma.rs
crates/persistence/src/repositories/karma.rs
crates/application/src/karma.rs
```

Migration:

```text
migrations/<timestamp>_karma_orchestra_metadata.up.sql
migrations/<timestamp>_karma_orchestra_metadata.down.sql
```

The up migration should:

- `ALTER TABLE karma ADD COLUMN parallel INTEGER NOT NULL DEFAULT 0`
- `ALTER TABLE karma ADD COLUMN timeout_seconds REAL NOT NULL DEFAULT 0`

If strict reversibility is required for the reserved run-control fields, the down migration should rebuild `karma` without `parallel` and `timeout_seconds` because SQLite column removal support depends on the embedded SQLite version and repository migration conventions.

### 2. New official package

Add:

```text
crates/web/src/sand/karma_orchestra/mod.rs
crates/web/src/sand/karma_orchestra/body.rs
crates/web/src/sand/karma_orchestra/styles.rs
crates/web/src/sand/karma_orchestra/script.rs
crates/web/src/sand/karma_orchestra/logic.js
```

Register it in:

```text
crates/web/src/sand/mod.rs
```

Use the Relations D3 asset instead of adding another vendored copy:

```rust
assets.insert(
    "d3.v7.min.js".into(),
    include_bytes!("../relations/d3.v7.min.js").to_vec(),
);
assets.insert(
    "LICENSE.txt".into(),
    include_bytes!("../relations/LICENSE.txt").to_vec(),
);
```

Manifest:

```text
filename: karma_orchestra.lince
title: Karma Orchestra
icon: a simple ASCII-safe symbol unless we decide to allow emoji
requires_server: true
permissions:
  - bridge_state
  - write_table
```

Do not include `read_view_stream`. That keeps the generic board config from requiring a normal View selection. The first version should use explicit widget actions only, not `/stream`.

### 3. New package identity helper

Add:

```text
crates/web/src/application/karma_orchestra_identity.rs
crates/domain/src/special_views.rs
```

Expected domain logic:

```rust
pub const KARMA_ORCHESTRA_VIEW_QUERY: &str = "karma_orchestra";

pub fn normalize_special_view_query(query: &str) -> String {
    query.trim().to_lowercase().replace(['-', ' '], "_")
}

pub fn is_karma_orchestra_view_query(query: &str) -> bool {
    normalize_special_view_query(query) == KARMA_ORCHESTRA_VIEW_QUERY
}
```

Expected web package helper logic:

```rust
const KARMA_ORCHESTRA_PACKAGE_ID: &str = "karma_orchestra";

pub(crate) fn is_supported_karma_orchestra_package_filename(package_name: &str) -> bool {
    package_id_from_filename(package_name) == KARMA_ORCHESTRA_PACKAGE_ID
}
```

Wire it into:

```text
crates/domain/src/lib.rs
crates/persistence/src/repositories/view.rs
crates/persistence/src/repositories/collection.rs
crates/tui/src/app/logic.rs
crates/web/src/application/mod.rs
crates/web/src/presentation/http/api/widgets.rs
```

Add `WidgetKind::KarmaOrchestra`.

Use the shared special View query constant and normalizer everywhere the `karma_orchestra` View query is checked. Do not duplicate string normalization rules in each crate.

### 4. New widget service

Add:

```text
crates/web/src/application/karma_orchestra_widget.rs
```

Wire it into:

```text
crates/web/src/application/state.rs
crates/web/src/lib.rs
crates/web/src/presentation/http/api/widgets.rs
```

The service should mirror the small parts of `TrailWidgetService` that are needed:

- resolve the board card by instance id
- validate the package filename
- resolve the configured server
- list Views from `view`
- create Views in `view`
- persist runtime state back into `card.widget_state`
- keep the selected special View in `card.widget_state`, not in `card.view_id`

The selected View should live only in widget state because `card.view_id` is used by normal SQL-backed View workflows. Keeping this binding separate is the simplest architecture and prevents generic host View behavior from trying to treat `karma_orchestra` like a streamable SQL View.

Runtime state key:

```text
karma_orchestra_runtime
```

Runtime shape:

```json
{
  "server_id": "local",
  "view_id": 12,
  "distinctness": "none"
}
```

`distinctness` defaults to `none`, meaning all distinctiveness is off and every Karma row has its own condition triangle unless the user changes the side-panel control.

### 5. Contract and actions

Contract endpoint:

```text
GET /host/widgets/{instance_id}/contract
```

Returned shape:

```json
{
  "widget": {
    "instanceId": "card-id",
    "title": "Karma Orchestra",
    "packageName": "karma_orchestra.lince"
  },
  "source": {
    "serverId": "local",
    "serverName": "Local",
    "requiresAuth": false,
    "authenticated": true
  },
  "binding": {
    "viewId": 12,
    "viewName": "Karma Orchestra"
  },
  "actions": ["list-views", "create-view", "use-view", "load-graph"],
  "dataContract": {
    "specialViewQuery": "karma_orchestra",
    "normalSqlViewsAccepted": false,
    "runControlFields": ["karma.parallel", "karma.timeout_seconds"]
  }
}
```

Actions:

```text
list-views
```

Lists Views where `is_karma_orchestra_view_query(view.query)` is true.

```text
create-view
```

Payload:

```json
{
  "name": "User chosen name"
}
```

The right-bottom modal should provide a short text input for this name and can prefill `Karma Orchestra`. The service creates the View with:

```json
{
  "name": "<payload.name>",
  "query": "karma_orchestra"
}
```

If the name already exists, use `<name> 2`, `<name> 3`, etc.

After creation, bind it immediately.

```text
use-view
```

Validates the selected View exists and is a Karma Orchestra View. If not, return a validation error. This is how "selecting a normal view won't work" is enforced.

```text
load-graph
```

Returns the current computed Karma graph snapshot.

### 6. Data loading behavior

Smallest complete behavior:

- The widget loads the contract.
- The widget opens the right-bottom picker when the user clicks the small canvas button.
- The picker calls `list-views`.
- Once a valid View is bound, the widget calls `load-graph`.

Do not implement `/stream` for the first version. A fake one-shot stream adds API surface without real reactivity. `karma_orchestra` has no SQL dependencies by design, so the existing View subscription path cannot naturally know when to emit updates.

The reusable cross-interface path is the analysis helper itself, not a web-only stream:

- Web calls `load-graph`.
- Other interfaces can call the same application-level Karma analysis helper with their own data loading path.
- Live updates can be added later when special Views have a real dependency/subscription model.

### 7. Karma analysis helpers

Create:

```text
crates/application/src/karma_analysis.rs
```

Keep Karma analysis separate from Karma execution:

- `karma.rs` remains responsible for applying Karma and side effects.
- `karma_analysis.rs` is side-effect-free and assumes data may be broken, incomplete, cyclic, or unsafe.
- Shared token parsing/display helpers can be extracted from `karma.rs`, but execution and analysis should not share side-effecting code paths.
- Karma Orchestra and Karma CRUD validation should call `karma_analysis.rs`.

Add public analysis types:

```rust
pub struct KarmaOrchestraRuleInput {
    pub karma_id: u32,
    pub karma_name: String,
    pub karma_quantity: i32,
    pub karma_parallel: bool,
    pub karma_timeout_seconds: f64,
    pub condition_id: u32,
    pub condition_name: String,
    pub condition_quantity: i32,
    pub condition_code: String,
    pub operator: String,
    pub consequence_id: u32,
    pub consequence_name: String,
    pub consequence_quantity: i32,
    pub consequence_code: String,
}

pub struct KarmaTokenCatalog {
    pub records: BTreeMap<u32, RecordToken>,
    pub commands: BTreeMap<u32, NamedToken>,
    pub queries: BTreeMap<u32, NamedToken>,
    pub frequencies: BTreeMap<u32, NamedToken>,
}

pub struct KarmaExpressionDisplay {
    pub code: String,
    pub human: String,
    pub value: KarmaDisplayValue,
    pub references: Vec<KarmaTokenReference>,
}

pub struct KarmaOrchestraSnapshot {
    pub view_id: u32,
    pub view_name: String,
    pub karma_rows: Vec<KarmaOrchestraRuleInput>,
    pub nodes: Vec<KarmaOrchestraNode>,
    pub links: Vec<KarmaOrchestraLink>,
    pub loops: Vec<KarmaOrchestraLoop>,
    pub check: KarmaCheckReport,
}
```

Execution token behavior should remain compatible with current delivery:

- `rq<ID>` in conditions becomes record quantity.
- `f<ID>` can run `frequency_check`.
- `c<ID>` in conditions can execute the command for numeric output.
- sync tokens can be checked.
- consequences can mutate records, run commands, run queries, or run sync.

Visualization must use a side-effect-free mode:

- `rq<ID>` becomes the current record quantity for numeric evaluation.
- `rq<ID>` human text becomes the record head, falling back to `Record #ID`.
- `f<ID>` becomes the frequency name, falling back to `Nameless Frequency`.
- `c<ID>` becomes the command name, falling back to `Nameless Command`.
- `sql<ID>` becomes the query name, falling back to `Nameless Query`.
- commands, queries, and frequencies are not executed.
- sync tokens are display-only and never execute sync behavior in Karma Orchestra.
- sync token human text should show the sync type plus root node identity, using the root node head and id when available.
- unresolved non-record tokens block full numeric evaluation but should not stop partial evaluation.

Partial evaluation:

- Replace every token that can be resolved without side effects.
- Evaluate every fully numeric contiguous expression segment.
- Do not cross unresolved display-only tokens when reducing math.
- Continue evaluating later numeric segments after an unresolved token.
- If the full expression cannot reduce to one value, display the partially reduced expression as the evaluated value.

Example:

```text
code: rq3 + 1 + c2 _ rq4
known values: rq3 = 1, rq4 = 3, c2 is display-only
partial value: 2 + c2 _ 3
```

This intentionally does not try to apply normal operator precedence across unresolved tokens. The goal is to show everything that can be computed without pretending the blocked part is zero.

Implementation detail:

- Token replacement runs first and produces a symbolic expression.
- Numeric islands are reduced only when every token in that island has a side-effect-free value.
- Unresolved tokens act as barriers.
- Evaluation resumes after a barrier when another numeric island is available.
- If unresolved tokens remain, `KarmaDisplayValue` should carry the partial expression and a flag like `complete: false`.
- If the expression fully reduces to one number, `complete: true` and the numeric value are returned.

### 8. Data loading for graph snapshot

The widget service should not load orphan conditions or consequences. It should join through `karma`.

Local SQL shape:

```sql
SELECT
  k.id AS karma_id,
  k.name AS karma_name,
  k.quantity AS karma_quantity,
  k.parallel AS karma_parallel,
  k.timeout_seconds AS karma_timeout_seconds,
  k.operator AS operator,
  kc.id AS condition_id,
  kc.name AS condition_name,
  kc.quantity AS condition_quantity,
  kc.condition AS condition_code,
  ks.id AS consequence_id,
  ks.name AS consequence_name,
  ks.quantity AS consequence_quantity,
  ks.consequence AS consequence_code
FROM karma k
JOIN karma_condition kc ON kc.id = k.condition_id
JOIN karma_consequence ks ON ks.id = k.consequence_id
ORDER BY k.id;
```

Remote/server-generic path:

- Use table endpoints for `karma`, `karma_condition`, `karma_consequence`, `record`, `command`, `query`, and `frequency`.
- Join in Rust.

Smallest reliable option:

- Use table endpoints for both local and remote through `BackendApiService`/`ManasGateway` helper methods, matching `TrailWidgetService` style.
- Join and analyze in the widget service or in application-level helper functions.

### 9. Evaluation semantics

Condition node:

- `code`: original condition code, such as `rq29 + 1`.
- `human`: token-replaced text, such as `Apple's Qty + 1`.
- `value`: safe expression result, such as `2`.

Consequence node:

- `code`: original consequence code, such as `rq30`.
- `human`: token-replaced text, such as `Orange's Qty`.
- `value`: if it is an `rq<ID>` write consequence and the condition triggers, use the condition value that would be written. Otherwise show `not executed` or `not evaluated`.

Rule activation:

```text
enabled = karma_quantity > 0 && condition_quantity > 0 && consequence_quantity > 0
```

All rows should still be shown, including inactive rows. Inactive rows should not render their condition-to-consequence arrow. Do not mute inactive nodes by color; reserve graying out for unreachable loop segments and similar Karma Check findings.

Operator handling should match current delivery first:

```text
=   triggers when evaluated condition value != 0
=*  always triggers after evaluation/display
```

No other operators are supported for Karma Orchestra in this version. Any other operator should be displayed as unsupported and should not create an active consequence arrow.

### 10. Karma Check, chain, and loop engine

Name this analysis pass `Karma Check`.

`Karma Check` should be a side-effect-free engine used by Karma Orchestra and Karma CRUD validation. It should report loops, nested loops, unreachable branches, missing tokens, and reserved execution metadata. It should not hard-block loops in the first version. The graph should show them clearly, and CRUD flows should require confirmation before persisting loop-creating edits.

Karma analysis and Karma execution have different jobs:

- Analysis assumes the graph may be invalid, incomplete, cyclic, or partially evaluable.
- Execution assumes persisted Karma has passed CRUD validation and focuses on applying side effects safely.

Karma Check findings are potential causal paths. For record quantity consequences, those paths should become real runtime possibilities after the execution refactor below. For commands, queries, frequencies, and sync tokens, Karma Check should stay conservative because the visualization does not execute them.

When Karma Check is used in Karma CRUD flows, loop findings should require explicit confirmation before saving. They should not be silently accepted, and they should not be blocked outright by default.

Build direct rule links:

```text
condition:<condition_id> -> consequence:<consequence_id>
```

Build inferred fulfillment links by simulating record quantity writes:

1. Evaluate every active condition against the current record quantity map.
2. For every active rule whose condition triggers, inspect the consequence.
3. If the consequence is `rq<ID>`, simulate setting record `ID` to the condition value.
4. Re-evaluate every active condition against that simulated record map.
5. If another condition becomes triggered because of that simulated write, add:

```text
consequence:<source_consequence_id> -> condition:<target_condition_id>
```

6. Traverse inferred links with a bounded queue.
7. If the traversal reaches an earlier rule or repeats a record quantity state signature, mark the path as a loop.

Suggested safety limits:

```text
maxDepth = max(8, active_rule_count * 2)
maxStates = max(32, active_rule_count * 4)
```

This engine never writes to the DB and never executes commands, queries, or frequencies.

Current runtime implications before the execution refactor:

- With today's executor, one `karma_deliver` call does not create one task per Karma. It evaluates a batch and applies consequences sequentially.
- Record updates made by Karma consequences use the low-level write wrapper and do not immediately recurse into `deliver_record_karma`.
- Command consequences are spawned asynchronously and can introduce side effects outside the deterministic sequence.

Cascading execution refactor:

- Keep execution single-worker/sequential for this feature.
- Do not spawn one task per Karma.
- Do not enforce `parallel` or `timeout_seconds` yet.
- Include `parallel` and `timeout_seconds` in graph data and Karma Check output as reserved metadata only.
- Replace direct recursive delivery with a bounded work queue.
- When a Karma consequence writes an `rq<ID>` value, enqueue a record-quantity change event for that record id.
- Build or reuse a dependency index from condition expressions to referenced `rq<ID>` tokens.
- For each record-quantity change event, re-evaluate active Karma rules whose conditions reference that record id.
- If one of those rules triggers and writes another `rq<ID>` consequence, enqueue the next record-quantity change event.
- Commands, queries, frequencies, and sync consequences remain side effects but do not create dependency-driven recheck events in this feature.
- Use state signatures and step limits to prevent infinite loops.

Suggested execution guard:

```text
maxCascadeSteps = max(32, active_rule_count * 4)
stateSignature = (rule_id, changed_record_id, new_record_quantity)
```

If the same state signature repeats, stop that cascade branch and report a loop guard finding. Do not let Karma execution hang.

Delivery executor update:

- Extend the clean `Karma` struct and repository selects with `parallel` and `timeout_seconds`.
- Refactor `karma_deliver` so record quantity write consequences can trigger bounded re-evaluation of dependent conditions.
- Keep the existing command/query/sync side-effect behavior unless a change is required to route record quantity writes through the cascade queue.
- Do not branch execution based on `parallel`.
- Do not wrap rule execution in a timeout.

Karma CRUD update:

- On create/update/delete of `karma`, `karma_condition`, or `karma_consequence`, run Karma Check for the affected graph.
- If Karma Check finds loops, return a confirmation-required response.
- If the caller confirms, persist the change.
- Do not silently persist loop-creating edits.
- Do not block all loops by default because intentional loops are part of the model.

Loop detection plan:

- Build a directed graph from conditions, consequences, and inferred consequence-to-condition fulfillment links.
- Treat each unique condition as a single base node, even when multiple Karma rows use it.
- Treat each distinct condition-to-consequence pair as one visual edge; if multiple Karma rows create the same pair, bundle them into one edge with a count/list in metadata.
- Detect strongly connected components first.
- Enumerate simple cycles inside each component up to a cap so overlapping loops and loops-within-loops can both be shown.
- Keep separate loop records when two cycles share only part of the path.
- Mark edges or nodes as `potentiallyUnreachable` when a path enters a cycle that has no deterministic exit before reaching later consequences.
- Do not collapse nested loops into a single warning. Show both the parent loop and the child loop when both can exist.

### 11. Graph node model

Node ids:

```text
condition:<id>
consequence:<id>
```

Node types:

```text
condition
consequence
bridge
```

`bridge` means a consequence fulfills at least one condition. It is rendered as a circle with a triangle inside.

Node shape:

- condition: triangle
- consequence: circle
- bridge: circle with triangle inside

Each node has a north label above the shape:

```text
human readable text
code left        evaluated value right
```

Use SVG with D3 force layout, not canvas, so the label can be implemented as SVG `foreignObject` or grouped text with predictable layout. Relations already proves D3 force usage; Trail already proves SVG+D3 packaging in this repo. SVG is the better fit for triangles, circles, arrows, and structured labels.

Layout ownership:

- Backend/application analysis returns semantic graph data only: rules, nodes, links, loops, and Karma Check findings.
- The D3 client owns positions, ring radius, distinctness mode, loop lane layout, zoom, pan, and visual reorganization.
- Persist only user controls such as selected View and distinctness mode in widget state. Do not persist computed node coordinates in the first version.

Base layout:

- The condition ring is the base structure in every layout mode.
- Base condition nodes should live on a condition ring around the center of the canvas.
- The ring radius should be computed dynamically from the number of rendered ring conditions and the current viewport.
- Every ring condition gets an equal angular slot and the same target radius from the center.
- The layout should use a radial constraint or custom force so conditions tend equally toward their assigned ring positions.
- No single condition should drift into the center or claim more central space than the others.
- Ring conditions start at the top of the circle and proceed clockwise.
- Ring condition color is a grayscale spectrum from black at the first condition to white at the last condition.
- Condition triangles point inward toward the center of the circle.
- Consequences should be placed relative to their source condition, usually outward from that condition's slot.
- For each ring condition, collect every consequence connected through Karma and render every possible condition-to-consequence arrow according to the selected distinctness mode.
- When a condition has many consequences, fan those arrows across a local arc outside that condition's ring slot.
- When multiple Karma rows share the same condition and consequence, keep one visual arrow and expose the contributing Karma rows in metadata.
- Consequences that also fulfill another condition remain bridge nodes, but they should still respect the condition ring as the base visual structure.

Suggested radius rule:

```text
slotSize = 56
neededCircumference = max(1, ringConditionCount) * slotSize
contentRadius = neededCircumference / (2 * PI)
viewportRadius = min(width, height) * 0.42
ringRadius = clamp(contentRadius, 140, viewportRadius)
```

If the viewport is too small for the preferred radius, keep the ring inside the viewport and rely on zoom/pan rather than allowing conditions to collapse into the center.

Distinctness modes:

- `None`: render one condition triangle per Karma row, ordered by `karma.id`; this produces a full black-to-white clockwise spectrum over all Karma lines.
- `Condition`: merge repeated `condition_id` nodes; ring order is the first `karma.id` where each condition appears.
- `Consequence`: keep one condition node per Karma row, but merge repeated consequence nodes; ring order remains by `karma.id`.
- `Both`: merge repeated conditions and repeated consequences; condition ring order is first condition occurrence by `karma.id`, and consequence ordering within each condition uses first consequence occurrence by `karma.id`.

The adjustments side panel should expose this as a segmented switch with the four modes above. Rebuilding the graph after a mode change should preserve the same ring-centered layout rules.

Default mode is `None`.

Loop rendering:

- A simple condition-to-consequence chain should render as a normal directed arrow.
- A consequence that also fulfills another condition should render as a bridge node, a circle with a triangle inside.
- A loop should be laid out as a circular ring or circular lane, not as a tangled force-directed cluster.
- Nodes in a loop should be placed around the ring in cycle order.
- Loop arrows should be curved arc paths around the ring with arrowheads pointing at the next item, matching the "A" case in the reference image.
- Branches that leave a loop should sprout from the node that creates them and return to normal graph layout.
- If a loop node starts another loop, render the child loop as its own ring near the spawning node.
- If two loops overlap, render both loops as separate cycle lanes or concentric/offset arcs instead of hiding one.
- If a child loop can trap execution before the parent loop continues, keep the parent loop visible but gray the parent segment that is potentially unreachable.
- The graph should support the example families shown in the image: a simple chain, a bridge node, a loop with branches, nested loops, and overlapping loops. Those examples are not exhaustive.

### 12. UI behavior

Body structure:

- Full-canvas graph stage.
- Small button inside the canvas at the bottom-right.
- Right-bottom modal/panel opened by that button.
- Small state ball inside the canvas, matching the Relations interaction pattern.
- Adjustments side panel opened by clicking the state ball.
- The side panel controls layout, graph distinctness, physics/radius options if added later, and display metadata.

Adjustments side panel:

- The panel should behave like Relations' state-ball controls panel: click the state ball to open, close button inside the panel, and persisted local/widget state where appropriate.
- Include a `Distinctness` segmented switch with `None`, `Condition`, `Consequence`, and `Both`.
- Default distinctness is `None`.
- Changing distinctness reorganizes nodes immediately without requiring a View reload.
- Include read-only summary pills for total Karma rows, rendered ring conditions, rendered consequences, and loop count.
- The active distinctness mode should be stored in card/widget state so the Sand reopens with the same layout.

Modal behavior:

- On open, call `list-views`.
- If views exist, show them as selectable rows with id and name.
- If no views exist, show "No Karma Orchestra Views found."
- Always show a View name input and a `Create and use` button.
- Selecting a View calls `use-view`, closes the modal, and loads the graph.
- Creating a View calls `create-view` with the input name, closes the modal, and loads the graph.

Initial unbound state:

- Show an empty graph state.
- Keep the bottom-right View button available.
- Do not show a generic host View error if `server_id` is configured.

### 13. Tests and checks

Rust unit tests:

- schema/model tests or compile checks cover `parallel` and `timeout_seconds`
- shared special View query helper normalizes and recognizes `karma_orchestra`
- package identity accepts `karma_orchestra.lince` and `karma_orchestra.html`
- View filtering accepts only `query = 'karma_orchestra'` after normalization
- normal SQL View is rejected by `use-view`
- created Karma Orchestra View uses the user-provided modal name and `query = 'karma_orchestra'`
- selected View is persisted only in `widget_state.karma_orchestra_runtime.view_id`, not `card.view_id`
- token display maps `rq`, `c`, `sql`, and `f` names with fallback labels
- sync tokens display sync type plus root node head/id and do not execute sync behavior
- partial evaluation reduces numeric islands around unresolved display-only tokens
- chain engine detects a consequence that fulfills another condition
- chain engine marks a simple loop
- Karma Check enumerates overlapping and nested loops separately
- Karma Check marks potentially unreachable loop segments
- Karma CRUD path returns confirmation-required when Karma Check detects loops
- cascading execution rechecks dependent conditions after an `rq<ID>` consequence writes a record quantity
- cascading execution stops a repeated state signature instead of recursing forever
- `parallel` and `timeout_seconds` are present in data structures but do not affect execution
- inactive rows remain visible, but inactive condition-to-consequence arrows are omitted
- backend snapshot contains semantic nodes, links, loops, and check findings but no persisted coordinates

Browser-level or JS-level smoke, if time allows:

- import/open Karma Orchestra
- open right-bottom modal
- see empty-state text when no special Views exist
- create a special View
- graph loads without selecting a normal host View
- distinctness modes produce expected node/edge counts for repeated conditions and consequences
- default distinctness is `None`
- rendered conditions get equal slots on the condition ring
- rendered condition ring radius changes from rendered condition count and viewport size
- rendered consequences fan from each source condition
- ring colors progress from black at the first clockwise condition to white at the last
- condition triangles point toward the center after layout and zoom transforms

Verification command:

```text
cargo check
```

Warnings are treated as errors in this repo, so `cargo check` is the compile gate.

## Files Expected To Change

New files:

```text
migrations/<timestamp>_karma_orchestra_metadata.up.sql
migrations/<timestamp>_karma_orchestra_metadata.down.sql
crates/domain/src/special_views.rs
crates/application/src/karma_analysis.rs
crates/web/src/sand/karma_orchestra/mod.rs
crates/web/src/sand/karma_orchestra/body.rs
crates/web/src/sand/karma_orchestra/styles.rs
crates/web/src/sand/karma_orchestra/script.rs
crates/web/src/sand/karma_orchestra/logic.js
crates/web/src/application/karma_orchestra_identity.rs
crates/web/src/application/karma_orchestra_widget.rs
```

Existing files:

```text
crates/web/src/sand/mod.rs
crates/web/src/application/mod.rs
crates/web/src/application/state.rs
crates/web/src/lib.rs
crates/web/src/presentation/http/api/widgets.rs
crates/domain/src/lib.rs
crates/persistence/src/repositories/view.rs
crates/persistence/src/repositories/collection.rs
crates/tui/src/app/logic.rs
crates/application/src/karma.rs
crates/domain/src/clean/karma.rs
crates/domain/src/dirty/karma.rs
crates/persistence/src/models/karma.rs
crates/persistence/src/repositories/karma.rs
```

Potentially existing files if we decide to expose repository methods instead of table-row loading:

```text
crates/persistence/src/repositories/command.rs
crates/persistence/src/repositories/query.rs
crates/persistence/src/repositories/frequency.rs
```

Smallest path changes the Karma repository return shape for the new Karma fields, but avoids changing command/query/frequency repository traits by loading those table rows from the widget service.

## Open Questions

No blocking open questions remain.

Implementation choices now fixed by the plan:

- Category work is removed from this feature.
- Inactive Karma rows are shown, but inactive condition-to-consequence arrows are omitted.
- Visualization evaluation reduces every safe numeric segment and leaves unresolved display-only tokens in the partial value.
- Supported operators are only `=` and `=*`.
- The View creation modal includes a name input.
- Selected Karma Orchestra View binding is stored in `widget_state.karma_orchestra_runtime.view_id`, not `card.view_id`.
- Sync tokens are display-only and show sync type plus root node head/id.
- Karma Check loop findings should require confirmation in Karma CRUD flows, not block by default.
- Condition ring radius is dynamic from rendered condition count and viewport size.
- Default distinctness mode is `None`.

Tunable during implementation, without changing the architecture:

- Exact clamp constants for condition ring radius.
- Exact side-panel copy and summary labels.
- Exact visual styling for unreachable loop segments.

- [ ] Add shared `karma_orchestra` special View query helper.
- [ ] Add `parallel` and `timeout_seconds` Karma fields and migration.
- [ ] Add side-effect-free Karma analysis module.
- [ ] Add Karma Orchestra widget service actions.
- [ ] Add official Karma Orchestra Sand package with D3 layout.
- [ ] Add bounded `rq<ID>` cascading Karma execution.
- [ ] Add Karma CRUD loop confirmation where the current API supports it.
- [ ] Run `cargo check` and fix compile errors.
