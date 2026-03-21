# View Dependency And Explicit Services Follow-Up

## Purpose

This document describes the next architectural refinement after the initial API/auth/single-writer implementation.

It focuses on two gaps that remain:

1. `view_dependency` exists, but it is not yet the authoritative and continuously maintained source of invalidation metadata.
2. The runtime behavior we wanted as explicit services currently exists mostly as free functions and handler-local logic.

This is a design document only. It does not imply implementation has started.

## Current Situation

The current implementation already provides:

- `app_user` and `view_dependency` tables
- `/api/auth/login`
- authenticated `/api/sql`
- authenticated `GET /api/sse/view/{view_id}`
- a single runtime SQLite writer with hooks
- hook-driven invalidation events
- SSE refreshes based on saved Lince `view` rows

However, the current shape is still transitional:

- `view_dependency` is populated by seeding and consulted at runtime, but runtime dependency resolution also falls back to inferring tables directly from `view.query`
- auth behavior lives in utility functions plus route-local request parsing
- view snapshot behavior lives in repository methods plus light application wrappers
- SSE subscription coordination is implemented inside the HTTP handler stream state instead of a dedicated service
- the write coordinator is explicit, but its request model is still minimal and actor-unaware

So the system works, but it is not yet as internally clean and explicit as originally intended.

## Part I: `view_dependency`

### What `view_dependency` Should Mean

`view_dependency` should answer one very specific runtime question:

"If table `X` changed, which saved Lince Views might need to be reread?"

That means the desired semantics are:

- one row per `(view_id, table_name)`
- only real database tables that can affect the result of that View
- stable enough to drive invalidation targeting
- updated whenever the corresponding saved View changes

It is not a query cache.
It is not presentation metadata.
It is not a substitute for the saved `view.query`.

It is invalidation metadata.

### Why the Current Hybrid Shape Is Not Ideal

Right now runtime dependency lookup does two things:

1. reads rows from `view_dependency`
2. also infers dependencies by regex from the current `view.query`

That is a practical fallback, but it weakens the contract.

Problems with the hybrid approach:

- `view_dependency` can silently drift and nobody is forced to notice
- inference and metadata can disagree
- inference may miss more complex SQL shapes
- the system no longer has one clear source of truth for invalidation targeting
- later maintainers will not know whether they are supposed to update the table or trust inference

The consequence is that the table exists, but it is not yet authoritative.

### Target Contract

The target contract should be:

- `view.query` is the source of truth for what a View returns
- `view_dependency` is the source of truth for which writes can invalidate that View
- inference is used only for controlled maintenance tasks, not as the runtime truth

That means runtime SSE refresh logic should eventually be able to say:

- load dependency rows from `view_dependency`
- if `changed_tables` does not intersect those rows, do nothing
- if it intersects, reread the View

No runtime inference should be needed once the metadata is trustworthy.

### When `view_dependency` Must Be Updated

`view_dependency` must be updated whenever one of these happens:

1. A View is created.
2. A View's `query` is changed.
3. A View is deleted.
4. A special pseudo-View is converted into a real SQL-backed View.
5. A real SQL-backed View is converted into a special pseudo-View.

The system should not depend on a startup seed pass to repair this metadata later.

### Ownership

There should be one explicit runtime component responsible for this metadata:

- `ViewDependencyService`

Its job is not to execute the View.
Its job is to compute and persist invalidation metadata for the View.

### Proposed `ViewDependencyService`

Responsibilities:

- classify whether a View is SQL-backed or special
- derive dependency table names from a View query
- write the normalized dependency set into `view_dependency`
- remove dependency rows when a View is deleted or becomes special
- provide dependency lookup for subscribers if needed

Conceptual API:

```rust
pub trait ViewDependencyService {
    async fn refresh_for_view(&self, view_id: u32) -> Result<(), Error>;
    async fn refresh_for_query(&self, view_id: u32, query: &str) -> Result<(), Error>;
    async fn delete_for_view(&self, view_id: u32) -> Result<(), Error>;
    async fn dependencies_for_view(&self, view_id: u32) -> Result<BTreeSet<String>, Error>;
}
```

### Recommended Source Of Dependencies

There are two candidate approaches:

1. explicit metadata only
2. runtime inference every time

The recommended approach is:

- explicit metadata as the runtime contract
- inference only inside `ViewDependencyService`

That gives you:

- fast and predictable runtime lookup
- one normal form in the database
- one place to improve parsing later if needed

### Recommended Update Strategy

When a View is created or its query is updated:

1. load the final `view.query`
2. classify it as special or SQL-backed
3. if special:
   - delete all `view_dependency` rows for that `view_id`
4. if SQL-backed:
   - derive dependency tables
   - normalize them to lowercase
   - replace all rows for that `view_id` in one small transaction

The replace behavior should be:

- delete existing rows for that `view_id`
- insert the newly derived normalized set

This avoids stale rows from old versions of the query.

### Should It Be Derived From SQL Text Or Stored Separately By The User

For your current model, derive it from SQL text.

Reasons:

- the user already edits `view.query`
- asking the user to maintain a second dependency list is error-prone
- the primary goal is invalidation targeting, not query planning

The only time explicit user-maintained dependency metadata would make sense is if Views become much more complex than plain SQL text and dependency inference becomes impossible.

That is not necessary now.

### How Accurate Must Derivation Be

For this stage, the derivation only needs to be conservative, not perfect.

Conservative means:

- false positives are acceptable
- false negatives are dangerous

Examples:

- if the service says a View depends on `record` and `tag`, but only `record` truly matters, the worst outcome is an unnecessary reread
- if the service forgets `tag`, then a tag change may fail to refresh a subscribed client

So the derivation rule should bias toward including too many tables rather than too few.

### Special Views

Lince already has non-SQL pseudo-Views such as special command/creation/karma views.

The dependency contract should be:

- SQL-backed Views may be subscribed by `/api/sse/view/{view_id}`
- special pseudo-Views are not streamable through this endpoint
- special pseudo-Views should have no `view_dependency` rows

This keeps the machine API aligned with actual SQL-backed data subscriptions.

### Runtime Usage

Once `view_dependency` is authoritative, SSE invalidation should work like this:

1. subscription opens for `view_id`
2. subscription loads dependency set from `view_dependency`
3. writer emits `InvalidationEvent { changed_tables }`
4. subscription checks intersection against cached dependency set
5. no intersection:
   - do nothing
6. intersection:
   - reread the View
   - compare snapshot/fingerprint
   - emit only if changed

This gives the cleanest runtime story:

- metadata answers whether reread is needed
- snapshot execution answers whether the result actually changed

### How To Handle Dependency Drift During Transition

The transition from hybrid behavior to authoritative metadata should be explicit.

Recommended migration path:

1. keep current inference fallback temporarily
2. add explicit maintenance on View create/update/delete paths
3. add a command or startup reconciliation task that refreshes all stored dependencies once
4. once that path is trusted, remove runtime fallback inference from subscriber dependency lookup

This avoids breaking existing databases while moving toward the cleaner model.

### Recommended Test Cases For `view_dependency`

- creating a SQL-backed View populates dependency rows
- updating a View query replaces old dependency rows
- deleting a View removes dependency rows
- converting a View to a special pseudo-View clears dependency rows
- dependency lookup returns normalized table names
- multi-table queries insert all relevant dependency rows
- dependency drift reconciliation repairs stale metadata
- an invalidation on an unrelated table does not wake a subscription
- an invalidation on a related table does wake a subscription

## Part II: Converting Implicit Services Into Explicit Types

### What "Implicit Services" Means In The Current Code

The code already has the behavior of several services, but the responsibilities are scattered:

- auth behavior:
  - password hashing in `utils::auth`
  - JWT parsing in `utils::auth`
  - login orchestration in the HTTP route
- view reads:
  - query execution in `ViewRepository`
  - light wrappers in `application::view`
- SSE coordination:
  - subscription state and refresh loop inside the API handler
- write coordination:
  - explicit type exists, but request model is still narrow and not actor-aware

The result is functional, but the boundaries are still transport-shaped rather than domain-shaped.

### Why Explicit Types Are Better

Explicit service types make the architecture easier to reason about:

- handlers become thin
- responsibilities are named
- testing gets easier
- the runtime flow becomes reusable outside HTTP
- auth, reads, and subscriptions stop being route-local behavior

This is especially important in your project because you already have multiple clients:

- HTML
- GUI
- TUI
- Karma
- future API consumers

### Recommended Explicit Types

The current implementation should be refined toward these runtime components:

- `AuthService`
- `WriteCoordinator`
- `ViewDependencyService`
- `ViewReadService`
- `SubscriptionRegistry`

The writer already exists, so that part is a refinement, not a greenfield addition.

## `AuthService`

### Responsibilities

- verify a username/password pair
- issue JWTs
- validate JWTs
- produce a runtime `AuthSubject`

### Why It Should Be Explicit

Right now login is orchestrated directly in the route, which means:

- auth policy is partly embedded in transport code
- any future non-HTTP auth consumer would duplicate logic
- token issuance and user verification are not represented as a named capability

### Conceptual Shape

```rust
pub struct AuthSubject {
    pub user_id: u64,
    pub username: String,
}

pub trait AuthService {
    async fn login(&self, username: &str, password: &str) -> Result<String, Error>;
    fn authenticate_token(&self, token: &str) -> Result<AuthSubject, Error>;
    fn hash_password(&self, password: &str) -> Result<String, Error>;
}
```

### Recommended Behavior

- `login` performs user lookup + password verification + JWT issue
- `authenticate_token` decodes the JWT and returns `AuthSubject`
- the HTTP layer should call this service, not assemble auth manually

## `WriteCoordinator`

### Current Status

This is already explicit, but its interface is still minimal.

### What Still Needs Refinement

- request types should become named commands, not just raw SQL + params
- request context should be able to carry the actor
- lifecycle events should be explicit

### Recommended Refinement

Conceptual request shape:

```rust
pub enum WriteRequest {
    ExecuteSql {
        actor: Option<AuthSubject>,
        sql: String,
        params: Vec<SqlParameter>,
        reply: oneshot::Sender<Result<WriteOutcome, Error>>,
    },
    RefreshViewDependencies {
        view_id: u32,
        reply: oneshot::Sender<Result<(), Error>>,
    },
    SeedRootUser {
        reply: oneshot::Sender<Result<(), Error>>,
    },
}
```

This does not mean every write must stop being SQL-based immediately.
It means the coordinator becomes the explicit runtime write service instead of just a narrow SQL queue.

## `ViewDependencyService`

### Responsibilities

- compute dependency rows from a View
- persist them
- load them for invalidation targeting
- reconcile them when needed

### Why It Should Be Separate From `ViewReadService`

These are different concerns:

- `ViewReadService` answers "what does this View return?"
- `ViewDependencyService` answers "what can invalidate this View?"

Keeping them separate preserves cleaner reasoning and testing.

## `ViewReadService`

### Responsibilities

- load a saved Lince View
- execute `view.query` on a read-only connection
- reject special pseudo-Views for API SSE
- serialize rows into a stable JSON form
- compute a stable fingerprint or payload string

### Why It Should Be Explicit

Right now this behavior is split between repository code and handler-local serialization decisions.

Making it explicit would give one place to define:

- what a "snapshot" means
- what shape gets serialized
- what equality comparison means
- what restrictions apply to streamable Views

### Conceptual Shape

```rust
pub struct ViewSnapshot {
    pub view_id: u32,
    pub name: String,
    pub query: String,
    pub columns: Vec<String>,
    pub rows: Vec<BTreeMap<String, String>>,
}

pub struct ViewSnapshotEnvelope {
    pub snapshot: ViewSnapshot,
    pub fingerprint: String,
}

pub trait ViewReadService {
    async fn read_snapshot(&self, view_id: u32) -> Result<ViewSnapshotEnvelope, Error>;
}
```

Using an explicit envelope avoids recomputing the serialized payload in every caller.

## `SubscriptionRegistry`

### Responsibilities

- own active subscriptions
- register and unregister streams
- cache dependency sets per subscription
- receive invalidation events
- decide which subscriptions need rereads
- emit updated snapshots only when changed

### Why It Should Be Explicit

Right now each SSE request constructs its own local loop and talks directly to the broadcast channel.

That works, but it means:

- subscription orchestration is embedded in the HTTP handler
- other transports cannot reuse it
- subscription policy is not represented as a named part of the system

### Recommended Runtime Model

The registry should be a long-lived runtime component, not a handler-local abstraction.

The flow should be:

1. HTTP authenticates the request
2. HTTP asks `SubscriptionRegistry` to subscribe the caller to `view_id`
3. registry loads dependency metadata
4. registry loads the initial snapshot through `ViewReadService`
5. registry returns a stream or receiver to HTTP
6. when invalidation events arrive, registry refreshes only affected subscriptions

### Conceptual Shape

```rust
pub struct SubscriptionHandle {
    pub view_id: u32,
    pub rx: tokio::sync::mpsc::UnboundedReceiver<SseFrame>,
}

pub enum SseFrame {
    Snapshot { payload: String },
    Error { payload: String },
}

pub trait SubscriptionRegistry {
    async fn subscribe_view(
        &self,
        actor: AuthSubject,
        view_id: u32,
    ) -> Result<SubscriptionHandle, Error>;
}
```

### Important Design Choice

The registry should own subscription policy.
The HTTP route should not decide:

- how dependencies are cached
- when to refresh on lag
- what counts as changed

That all belongs in the registry.

## Recommended Wiring Changes

### Current Injection

Current injection provides:

- repositories
- writer handle

### Recommended Runtime Injection

The next shape should expose explicit runtime services as first-class dependencies:

- repositories
- `auth_service`
- `write_coordinator`
- `view_dependency_service`
- `view_read_service`
- `subscription_registry`

Conceptually:

```rust
pub struct RuntimeServices {
    pub repository: Repositories,
    pub auth: Arc<dyn AuthService>,
    pub writer: WriteCoordinatorHandle,
    pub view_dependencies: Arc<dyn ViewDependencyService>,
    pub view_reads: Arc<dyn ViewReadService>,
    pub subscriptions: Arc<dyn SubscriptionRegistry>,
}
```

This makes the runtime architecture visible at the dependency boundary instead of hiding it inside helper functions.

## Handler Simplification After Extraction

### Login Route

Current route responsibilities:

- parse JSON
- fetch user
- verify password
- issue JWT

Desired route responsibilities:

- parse JSON
- call `AuthService::login`
- return token

### SQL Route

Current route responsibilities:

- parse bearer token
- validate token
- execute SQL through writer

Desired route responsibilities:

- authenticate via `AuthService`
- call `WriteCoordinator`
- return structured write result

### SSE Route

Current route responsibilities:

- authenticate
- load dependencies
- load snapshot
- subscribe to invalidations
- compare payloads
- refresh on lag
- serialize errors

Desired route responsibilities:

- authenticate via `AuthService`
- call `SubscriptionRegistry::subscribe_view`
- translate emitted frames into Axum SSE events

That is a major cleanup in complexity, even though the runtime behavior stays the same.

## Recommended Implementation Order For This Follow-Up

### 1. Make `view_dependency` authoritative

- create `ViewDependencyService`
- make View create/update/delete paths call it
- add a reconciliation path for existing databases
- keep fallback inference only during transition

### 2. Extract `AuthService`

- move login/token orchestration out of route code
- keep current utility functions behind the service implementation

### 3. Extract `ViewReadService`

- move snapshot serialization and fingerprint logic into one service
- make SSE code depend on that service rather than direct repository calls

### 4. Extract `SubscriptionRegistry`

- move subscription state machine out of the route
- let the route just expose the stream

### 5. Refine `WriteCoordinator` request model

- optionally add actor context
- add named maintenance requests if useful

### 6. Remove runtime dependency inference fallback

- only after authoritative metadata is trusted

## Risks To Watch

### `view_dependency` False Negatives

If dependency derivation misses a table, a subscribed client may not refresh when it should.

That is why:

- derivation must be conservative
- reconciliation should exist
- tests should prefer catching false negatives

### Service Extraction Without Clear Ownership

If explicit types are added but responsibilities remain duplicated, the code will get worse, not better.

Each extracted type must own a single clear concern.

### Overcoupling The Registry To HTTP

The `SubscriptionRegistry` should emit generic frames or payloads.
It should not depend on Axum-specific types internally.

### Mixing Query Semantics With Invalidation Semantics

Keep these separate:

- query execution: `ViewReadService`
- invalidation targeting: `ViewDependencyService`

That separation is the main reason to do this cleanup.

## Recommended Tests For The Explicit-Type Refactor

- `AuthService::login` succeeds for valid credentials
- `AuthService::login` rejects invalid credentials
- `AuthService::authenticate_token` rejects expired/invalid JWTs
- `ViewReadService` rejects special pseudo-Views
- `ViewReadService` returns stable snapshot payloads for unchanged data
- `ViewDependencyService` rewrites dependency rows on View query updates
- `SubscriptionRegistry` ignores unrelated invalidations
- `SubscriptionRegistry` emits a snapshot when related data changes
- `SubscriptionRegistry` does not emit when reread data is unchanged
- handler tests confirm routes become thin wrappers over these services

## End State

After this follow-up, the architecture should look like this:

- `view.query` defines what a saved View returns
- `view_dependency` authoritatively defines what can invalidate that View
- `AuthService` owns login and token validation
- `WriteCoordinator` owns runtime writes and invalidation emission
- `ViewReadService` owns snapshot execution and serialization
- `SubscriptionRegistry` owns live subscription refresh policy
- HTTP routes become small transport adapters

That is the cleanest version of the architecture we already started.
