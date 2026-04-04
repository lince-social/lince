= Sandbox Website Mode

This document captures an opt-in Lince mode for serving a preset collection of workspaces as a public interactive surface.

The current name here is `sandbox website mode` because it matches the current discussion better than overloading the normal web host vocabulary.

The scope here is not "replace the normal website".
The scope is:

- keep the old website
- add a second interactive published surface
- allow local browser play on top of a server-published preset
- keep the normal authenticated web/API host separate

== What Is Agreed

- This is a host mode, not a widget trick.
- This should be compile-time gated behind a feature flag if binary bloat matters.
- Using the same source tree for two binaries is acceptable.
- The normal web/API binary stays broad.
- The sandbox binary stays narrow.
- The public mode starts from a server-published preset.
- The browser may keep a local overlay on top of that preset.
- There should be an explicit "reset to published preset" action.
- SSE is allowed for a narrow allowlisted set of bindings declared by the preset.
- Other Lince backend services and endpoints should not be available in the restricted mode.
- The old website remains.

Suggested deployment split:

- `sandbox.lince.social`: restricted preset mode
- `manas.lince.social`: normal API-only or authenticated web/API host

This split is better than trying to keep one public router broad while asking widgets to behave politely.

== Binary And Feature Direction

The corrected direction is:

- feature flags, not just runtime configuration
- same repository
- separate binaries or feature-shaped entrypoints

That matters because runtime disablement does not meaningfully control binary composition on its own.

The project may still share most source code.
The important part is that the restricted binary should not accidentally ship broad host behavior just because it is "turned off in config".

== Primary Model

The model is:

1. server publishes a preset
2. browser loads that preset as initial state
3. browser may alter that state locally
4. browser may reset back to the published preset
5. only explicitly allowed SSE bindings may reconnect automatically

That means the public mode is not ordinary board persistence.
It is closer to a published scene plus a local overlay.

== Persistence Model

Agreed direction:

- published preset from server
- local overlay in `localStorage`
- explicit reset action

This is better than partial persistence rules such as "save scroll but not widgets" because partial rules become incoherent quickly.

The browser should be able to answer three questions clearly:

- what is the published state?
- what is my local overlay?
- how do I return to the published state?

== Required New Artifact

The current workspace archive shape is not enough because it only models one workspace.

The restricted mode needs a new artifact, conceptually something like:

- preset metadata
- active workspace
- ordered workspace list
- cards for each workspace
- package references or embedded package payloads
- allowlisted SSE bindings
- optional preset version

This should not be disguised as "just export one more workspace".

The real object is a published board collection, not a single workspace.

For now the useful name is something like:

- `BoardCollectionArchive`
- or `SandboxPresetArchive`

The core requirement is:

- export all workspaces
- edit the result in normal mode
- re-package it
- publish it into sandbox mode

== Required Route Policy

This is the main non-negotiable constraint.

The restriction must live in the host router, not in widget convention.

The public mode should expose only:

- the initial HTML/bootstrap route
- static assets
- local overlay mechanisms that never hit the server
- the allowlisted SSE route or routes
- whatever bridge/bootstrap support is required to keep the front-end viable without broad host APIs

The public mode should not expose:

- board state persistence routes
- generic package catalog routes
- generic DNA package routes
- server management routes
- terminal routes
- AI builder routes
- generic widget action routes
- arbitrary Lince integration endpoints

The public mode is only credible if the forbidden endpoints are absent or reject access.

== Why The Normal Board Client Cannot Be Reused Unchanged

The issue is not only the widgets.

The current board client itself assumes broad host access.
In current code, the board front-end persists board state through the host API, reads local package catalog endpoints, reads DNA package endpoints, and reads other broad host surfaces.

So the statement is:

- the normal board client may still be reused in pieces
- the normal board client cannot be reused unchanged

The unchanged client would try to call routes that the sandbox binary should not mount.

That means sandbox mode needs at least one of:

- a forked front-end entrypoint
- feature-gated client branches
- a stronger bootstrap contract that disables those code paths cleanly

This is not a theoretical complaint.
It is a direct consequence of the current front-end assuming ordinary board persistence and package-management access.

== Why Widget-Level Discipline Is Not Enough

Widgets are not trusted policy objects.

A preset may embed arbitrary HTML widgets.
If those widgets run on the same origin and broad host routes are still mounted, then "please do not call those endpoints" is not a security model.

So the mode must assume:

- widgets may try to fetch what they can reach
- route policy is the real wall
- preset validation is still useful even after route sealing

This also answers the simplified argument:

- "if a widget assumes whole backend access in sandbox mode, it just will not work"

That is acceptable as a product outcome.
It is not acceptable as the only enforcement layer.

The host still must refuse access.

== SSE Policy

The SSE exception should be narrower than:

- "all views are readable if a widget asks"

Better rule:

- a preset declares its allowed live bindings
- only those bindings may open
- bindings are read-only
- bindings are not general backend capability tokens

The widget should not be able to widen the binding by editing its own host metadata or by manually changing `view_id`.

The host should validate the requested binding against the published preset.

That means:

- the preset fixes which stream binding belongs to which card or widget instance
- the public front-end does not expose UI for changing that binding
- manual client edits still do not widen access because host validation rejects them

This keeps the mode focused on published live surfaces instead of silently becoming a second authenticated web client.

== Bridge Direction

Some host bridge or bootstrap support is still acceptable in sandbox mode if it only serves front-end viability.

Examples:

- initial board bootstrap
- instance metadata needed for layout or widget identity
- local-only overlay synchronization
- narrow read-only binding metadata

This is different from exposing general host services.

The bridge is acceptable when it helps the front-end remain interactive without turning the sandbox binary into a broad backend.

== Edit, Build, Publish

The public mode and the editor should remain different concerns.

The stronger direction is:

- broad mode stays broad
- sandbox mode stays sealed
- publishing is a deliberate export step from broad mode into restricted mode

That implies a validator at publish time.

The validator should reject or strip cards that need forbidden capabilities.

Examples of cards that should be treated carefully:

- cards that depend on generic host actions
- cards that depend on server CRUD
- cards that assume unrestricted `/host` availability
- cards imported from DNA without restricted-mode compatibility

The point is not that every incompatible widget is forbidden everywhere.
The point is:

- incompatible widgets may exist in broad mode
- incompatible widgets should not silently be published into sandbox mode

== Blog And Website Content

Agreement here stays intentionally small for now.

The old website remains.
This mode is additional.

Migrating home page widgets, blog widgets, and other content widgets may happen later.
For now the important part is the host mode and its policy boundary.

This is the right priority because content conversion is not the hard problem.
The hard problem is sealing the runtime surface while keeping the front-end viable.

== Criticisms And Rejected Shortcuts

The following shortcuts should be rejected:

- "just let widgets promise not to call Lince endpoints"
- "permissions metadata alone is enough"
- "runtime disabled means no binary bloat"
- "single-workspace export is basically the same thing"
- "the normal board client can stay unchanged"
- "if an unrestricted widget breaks in sandbox mode, that alone is enough protection"
- "hiding the UI to change `view_id` is enough"

Reasons:

- widget HTML is executable code, not a trusted declaration
- metadata helps validation but does not replace router enforcement
- only compile-time structure meaningfully affects binary composition
- a published preset is structurally broader than one workspace archive
- the current board front-end already assumes broader host APIs
- broken widgets are acceptable, unauthorized host access is not
- SSE binding must be validated at the host, not only hidden in the UI

== Criticisms Of The Remaining Ambition

The remaining ambition is reasonable, but a few costs should still be stated plainly.

This mode is good for:

- interactive published scenes
- playful local overlays
- personalized content surfaces
- narrow live dashboards

This mode is weaker for:

- frictionless reuse of every existing widget
- pretending the current board client needs no adaptation
- broad host capabilities under the same public binary

That does not make the mode wrong.
It means the project should treat it as a deliberate second product surface, not as a nearly-free toggle.

== Minimal Sound Slice

The smallest sound implementation is:

- feature-gated restricted binary
- new all-workspaces preset artifact
- sealed router with no generic host API surface
- one or more allowlisted read-only SSE bindings
- host validation of fixed widget-to-stream bindings
- local overlay in `localStorage`
- explicit reset to published preset
- publish-time preset validation
- front-end branches or a sandbox-specific entrypoint so the client does not try to call forbidden routes

Anything weaker than that risks turning the mode into an unrestricted host with polite language around it.
