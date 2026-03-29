== Self-Provisioning Widgets

This document captures a working agreement for making Lince more generic.

The target is not "make Chess special in Rust".
The target is "let a widget package provision its own backing resources through generic host capabilities".

=== Problem

Today Lince lets a widget do frontend-heavy interaction work, but it does not yet let that widget complete the full lifecycle of its own backend setup in a generic way.

Current behavior is split:

- a package can render arbitrary UI inside the iframe
- a package can read bridge state and local `widgetState`
- a package can call generic backend table and stream endpoints if it already knows which server and view to use
- the board host still expects `serverId` and, for stream widgets, `viewId` to already be configured on the card

That means a widget like Chess is still blocked by host configuration even if all game logic is perfectly valid in frontend JavaScript.

=== Why this matters

This is an important genericity boundary.

If the only way to ship a shared widget is to also add Rust application code that knows that widget by name, then Lince is still feature-coded instead of capability-coded.

The desired direction is:

- widget packages should be able to bring their own interaction logic
- widget packages should be able to persist domain-specific JSON in generic sidecar tables
- widget packages should be able to request creation of their own backing record and view resources
- the host should only provide generic permissions, orchestration, persistence, and safety rails

=== Chess as the motivating example

The Chess widget is a good test because its domain rules are intentionally lightweight.

The important facts are:

- the board UI is frontend-owned
- move application is frontend-owned
- undo is frontend-owned
- synchronization can be modeled as "subscribe to a view, patch one JSON payload"
- durable state fits naturally in `record_extension.data_json`

For Chess, the desired resource model is:

- one `record` for the game identity, title, and presence in the system
- one `record_extension` row for the game state sidecar
- one `view` that streams that record and its sidecar payload

The state payload should remain widget-owned JSON:

```txt
{
  "history": [...],
  "current": {...}
}
```

This is precisely the kind of "gambiarra with discipline" that Lince should permit.

=== What Lince does today

The current host model is still pre-provisioned.

The host persists card-level configuration such as:

- `serverId`
- `viewId`
- `widgetState`
- stream enabled state

The host bridge exposes `widgetState`, but not a generic way for a widget to tell the host:

- "create the resources I need"
- "bind this card to the created server/view"
- "remember that these resources belong to this widget instance"

As a result, a widget that needs SSE must currently arrive with a valid `viewId` or it is treated as misconfigured.

=== What Kanban does today

Kanban only partially solves this problem.

Kanban already proves that Lince can create widget-owned backing views automatically, but it does so through widget-specific Rust orchestration.

Kanban currently:

- requires a base `serverId` and `viewId`
- loads the base view definition
- derives a filtered SQL query
- creates or updates a derived `view`
- stores the derived view identity in `widgetState`

This is useful, but it is not yet generic provisioning.

Kanban still depends on application code that knows:

- what package it is
- how to derive its view
- how to persist its runtime linkage

So the answer to "does Kanban already operate this way?" is:

- not fully
- it already contains the closest existing pattern
- it can be migrated toward this model

=== Agreed direction

The generic direction should be:

- widgets may own their domain interaction logic entirely in frontend code
- widgets may use sidecar rows such as `record_extension` for domain-specific JSON
- widgets may request first-use provisioning of the resources they need
- provisioning must be idempotent
- provisioning must be permission-checked by the host
- provisioning identity must be tied to the widget instance or an explicit shared-resource key
- widgets should not need bespoke Rust services just to exist

This means Chess should not require a dedicated Rust service.

It should require only:

- generic host provisioning
- generic bridge metadata
- generic CRUD and SSE transport

=== Important distinction

There are two possible ways to implement this.

==== Option A: widget-owned raw provisioning from frontend

The widget itself would:

- create a `record`
- create or locate a `record_extension`
- create or locate a `view`
- persist the discovered ids in `widgetState`
- start using SSE and generic table patch routes

This is conceptually simple, but it pushes too much infrastructure logic into each widget.

Problems:

- idempotency becomes each widget author's burden
- naming conventions become inconsistent
- raw SQL view creation becomes duplicated and fragile
- host/card rebinding is still awkward
- permission errors and retries become package-specific

==== Feasible first step

A practical first step is still possible with this model.

The host change would be:

- a widget that declares `read_view_stream` must be allowed to boot with `serverId` set and `viewId = null`
- the board should treat that state as provisionable, not immediately misconfigured

Then the widget can:

- show "Connect to view" and "Start" actions
- create a `record`
- create a `record_extension`
- create a `view`
- switch into normal SSE behavior after a `viewId` is known

This is feasible.
It is the smallest route to a working generic Chess without adding a Chess Rust service.

==== Critique of the self-service path

As a first implementation, this path is good.
As the final model for many widgets, it is weak.

Main criticisms:

- it makes every widget author solve provisioning orchestration alone
- it tends to create orphaned rows and views when a multi-step flow fails midway
- it makes retries and concurrent starts harder to reason about
- it makes naming conventions part of widget code instead of host policy
- it forces widgets to know too much about SQL views and infrastructure layout

Using a human-readable game name plus a 4-digit hash is acceptable as a toy bootstrap key, but weak as a durable identity rule.

Problems with that key:

- collisions are possible
- manual renames can break lookups
- concurrent creators can race
- the widget must trust a string lookup where the create response should be enough

If self-service is used, the widget should strongly prefer:

- using returned insert ids directly when available
- using a longer stable slug for shared discovery
- treating name lookups as fallback, not the primary identity mechanism

==== Option B: host-managed generic provisioning

The widget asks the host to "ensure my resources".

The host then:

- checks permissions
- creates or finds the required rows and views
- stores the resulting linkage in card state
- returns a resolved contract to the widget

This is the better target.

It keeps the host generic while avoiding widget-specific Rust services.

==== Hotel service shape

This model can be thought of as hotel service from the backend to the widget.

The widget should not have to cook the meal, clean the room, and register itself at the desk.
It should be able to arrive, ask for what kind of stay it needs, and receive a ready contract.

In that metaphor:

- the widget is the guest
- the host runtime is the concierge
- the backend CRUD and view system are the kitchen and room service

The guest says:

- I need a shared game resource
- I need a stream for it
- I need write access to its sidecar JSON

The concierge ensures:

- the right resources exist
- duplicates are not created accidentally
- permissions are respected
- the widget receives the resolved ids and ready state

This is still generic infrastructure.
It is not a Chess-specific backend.

=== Minimal generic contract

The host needs a generic instance-aware provisioning contract.

At minimum, the runtime should support something like:

```txt
GET  /host/widgets/{instance_id}/contract
POST /host/widgets/{instance_id}/provision
POST /host/widgets/{instance_id}/actions/{action}
```

The exact route names may change, but the responsibilities should not.

The provisioning contract should be able to answer:

- which server this widget is bound to
- whether it already has provisioned resources
- which `record_id`, `record_extension_id`, and `view_id` belong to it
- whether provisioning is allowed for the current user
- whether the stream is ready

=== Host responsibilities

The host should become responsible for:

- permission checks
- idempotent resource creation
- storing resolved resource identities in `widgetState`
- rebinding the card to the created `viewId` when streaming is required
- exposing a stable contract to the widget iframe

This is not domain code.
This is runtime infrastructure.

=== Widget responsibilities

The widget should remain responsible for:

- rendering its domain UI
- interpreting its own JSON payloads
- applying local interaction rules
- patching its domain JSON back through generic endpoints

For Chess, that means the widget should still own:

- piece selection
- movement
- undo
- promotion
- move history rendering

=== Shared-resource identity

Provisioning must define whether a resource is:

- per widget instance
- per shared key

Chess should probably support both modes eventually.

Examples:

- per widget instance: each dropped chess card creates its own game
- per shared key: multiple cards can intentionally attach to the same game

The first implementation can stay with per widget instance because it is simpler and avoids accidental collisions.

=== View generation

The most sensitive part is view creation.

The system should avoid making every widget author invent raw SQL from scratch in browser code.

Preferred direction:

- the package declares a provisioning recipe or resource template
- the host turns that recipe into concrete `record`, `record_extension`, and `view` operations

This keeps the package portable without making arbitrary SQL generation the default frontend responsibility.

=== Consequences for the board host

The board must stop assuming that a stream widget without `viewId` is simply misconfigured.

It needs a third state:

- provisionable

That state means:

- the widget can boot
- the widget can request provisioning
- the host can show progress, permission denial, or success

The important refinement is:

- `read_view_stream` should mean "this widget can operate on a view stream"
- it should not always mean "this widget already has a valid bound view id before first render"

Without this change, self-provisioning widgets cannot fully start.

=== Consequences for Kanban

Kanban should move in the same direction over time.

It does not need to lose its richer runtime.
But its provisioning logic should stop being a Kanban-only special case.

The generic runtime should own:

- resource creation
- derived view persistence
- instance linkage

Kanban-specific code should own:

- contract validation
- record semantics
- board ergonomics
- domain actions

=== Minimum changes required

This is a moderate architectural change, not a total rewrite.

The main changes are:

- board host: allow stream widgets to boot before `viewId` exists
- bridge/runtime: add a generic provisioning action or instance contract route
- persistence of linkage: store created resource ids in `widgetState`
- provisioning service: generic logic for ensuring widget-owned resources
- package format: declare provisioning intent in a generic, package-owned way

The frontend game logic for Chess is the easy part.
The missing part is generic runtime provisioning.

=== Open questions

These points still need explicit design decisions:

- Should provisioning be triggered by bridge action, HTTP route, or both?
- Should the host rewrite `card.viewId`, or should widgets consume only provisioned ids from `widgetState`?
- How declarative should the package provisioning recipe be?
- How much raw SQL should a package be allowed to declare?
- When a widget is deleted, should its provisioned resources be retained or garbage-collected?
- How should a widget opt into "shared existing game" instead of "new game per card"?

=== Working conclusion

The right long-term move is not "make Chess a Rust feature".

The right long-term move is:

- keep Chess frontend-owned
- make provisioning generic
- treat `record_extension.data_json` as an intentional extension surface
- let widgets become self-provisioning applications running inside a host with capability checks

That is a real step toward making Lince generic.
