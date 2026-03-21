# `/consult` Endpoint Notes

This document explains the current `/consult` mount, why it is awkward, and the main ways to evolve it.

## Current Situation

The merged HTTP process currently serves two different HTTP surfaces:

- the new web UI at the root namespace
- the older backend HTML/API surface mounted under `/consult`

Today that means:

- web UI pages and assets live at `/`, `/ai`, `/api/*`, `/static/*`
- the older backend API and Datastar HTML live at `/consult/*`
- the current stable backend CRUD and SSE endpoints are therefore externally reachable as:
  - `/consult/api/auth/login`
  - `/consult/api/table/{table}`
  - `/consult/api/table/{table}/{id}`
  - `/consult/api/sse/view/{view_id}`

The web crate’s remote gateway currently defaults to:

- `http://127.0.0.1:6174/consult`

so that web-side integrations call the backend through `/consult/api/...`.

## What Is Wrong With `/consult`

### 1. The public contract and the mounted contract diverge

Logically, the backend contract is being designed as:

- `/api/auth/login`
- `/api/table/...`
- `/api/sse/view/...`

But the actual mounted contract is:

- `/consult/api/auth/login`
- `/consult/api/table/...`
- `/consult/api/sse/view/...`

That difference leaks into:

- docs
- tests
- frontend integration code
- reverse proxy config
- user expectations

This increases confusion for no real domain benefit.

### 2. `/consult` is an implementation detail, not a domain concept

`/consult` is not a business boundary like:

- `/api`
- `/files`
- `/auth`
- `/views`

It exists only because two route trees currently collide.

That makes it feel temporary, but the longer it stays, the more code starts depending on it.

### 3. The web app now depends on a legacy mount point

The web UI is the default UI, but some of its integration code still targets the backend through:

- `/consult/api/...`

That means the new default UI depends on the old surface shape as an internal tunnel.

This is backwards architecturally:

- the stable backend should be primary
- legacy/consultation routes should adapt to it, not the other way around

### 4. It creates two meanings of “API”

Right now:

- `/api/*` belongs to the web host app
- `/consult/api/*` belongs to the backend CRUD/SSE contract

So one server exposes two unrelated API namespaces with different responsibilities.

That makes integration harder to reason about:

- which `/api/auth/login` is the real login?
- which `/api` is stable?
- which `/api` should external clients consume?

### 5. It makes future frontend refactors harder

If the frontend later wants to directly call the stable backend CRUD endpoints, it either has to:

- know about `/consult`
- or you must re-route everything again

That means `/consult` is likely to cause churn later, not prevent it.

### 6. It complicates deployment and reverse proxy rules

If you later expose this through nginx or another reverse proxy, `/consult` becomes one more routing rule to explain and maintain.

It is not fatal, but it adds complexity that is purely accidental.

## What Already Happened

The old backend raw SQL route has now been removed from the backend stable API surface.

The stale web-side proxy route that targeted it has also been removed:

- `/api/integrations/manas/sql`

So the remaining backend integration dependency is now mostly about:

- login
- view SSE
- future table CRUD integration

## Possible Solutions

## Option 1. Keep `/consult` temporarily

Use this if the immediate goal is to keep moving without another routing refactor yet.

### Shape

- root remains owned by the web UI
- legacy backend stays at `/consult`
- web integration code keeps calling `/consult/api/...`

### Pros

- minimal immediate changes
- low risk
- preserves the current default UI
- lets you continue refactoring backend endpoints first

### Cons

- keeps the conceptual mismatch alive
- keeps docs and runtime paths different
- makes `/consult` more entrenched over time

### Recommendation

Acceptable as a short-lived transition state only.

## Option 2. Move the stable backend API back to root `/api`

This is the cleanest direction if the backend API is meant to stabilize now.

### Shape

- stable backend endpoints move to `/api/...`
- web host-specific endpoints move somewhere else, for example:
  - `/web-api/...`
  - `/host-api/...`
  - `/ui-api/...`
- old consultation pages, if still needed, can stay at `/consult/...`

### Pros

- the stable API gets the obvious canonical path
- external clients and docs become simple
- web UI can consume the same stable API without special casing
- `/consult` becomes truly optional

### Cons

- requires renaming the current web app’s own `/api/*` namespace
- requires patching frontend fetch paths

### Recommendation

This is the best long-term HTTP shape if the table CRUD and SSE API is the core contract.

## Option 3. Move the web UI under `/web`

This is the inverse routing choice.

### Shape

- backend API owns `/api/...`
- backend consultation or HTML can own `/consult/...` if desired
- web UI pages/assets/API move to `/web`, `/web/static`, `/web/api`

### Pros

- backend API gets the clean root namespace
- makes it explicit that the web UI is one client among others
- easy mental model for external integrations

### Cons

- the default app URL becomes `/web`
- less pleasant if the web UI is intended to be the primary product surface

### Recommendation

Architecturally clean, but worse if you specifically want the new web UI at `/`.

## Option 4. Split host API and backend API inside the same process

This is similar to Option 2, but more explicit in naming.

### Shape

- backend API:
  - `/api/auth/login`
  - `/api/table/...`
  - `/api/sse/view/...`
- web host/session/package/terminal endpoints:
  - `/host/auth/...`
  - `/host/board/...`
  - `/host/packages/...`
  - `/host/terminal/...`

### Pros

- removes ambiguity
- preserves the web UI at `/`
- keeps one binary and one port
- separates “product backend API” from “local web host UI support API”

### Cons

- requires a moderate rename pass through the web frontend

### Recommendation

This is probably the best compromise if you want:

- web UI at `/`
- stable backend API at `/api`
- no extra subdomain or separate process

## Option 5. Separate processes or subdomains

### Shape

- web host UI on one port or subdomain
- stable backend API on another port or subdomain

Example:

- `app.example.com`
- `api.example.com`

### Pros

- strongest separation of concerns
- no route collisions
- simpler conceptual ownership

### Cons

- more deployment and ops complexity
- more cross-origin considerations
- unnecessary if one process is otherwise fine

### Recommendation

Probably more than you need right now.

## What I Recommend

The best next target is:

### Recommended target shape

- keep the web UI at `/`
- move the stable backend contract to `/api/...`
- move the web host-specific endpoints to a separate prefix such as `/host/...`
- keep `/consult/...` only for old Datastar pages if you still want them for reference

That gives:

- stable backend API where everyone expects it
- clean frontend/backend separation
- no dependency on a temporary consultation prefix
- no need to move the primary web UI away from root

## Suggested Migration Path

1. Keep `/consult` only as a temporary compatibility mount.
2. Decide a prefix for web-host-only endpoints.
   Recommended:
   - `/host/...`
3. Move backend CRUD/auth/SSE to canonical `/api/...`.
4. Patch the web frontend to call:
   - `/api/...` for stable backend resources
   - `/host/...` for board/package/terminal/widget-bridge host functionality
5. Once the frontend no longer needs `/consult`, remove the backend dependency on that mount.
6. Keep `/consult` only if you still want the old HTML surface for reference.

## Short Version

`/consult` works as a temporary routing shim, but it is not a good stable public boundary.

The main problem is that the real backend contract is being hidden behind a legacy prefix because the web UI currently owns `/api`.

The best long-term fix is:

- backend contract at `/api`
- web-host-only endpoints at another prefix
- `/consult` kept only for legacy/reference pages or removed entirely later
