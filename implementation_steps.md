# API Auth, Single Writer, and View SSE Implementation Steps

## Goal

Implement a JSON API under `/api` with:

1. Authentication based on username/password and JWT.
2. A single runtime SQLite writer path shared by HTTP, Karma, and any future clients.
3. Write-driven invalidation using SQLite hooks instead of periodic polling.
4. SSE endpoints that stream updates for saved Lince Views.

This document describes the architecture we agreed on before implementation.

## Main Decisions

1. Keep HTML/Datastar transport separate from the JSON API.
2. Use `.env` for `JWT_SECRET`.
3. Store password hashes, never plain passwords.
4. Make `/api/auth/login` the only unauthenticated API endpoint.
5. Make runtime writes converge into one internal `WriteCoordinator`.
6. Use SQLite hooks on that one writer connection.
7. Base SSE subscriptions on saved Lince `view` rows, not arbitrary SQL.
8. Avoid continuous polling as the primary invalidation mechanism.

## Why This Architecture

The important shift is from a polling model to a write-driven invalidation model.

Polling model:
- each subscriber wakes up periodically
- checks if something changed
- most wakeups do no useful work

Write-driven invalidation model:
- nothing happens while the database is idle
- when a write happens, the writer knows immediately
- the writer determines which tables changed
- only affected subscriptions reread their Views

This is cleaner and cheaper for SQLite because it aligns with the natural "many readers, one writer" shape of the database.

## Core Rule

All runtime writes must go through the same writer connection.

This rule matters because SQLite hooks are connection-scoped, not database-global. If some other runtime connection writes directly to the database, the hook-based invalidation path will miss that change.

That means:
- HTTP writes go through the coordinator
- Karma writes go through the coordinator
- future TUI/CLI/local automation writes should go through the coordinator
- read-only endpoints can still use separate read connections

## SSE Clarification

The SSE endpoints should be based on Lince's existing saved `view` rows.

That means the subscription target is:
- a Lince `view.id`
- whose SQL lives in `view.query`

The SSE endpoint is not meant to subscribe to:
- arbitrary SQL sent by the client
- HTML fragments
- "whatever is visible on screen" as the first version

Recommended first read endpoint:

- `GET /api/sse/view/{view_id}`

What the client gets:
- the current result of that saved View
- later pushes when the result of that same View changes

This keeps the read API aligned with the application's existing query model.

## Proposed Runtime Components

### AuthService

Responsibilities:
- verify username/password
- issue JWT
- validate JWT

Input:
- username
- password

Output:
- authenticated subject
- JWT token

### WriteCoordinator

Responsibilities:
- own the single runtime SQLite writer connection
- receive write requests from transports and internal services
- execute writes serially
- use SQLite hooks to capture changed tables
- emit invalidation events after commit

Input:
- write requests from HTTP
- write requests from Karma
- write requests from future clients

Output:
- write result
- invalidation event

### SubscriptionRegistry

Responsibilities:
- track active SSE subscriptions
- know which tables each subscribed View depends on
- react to invalidation events
- rerun only affected Views
- emit new SSE payloads only when the View result actually changed

### ViewReadService

Responsibilities:
- load a saved View by `view_id`
- execute `view.query` on a read-only connection
- serialize a stable JSON snapshot
- compute a stable fingerprint for equality comparison

## Required Database Additions

### `app_user` table

Use a separate table name such as `app_user`, not `user`.

Suggested columns:
- `id INTEGER PRIMARY KEY`
- `name TEXT NOT NULL`
- `username TEXT NOT NULL UNIQUE`
- `password_hash TEXT NOT NULL`
- `created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP`
- `updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP`

Optional later:
- `disabled INTEGER NOT NULL DEFAULT 0`

### `view_dependency` table

This table is used only for targeted invalidation.

Suggested columns:
- `view_id INTEGER NOT NULL`
- `table_name TEXT NOT NULL`
- composite unique key on `(view_id, table_name)`
- foreign key from `view_id` to `view(id)`

Purpose:
- when a write touches `record`
- only subscriptions to Views that depend on `record` need to be reconsidered

Without this table, the server would need to parse or infer SQL dependencies dynamically, which is less reliable and more expensive.

## Required Configuration

### `.env`

The application should load `.env` at startup using `dotenvy`.

Required variable:

```env
JWT_SECRET=very-long-random-secret
```

Rules:
- fail fast if API mode is enabled and `JWT_SECRET` is missing
- do not auto-generate secrets at runtime
- do not commit `.env`

## Step-by-Step Implementation Plan

### 1. Load `.env` and API auth configuration

At startup:
- call `dotenvy::dotenv()` early in `lince`
- read `JWT_SECRET`
- make startup fail if the API/auth path is enabled and the secret is absent

This should happen before serving HTTP.

### 2. Add migrations for auth and invalidation metadata

Create migrations for:
- `app_user`
- `view_dependency`

The migration should:
- enforce `username` uniqueness
- enforce `password_hash` presence
- add the `view_dependency` foreign key

This metadata is part of the stable runtime contract of the API.

### 3. Add root seeding flow

Create a dedicated seed path for the root user:
- username: `bomboclaat`
- password: `crazyfrog`

Important rules:
- store only an Argon2 hash
- make the seed idempotent
- do not recreate the user if it already exists
- do not make this seed implicit in normal startup unless explicitly desired

Preferred shape:
- a CLI flag or explicit seed command path that triggers root creation

This seed is for your personal VPS testing environment, not as a production pattern.

### 4. Add `AuthService`

Implement:
- password hashing
- password verification
- JWT creation
- JWT validation

Use:
- `argon2`
- `jsonwebtoken`

JWT content should be minimal:
- `sub` or `user_id`
- `username`
- expiration timestamp

The validated JWT should produce an `AuthSubject` for downstream handlers.

### 5. Add unauthenticated login endpoint

Endpoint:

- `POST /api/auth/login`

Request body:

```json
{
  "username": "bomboclaat",
  "password": "crazyfrog"
}
```

Response body:

```json
{
  "token": "<jwt>",
  "token_type": "Bearer"
}
```

This is the only API endpoint that should not require prior authentication.

### 6. Add the internal `WriteCoordinator`

This is the central runtime service.

Responsibilities:
- own one `SqliteConnection`
- receive serialized write requests
- execute them one at a time

The key idea:
- transports do not write directly to SQLite
- they send a request to the coordinator

Recommended implementation shape:
- a spawned Tokio task
- `tokio::sync::mpsc` for requests
- `tokio::sync::oneshot` for replies

Example request types:
- execute one SQL write request
- seed root user
- future internal write operations

### 7. Route all runtime writes through the coordinator

Before implementing SSE, make sure the write path is unified.

That includes:
- HTTP `/api/sql`
- Karma writes
- any future internal automation

Karma should not write directly using a separate pool connection. It should compute what it wants to do, then send a write request to the coordinator.

This rule is what makes hook-based invalidation trustworthy.

### 8. Install SQLite hooks on the writer connection

Use the `sqlx` SQLite connection handle APIs:
- `set_update_hook`
- `set_commit_hook`
- `set_rollback_hook`

What each one does here:

#### `update_hook`

Triggered for row-level `INSERT`, `UPDATE`, and `DELETE` events on that connection.

Use it to:
- collect the changed table names into an in-memory set for the current write request

Do not use it to:
- run SQL
- do async work
- emit network events directly

#### `commit_hook`

Triggered when the transaction commits.

Use it to:
- mark that the accumulated changed-table set is valid

Important constraint:
- do not run SQL inside the hook callback

#### `rollback_hook`

Triggered when the transaction rolls back.

Use it to:
- discard the accumulated changed-table set
- ensure no invalidation event is emitted

### 9. Convert hook results into invalidation events

After the write completes successfully:
- take the changed-table set accumulated during the write
- emit one invalidation event

Suggested shape:

```text
InvalidationEvent {
  changed_tables: {"record", "view"}
}
```

Do not emit one event per row.
Emit one event per committed write request or committed transaction.

### 10. Add authenticated write endpoint

Endpoint:

- `POST /api/sql`

This endpoint should:
- require a valid JWT
- accept one SQL statement per request
- forward that write request to the coordinator
- return a structured response

Recommended constraints:
- no multi-statement SQL in a single request
- coordinator controls transaction boundaries
- response contains at least success/failure and rows affected

Suggested request:

```json
{
  "sql": "UPDATE record SET quantity = quantity + 1 WHERE id = 1"
}
```

Suggested response:

```json
{
  "ok": true,
  "rows_affected": 1
}
```

### 11. Add and maintain `view_dependency`

The server must know which tables a View depends on.

Initial recommendation:
- treat `view_dependency` as explicit metadata
- populate it when creating or updating a View

This allows `SubscriptionRegistry` to answer:
- "does this invalidation event matter for this subscribed View?"

Later, if desired, dependency derivation can be automated, but the initial implementation should prefer explicit correctness.

### 12. Add `ViewReadService`

Implement a service that:
- loads `view.query` by `view_id`
- executes it on a read-only connection
- converts the result to stable JSON
- computes a stable fingerprint

Stability requirements:
- stable column ordering
- stable row serialization
- deterministic query semantics

Strong recommendation:
- subscribed Views should use deterministic ordering
- if a View query has no `ORDER BY`, the API may get false-positive "changes" due to row ordering differences

### 13. Add authenticated SSE endpoint for saved Views

Endpoint:

- `GET /api/sse/view/{view_id}`

Flow:
- validate JWT once when the request starts
- load the saved View
- load that View's dependency tables
- execute the initial read
- send the initial SSE snapshot
- register the subscription in `SubscriptionRegistry`

Then on invalidation:
- check if `changed_tables` intersects this View's dependency set
- if not, do nothing
- if yes, rerun the View query
- compare the new fingerprint with the previous one
- only send a new SSE event if the result actually changed

### 14. Make SSE payloads data-oriented

The JSON API SSE stream should send data snapshots, not HTML fragments.

Suggested event shape:

```text
event: snapshot
data: {"view_id": 7, "rows": [...]}
```

This keeps the API transport independent from the HTML frontend.

The HTML/Datastar side can remain its own transport layer.

### 15. Defer higher-level subscriptions until later

Do not start with:
- arbitrary SQL subscriptions
- collection-wide subscriptions
- page-wide subscriptions

Start with:
- one stream per `view_id`

That is the cleanest first version and aligns with Lince's current data model.

## Proposed Endpoints

### Authentication

- `POST /api/auth/login`

### Writes

- `POST /api/sql`

### Reads

- `GET /api/sse/view/{view_id}`

This is enough for the first architecture.

## Suggested Internal Types

These are conceptual examples, not fixed code.

### `AuthSubject`

```rust
pub struct AuthSubject {
    pub user_id: u64,
    pub username: String,
}
```

### `WriteRequest`

```rust
pub enum WriteRequest {
    ExecuteSql {
        actor: AuthSubject,
        sql: String,
        reply: tokio::sync::oneshot::Sender<Result<WriteOutcome, std::io::Error>>,
    },
    SeedRootUser {
        reply: tokio::sync::oneshot::Sender<Result<(), std::io::Error>>,
    },
}
```

### `WriteOutcome`

```rust
pub struct WriteOutcome {
    pub rows_affected: u64,
    pub changed_tables: std::collections::BTreeSet<String>,
}
```

### `InvalidationEvent`

```rust
pub struct InvalidationEvent {
    pub changed_tables: std::collections::BTreeSet<String>,
}
```

### `ViewSubscription`

```rust
pub struct ViewSubscription {
    pub id: u64,
    pub view_id: u32,
    pub dependent_tables: std::collections::BTreeSet<String>,
    pub last_fingerprint: u64,
}
```

## File and Module Direction

Suggested module placement:

- `crates/application/src/auth.rs`
- `crates/application/src/write_coordinator.rs`
- `crates/application/src/subscription_registry.rs`
- `crates/application/src/view_read.rs`
- `crates/html/src/api.rs`

Possible supporting persistence modules:

- `crates/persistence/src/repositories/user.rs`
- `crates/persistence/src/repositories/view_dependency.rs`

## Important Runtime Constraints

1. Hook callbacks must stay tiny.
   They should only capture metadata in memory.

2. Do not run SQL inside SQLite hook callbacks.
   Read work should happen after the write request completes.

3. Do not allow multiple runtime writers.
   The single writer rule is the foundation of the design.

4. Keep read connections separate from the writer connection.
   This fits WAL mode well.

5. Make SSE subscriptions explicit and deterministic.
   A saved View is the unit of subscription.

## Why `view_id` Is Better Than Arbitrary SQL for SSE

Subscribing by `view_id` gives:
- stable identity
- better security
- easier caching and diffing
- a clean dependency map
- a direct match to Lince's existing query model

Arbitrary SQL subscriptions would make:
- invalidation targeting less reliable
- API contracts less stable
- abuse easier
- client behavior harder to reason about

## Future Extensions

These are reasonable later, but should not be first:

- `GET /api/sse/collection/{collection_id}`
- automatic dependency extraction from `view.query`
- row-level diff events using `preupdate_hook`
- token revocation or user disabling
- richer write commands than raw SQL

## Minimal First Implementation Order

If implementation needs to be phased, do it in this order:

1. `.env` loading and `JWT_SECRET`
2. migrations for `app_user` and `view_dependency`
3. root seeding path
4. `AuthService`
5. `/api/auth/login`
6. `WriteCoordinator`
7. move Karma and HTTP writes onto the coordinator
8. install SQLite hooks
9. emit invalidation events
10. `/api/sql`
11. `ViewReadService`
12. `SubscriptionRegistry`
13. `/api/sse/view/{view_id}`

This order reduces the risk of building live subscriptions before the single-writer rule is actually enforced.
