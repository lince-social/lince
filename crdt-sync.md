# CRDT Record Sync Between Organs

## Goal

Implement optional record-bundle sync between organs.

Keep current numeric local primary keys. Each organ owns its own integer `id` values. Add separate sync identity columns so rows can be matched across organs without sharing local IDs.

Phase 1 uses a homegrown Rust operation-log CRDT with last-write-wins field registers. Phase 2 can add collaborative text editing and cursor presence.

## Critique

- Syncing only `record` is incomplete. Record sync must include record sidecars, work metadata, assignments, and relation rows.
- Reusing SQLite integer `id` across organs is wrong. Local numeric IDs must remain local.
- Sync needs stable cross-organ identity: `sync_uid`, `origin_organ_id`, and record ownership metadata.
- Raw SQL/table writes are unsafe for sync. Every local and remote record-bundle write must pass through application write helpers that run validation, karma, file sync, side effects, and sync hooks.
- Real-time collaborative editing for `head`/`body` at character level is too expensive for v1. Whole-field CRDT is enough for Phase 1.
- Login sync intent and persisted organ sync policy are different controls. Sync is enabled only when both allow it.

## Record Bundle Scope

Record sync includes:

- `record`
- `record_extension`
- `record_link`
- `record_comment`
- `record_worklog`
- `record_resource_ref`
- `work_metadata` where `owner_kind = 'record'`
- `work_assignment` rows for synced `work_metadata`
- `work_subject` rows referenced by synced assignments

Do not sync `app_user` as a record dependency. Preserve nullable user references, snapshots, or remote subject references.

## Data Model

Keep every existing numeric `id` as the local primary key.

Add sync identity to every synced table:

- `sync_uid TEXT NOT NULL UNIQUE`
- `origin_organ_id INTEGER NOT NULL REFERENCES organ(id)`
- `created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP`
- `updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP`

Add record ownership:

- `record.owner_organ_id INTEGER NOT NULL REFERENCES organ(id)`

Local records default to the local organ. Incoming records preserve the source or declared owner organ.

Add organ sync policy:

- `organ_sync_policy.organ_id`
- `organ_sync_policy.sync_resources`, JSON array, initially `["record"]`
- `organ_sync_policy.record_sync_mode`, one of `none`, `sync_outgoing`, `sync_incoming`, `sync_both`

Add durable sync state:

- `record_sync_operation`
- `record_sync_tombstone`
- `record_sync_ack`
- `record_sync_pending_dependency`

`record_sync_operation` fields:

- `operation_uid`
- `source_organ_id`
- `actor_user_id`
- `root_record_sync_uid`
- `table_name`
- `row_sync_uid`
- `action`: `insert`, `update`, `delete`
- `field_payload_json`
- `operation_clock`
- `source_operation_uid`
- `created_at`
- `applied_at`
- `sent_at`

`operation_clock` must be monotonic per organ. Prefer a hybrid logical clock. Accept `(updated_at, actor_organ_id, sequence)` if simpler.

## Sync Modes

- `none`: no record sync.
- `sync_outgoing`: send eligible local changes to the remote organ.
- `sync_incoming`: receive eligible remote changes from the remote organ.
- `sync_both`: send and receive eligible changes.

Effective sync requires:

- persisted organ policy allows resource and direction
- JWT sync claim allows resource and direction
- authenticated user has record read/write permissions
- organ `trust_state` is not `blocked`
- socket/session requested record sync

## JWT And Session Rules

Login may include sync intent:

- `sync_resources`
- `record_sync_mode`

Issued JWT includes sync claims. The remote organ uses these claims to decide whether the socket should receive or send record sync frames.

Persisted organ policy is still required. JWT claims cannot enable sync that local policy disabled.

## Ownership Rules

- Local records default to `owner_organ_id = local organ`.
- Incoming records keep `owner_organ_id = source/owner organ`.
- Editing a remote-owned record locally sends changes back only to the owner organ.
- Local-owned records sync outward only to organs with `sync_outgoing` or `sync_both`.
- `sync_incoming` accepts remote changes but does not send local edits.
- Do not echo a remote operation back to its source organ.

## Write Path

Application layer owns record sync.

Create record-bundle write helpers in `crates/application/src/write.rs`.

All mutation paths must use these helpers for synced tables:

- local web backend table API
- local widget actions
- karma actions that edit record sidecars
- file sync writes
- remote sync apply

Record-bundle helpers must:

- validate payload
- execute the write
- resolve affected root record IDs
- run existing record business hooks
- enqueue sync operation when `SyncOrigin::Local`
- skip re-enqueue to source when `SyncOrigin::Remote`
- fan out eligible operations to connected sync sockets

Remote sync apply must never call `writer.execute_statement` directly. It must call the same record-bundle helpers with `SyncOrigin::Remote`.

## Transport

Use Axum WebSocket for bidirectional sync.

Add endpoint:

- `GET /sync/record/socket`

Socket handshake requires bearer auth. JWT claims identify:

- user
- permissions
- allowed sync resources
- allowed sync direction
- source organ

Socket frames are JSON:

- `hello`
- `operation`
- `ack`
- `catch_up_request`
- `catch_up_batch`
- `error`

Connected socket registry tracks:

- organ id
- user id
- allowed resources
- direction
- last acknowledged clock

Local commits enqueue operations and fan out to eligible connected sockets. Offline peers catch up from `record_sync_operation` using `record_sync_ack`.

## CRDT Rules

Phase 1 CRDT is operation-log based.

- Insert: add row by `sync_uid`.
- Update: merge per field with last-write-wins.
- Delete: create tombstone.
- Tombstone wins over older updates.
- Newer update after tombstone may recreate only if explicitly allowed by an insert/update operation with newer clock.
- Deduplicate by `operation_uid` and `source_operation_uid`.
- Sort remote replay by `operation_clock`, then `source_organ_id`, then `operation_uid`.

If one organ edits Monday and another edits Tuesday, and both reconnect Friday:

1. Apply Monday operation.
2. Apply Tuesday operation.
3. Latest field clock wins.

For Phase 1:

- `record.head` is a whole-field LWW register.
- `record.body` is a whole-field LWW register.
- numeric/date/status fields are whole-field LWW registers.
- `record_link`, `work_assignment`, and sidecars use `sync_uid` plus existing natural uniqueness for validation.
- missing linked target rows create pending dependency operations and retry after dependencies arrive.

## Phase 1 Implementation Steps

1. Add sync identity, ownership, policy, operation, ack, tombstone, and pending-dependency schema through persistence-owned Rust structs and migrations.
2. Add local sync UID generation while preserving numeric local IDs.
3. Add record-bundle write helpers and `SyncOrigin`.
4. Route all record-bundle mutations through those helpers.
5. Add operation enqueue and CRDT merge logic.
6. Add WebSocket endpoint and connected-organ registry.
7. Add login request sync intent and JWT sync claims.
8. Add organ sync policy API fields.
9. Add replay/catch-up by last acknowledged operation clock.
10. Add tests and run `cargo check`.

## Phase 2

Implement only after Phase 1 is stable:

- collaborative editing for `record.head` and `record.body`
- cursor/presence frames using `user@organ`
- optional real text CRDT dependency only for document text

Do not add a JS CRDT dependency in Phase 1.

## Tests

Run `cargo check`. Warnings are errors.

Add tests for:

- migrations create sync tables and columns
- local numeric IDs remain local
- sync UID maps rows across organs
- LWW field merge
- tombstone precedence
- dedupe by operation UID
- Monday/Tuesday/Friday replay order
- outgoing sync
- incoming sync
- sync-both
- blocked organ rejection
- missing permission rejection
- JWT sync disabled rejection
- record sidecar mutation enqueues sync
- remote operation applies through record-bundle helpers
- source organ does not receive echoed operations
- disabled resource receives no socket frames
