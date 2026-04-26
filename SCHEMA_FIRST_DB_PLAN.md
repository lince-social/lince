# Rust Schema-First SQLite Plan

## Decisions

- SQLite only.
- `sqlx` stays.
- Business queries stay mostly raw, bound SQL.
- Schema ownership moves from `migrations/*.sql` to Rust table structs.
- Schema upgrades run automatically at startup. No migration CLI in the normal path.
- `domain` is renamed to `structure`.
- All DB-facing row types, schema metadata, reconciliation, and SQLite DDL live in `persistence`.
- `structure` holds request/response DTOs, app-wide shared types, board/runtime types, and other non-row types.
- Macro naming is `#[derive(Table)]` and `#[table(...)]`.
- Core Lince tables should end at SQLite `STRICT` tables.
- Renames are not inferred in v1.
- Kanban/frontend filters edit a derived persisted `view.query`, not table layout.
- The schema endpoint is a writable field contract for UI/table sands, not a raw DB schema dump.

## Scope

This is not an ORM rewrite. It is a schema ownership rewrite.

What changes:

- Rust structs define the tables.
- Persistence computes desired schema from those structs.
- Persistence introspects the live SQLite schema.
- Persistence computes a migration plan.
- Persistence applies safe changes automatically before the app starts serving.

What does not change:

- repositories can keep using raw `sqlx::query`, `sqlx::query_as`, and bound parameters
- the one-writer model stays
- HTTP code does not become ORM-driven

## View-Backed Filter Truth

Frontend filtering should operate by rewriting persisted SQL views, not by rewriting table layout and
not by inventing an internal projection contract view.

Rules:

- the source query stays a normal SQL query over `record` and related tables
- the frontend may produce ugly SQL if needed; correctness and editability matter more than beauty here
- filter UI state may still live in board/widget state for UX, but the data-source truth is the derived `view.query`
- `apply-filters` should materialize the derived view immediately
- stream preparation may still re-check and refresh the derived view as a safety net

Non-goal:

- do not introduce `_sys_record_projection` as the frontend contract for this workflow

## Readability Standard

The implementation must optimize for human scanning, not just correctness.

Rules:

- each schema submodule should have one clear job
- avoid monolithic files for schema logic
- prefer explicit types over clever generic helpers
- keep SQL rendering separate from diff logic
- keep SQLite inspection separate from migration application
- keep row structs separate from HTTP DTOs
- prefer a few obvious structs and enums over deep trait hierarchies
- favor stable naming over terse naming

Target file shape:

- `types.rs`: only data structures and enums
- `registry.rs`: only table registration and ordering
- `inspect.rs`: only live SQLite reads
- `plan.rs`: only desired-vs-live decision logic
- `apply.rs`: only execution of planned actions
- `rebuild.rs`: only table rebuild flow
- `sql.rs`: only SQLite SQL string rendering
- `state.rs`: only schema fingerprint/meta table handling

Target function shape:

- short functions with one phase each
- explicit input/output types
- avoid hidden side effects
- use small helper functions instead of deeply nested control flow

Target naming:

- `OrganRow`, `RecordRow`, `ViewRow` for persistence rows
- `OrganResponse`, `UpsertOrganRequest` for structure DTOs
- `LiveTable`, `LiveColumn`, `SchemaAction`, `RebuildReason` for schema internals
- `ensure_schema`, `inspect_schema`, `build_plan`, `apply_plan`, `rebuild_table` for top-level flow

## Layer Ownership

### `structure`

This replaces `domain`.

It owns:

- HTTP request DTOs
- HTTP response DTOs
- app-wide shared enums and value objects
- board/bootstrap/runtime state types
- package/archive types
- non-persistence traits and types used across application/web/gui/tui

It does not own:

- row structs
- schema annotations
- SQL type mapping
- migration logic
- SQLite introspection

### `persistence`

It owns all DB-specific concerns:

- row structs
- `#[derive(Table)]` / `#[table(...)]` metadata
- live SQLite schema inspection
- schema diffing
- SQL rendering for SQLite DDL
- automatic schema application
- raw SQL repositories/stores

Rule: if a type is shaped by a database table, it belongs in `persistence`.

## Current Codebase Mapping

Move current `domain::clean::*` row-like structs into `persistence::models::*`.

Expected moves:

- `clean::record::Record`
- `clean::view::View`
- `clean::configuration::Configuration`
- `clean::collection::Collection`
- `clean::frequency::Frequency`
- `clean::command::Command`
- `clean::app_user::AppUser`
- `clean::role::Role`
- `clean::karma::*`

Move current `domain::dirty::*` app/runtime types into `structure::*`.

Expected moves:

- `dirty::operation::*`
- `dirty::view::*`
- `dirty::karma::*`
- other non-row transport/runtime shapes

## Naming

Use `Table` everywhere the schema contract is described.

```rust
#[derive(Table, Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[table(name = "organ")]
pub struct OrganRow {
    #[table(primary_key)]
    pub id: String,
    pub name: String,
    pub base_url: String,
}
```

Supported attributes in the design:

- `#[table(name = "...")]`
- `#[table(primary_key)]`
- `#[table(unique)]`
- `#[table(index)]`
- `#[table(default = "...")]`
- `#[table(references = "other_table(other_column)")]`

Not in v1:

- inferred renames
- multi-column indexes declared via field attributes
- multi-driver metadata

## SQLite Feature Decisions

### Generated Columns

Do not collapse sidecar tables into `record_extension`.

Decision:

- `record_comment`, `record_link`, `record_worklog`, and `record_resource_ref` stay dedicated tables
- if generated columns are introduced later, they are only helper columns inside an existing table
- the first candidate is `record_extension`, for frequently queried same-row JSON paths
- generated columns are not part of the first schema-first refactor pass

### JSON Mutation Functions

For targeted JSON edits, prefer SQLite JSON functions over Rust parse-edit-serialize loops.

Decision:

- use `json_set`, `json_remove`, and related functions for small targeted updates
- each such change should be one explicit SQL write at the call site
- no background reconciliation loop
- no repeated write amplification beyond the single intended `UPDATE`

### `STRICT`

`STRICT` is part of the end-state schema contract.

Decision:

- `#[derive(Table)]` / `TableSchema` should render core Lince tables as `CREATE TABLE ... STRICT`
- existing compatibility work can still use bridge migrations while the Rust-owned schema layer is being introduced
- the first bridge slice is already represented by `[migrations/20260426183000_sqlite_strict_sidecars_and_checks.up.sql](/home/user/git/lince-social/lince/migrations/20260426183000_sqlite_strict_sidecars_and_checks.up.sql)`

### FTS5

FTS5 is planned, but not part of the first schema-first migration engine pass.

Decision:

- keep first-pass search behavior on normal tables
- once the schema-first foundation is in place, add FTS5 for real text-search surfaces
- first candidates are `record(head, body)` and `record_comment(body)`

## Persistence Layout

```text
crates/persistence/src/
  lib.rs
  connection.rs
  seeder.rs
  write_coordinator.rs
  models/
    mod.rs
    organ.rs
    record.rs
    view.rs
    configuration.rs
    collection.rs
    frequency.rs
    command.rs
    app_user.rs
    role.rs
    karma.rs
  schema/
    mod.rs
    types.rs
    registry.rs
    inspect.rs
    plan.rs
    apply.rs
    rebuild.rs
    state.rs
    sql.rs
  repositories/
    ...
```

Module responsibilities:

- `models/*`: row structs and `Table` derive usage
- `schema/types.rs`: `TableSchema`, `ColumnDef`, `ForeignKeyDef`, `IndexDef`, `SchemaPlan`
- `schema/registry.rs`: single list of all tables and stable ordering
- `schema/inspect.rs`: SQLite `PRAGMA` and `sqlite_master` inspection
- `schema/plan.rs`: desired-vs-live diff into explicit actions
- `schema/apply.rs`: execute plan through the writer/connection
- `schema/rebuild.rs`: SQLite table rebuild flow for incompatible changes
- `schema/state.rs`: schema fingerprint/meta table handling
- `schema/sql.rs`: render SQLite DDL from metadata

Readability constraint:

- no schema file should mix more than one of inspection, planning, SQL rendering, and execution
- if a file starts to hold more than one phase, split it
- prefer more small files over one large "migration engine" file

## Table Contract

The first version can be trait-based or derive-based. The target API should look like this:

```rust
pub trait Table {
    const NAME: &'static str;
    fn schema() -> TableSchema;
}

pub struct TableSchema {
    pub name: &'static str,
    pub columns: &'static [ColumnDef],
    pub foreign_keys: &'static [ForeignKeyDef],
    pub indexes: &'static [IndexDef],
}

pub struct ColumnDef {
    pub name: &'static str,
    pub sql_type: SqliteType,
    pub nullable: bool,
    pub primary_key: bool,
    pub unique: bool,
    pub default_sql: Option<&'static str>,
}

pub struct ForeignKeyDef {
    pub column: &'static str,
    pub references_table: &'static str,
    pub references_column: &'static str,
}

pub struct IndexDef {
    pub name: &'static str,
    pub columns: &'static [&'static str],
    pub unique: bool,
}
```

Use a narrow SQLite type enum, not arbitrary strings:

```rust
pub enum SqliteType {
    Integer,
    Real,
    Text,
    Blob,
}
```

Readability constraint:

- table structs should stay visually close to the actual table shape
- keep field order matching the table order
- keep `#[table(...)]` attributes directly on the affected field
- do not hide schema details behind unrelated helper macros

## SQLite Inspection

Inspection is SQLite-specific and should not be abstracted.

Use:

- `PRAGMA table_info(table_name)`
- `PRAGMA foreign_key_list(table_name)`
- `PRAGMA index_list(table_name)`
- `PRAGMA index_info(index_name)`
- `SELECT sql FROM sqlite_master WHERE type = 'table' AND name = ?`

Existing reuse opportunity:

- current column inspection in `[crates/persistence/src/repositories/table.rs](/home/user/git/lince-social/lince/crates/persistence/src/repositories/table.rs)` should be absorbed into `schema::inspect`

The internal live representation should normalize SQLite metadata into:

- `LiveTable`
- `LiveColumn`
- `LiveForeignKey`
- `LiveIndex`

Do not expose raw pragma output across the rest of the codebase.

Readability constraint:

- one normalization step at the boundary
- after normalization, the rest of the code should not know pragma row shapes

## Schema Planning

The planner turns desired schema and live schema into explicit actions.

```rust
pub enum SchemaAction {
    CreateTable(TableSchema),
    AddColumn { table: &'static str, column: ColumnDef },
    CreateIndex { table: &'static str, index: IndexDef },
    RebuildTable { table: &'static str, target: TableSchema, reason: RebuildReason },
}
```

Safe automatic actions:

- create missing tables
- create missing indexes
- add missing nullable columns
- add missing non-nullable columns only when they have a SQL default

Actions that require SQLite rebuild:

- foreign key drift
- unique constraint drift
- primary key drift
- column removal
- type changes

Actions not supported automatically in v1:

- inferred table rename
- inferred column rename

Additional SQLite rule:

- new core tables created by the Rust schema engine should be emitted as `STRICT`

Decision on renames:

- v1 does not support automatic rename detection
- a rename is treated as add+orphan drift and should fail fast with a clear error
- if rename support is ever added, it should be explicit via dedicated compatibility metadata, not inference

Readability constraint:

- planning code should be declarative and table-by-table
- each `SchemaAction` variant should correspond to one obvious execution path
- the planner should produce reasons alongside rebuild actions so failures are self-explanatory

## SQLite Rebuild Policy

SQLite cannot apply many constraint changes with a simple `ALTER TABLE`.

For rebuild-required changes, persistence should use a standard rebuild flow:

1. create `__new_<table>`
2. create the target schema there
3. copy compatible data columns
4. validate row count / copy success
5. drop old table
6. rename new table
7. recreate indexes

Rebuilds are allowed automatically only when data copy is lossless under the planned change.

If the change is destructive or ambiguous, startup must fail with a clear schema error instead of guessing.

Readability constraint:

- rebuild flow should be written as numbered phases in code comments or helper names
- do not inline the entire rebuild algorithm into one function

## Schema State

Track applied schema state in SQLite itself.

Add a small meta table, for example:

```sql
CREATE TABLE IF NOT EXISTS __schema_state (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    fingerprint TEXT NOT NULL,
    applied_at TEXT NOT NULL
);
```

Policy:

- compute fingerprint from the Rust registry
- compare it on startup
- inspect live schema anyway before applying
- update the fingerprint only after a successful apply

The fingerprint is an optimization and audit aid, not the source of truth.

## Startup Flow

Startup ordering should be fixed:

1. open SQLite connection
2. run `persistence::schema::ensure_schema`
3. run `persistence::seeder::seed`
4. build services
5. build HTTP routes
6. start serving

The web layer should not know how schema reconciliation works. It just depends on persistence having finished.

Readability constraint:

- startup integration should call one persistence entrypoint, not orchestrate many schema internals from web

## Seeder

`[crates/persistence/src/seeder.rs](/home/user/git/lince-social/lince/crates/persistence/src/seeder.rs)` becomes data-only.

It should:

- seed default rows
- seed default views/collections/content

It should not:

- create tables
- define schema
- be treated as the migration mechanism

## Repository Style

Repositories stay raw SQL.

Example:

```rust
sqlx::query_as::<_, OrganRow>(
    "SELECT id, name, base_url FROM organ WHERE id = ? LIMIT 1",
)
.bind(id)
.fetch_optional(&*self.db)
.await?;
```

Mapping into `structure` DTOs happens after persistence returns row structs.

```rust
pub fn organ_response_from_row(row: OrganRow) -> OrganResponse {
    OrganResponse {
        id: row.id,
        name: row.name,
        base_url: row.base_url,
    }
}
```

Readability constraint:

- mapping functions should be explicit and boring
- avoid implicit conversion traits for request/response boundaries in the first pass

## Schema Endpoint

Add:

- `GET /schema/{table}`

It returns:

- the requested table name
- that table first
- all other tables after it in stable registry order
- create-time field metadata for each table
- only what the user needs to type to insert an extra row through the UI
- a simplified input contract, not an exact DB schema mirror

Each table response should look closer to:

```json
{
  "table": "record_extension",
  "create_fields": [
    { "name": "record_id", "input": "integer", "required": true },
    { "name": "namespace", "input": "text", "required": true },
    { "name": "version", "input": "integer", "required": false, "default": 1 },
    { "name": "freestyle_data_structure", "input": "json", "required": true }
  ]
}
```

Rules:

- hide generated fields
- hide read-only fields
- hide auto ids unless the table truly requires caller-supplied ids
- include default values when they matter to data entry
- optionally include lightweight relation hints when a field expects another table id

Internal note:

- persistence should still inspect indexes, foreign keys, defaults, and nullability for migration planning
- the endpoint does not need to expose all of that directly

## `utoipa`

Document the schema endpoint as first-class API.

```rust
#[derive(utoipa::OpenApi)]
#[openapi(
    paths(get_schema),
    components(schemas(
        TableName,
        SchemaResponse,
        TableInputSchemaResponse,
        CreateFieldResponse
    ))
)]
pub struct ApiDoc;
```

The schema endpoint response types belong in the HTTP layer, not in persistence.

## Existing File Changes

### `migrations/`

End state:

- not the primary path
- either removed after stabilization
- or kept only as temporary compatibility/import material

### `crates/persistence/src/repositories/table.rs`

Use it as the seed for SQLite inspection work, then narrow it to repository concerns or remove it if the inspection responsibility fully moves into `schema::inspect`.

### `crates/web/src/infrastructure/organ_store.rs`

Its row-like `Organ` type should move into `persistence::models::organ::OrganRow`.

The store logic can stay raw SQL, but it should query into the persistence row type.

### `crates/web/src/presentation/http/api/schema.rs`

New API module for schema metadata response and `utoipa` annotations.

## Rollout Order

1. Create `crates/structure` and rename/move non-row `domain` types there.
2. Add `persistence::models` and move row-like `domain::clean::*` types there.
3. Add `persistence::schema::types`, `registry`, and `inspect`.
4. Implement `Table` trait manually for a small first slice: `organ`, `record`, `view`, `configuration`.
5. Implement `schema::sql`, `schema::plan`, and `schema::apply`.
6. Add `__schema_state`.
7. Wire `ensure_schema` into startup before seeding.
8. Convert `seeder.rs` to data-only assumptions.
9. Add `/schema/{table}` and `utoipa` for the writable field contract.
10. Replace remaining row-like `domain` dependencies in repositories.
11. Optionally add the derive macro after the trait-based version is proven.

## Code Review Checklist

Every schema-related change should be checked against:

- is the file doing exactly one phase of the pipeline
- are row structs separated from DTOs
- is SQLite-specific logic confined to persistence
- can a reader follow `inspect -> plan -> apply -> rebuild` without jumping through indirection
- do names reflect the actual DB concept clearly
- are failure messages explicit about which table/column/action failed

## Non-Goals

- multi-database support
- ORM query abstraction
- automatic rename inference
- destructive best-effort schema guessing

## Answers To The Former Open Questions

- Foreign keys: enforce them fully in v1, including rebuilds when SQLite requires it. Do not ship a half-enforced mode.
- Renames: do not support automatic renames in v1. Fail fast and require an explicit compatibility step.
- Kanban/frontend filters: persist and edit a derived `view.query`, not table layout.
- Sidecar tables: keep them as dedicated tables; do not collapse them into `record_extension`.
- JSON mutation writes: one explicit SQLite JSON update per intended write, no background sync.
- Core schema strictness: yes, end-state core tables should be `STRICT`.
- Schema endpoint: expose writable create-field contracts, not a raw schema mirror.
- FTS5: planned after the schema-first foundation, not in the first pass.
