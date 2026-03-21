# API Table CRUD Specification

This document defines the backend API contract that replaces the raw SQL endpoint for the main application workflow.

The goal is:

- keep `POST /api/auth/login`
- keep `GET /api/sse/view/{view_id}`
- replace `POST /api/sql` with authenticated CRUD endpoints under `/api/table/{table}`
- use role-aware authorization
- keep `view_dependency` internal and auto-managed

Important mounting note:

- the logical backend contract is described here as `/api/...`
- in the current merged server, the backend is mounted under `/consult`
- so the currently implemented external paths are `/consult/api/...`

## Authentication

All backend API endpoints require authentication except:

- `POST /api/auth/login`

Authentication uses a Bearer JWT.

The JWT claims must contain:

- `sub`: user id
- `username`: username
- `role_id`: role id
- `role`: role name
- `exp`: expiration timestamp

## Roles

Add a `role` table.

Minimum seeded roles:

- `admin`
- `lince`

Rules:

- root seed user must always be `admin`
- newly created users default to role `lince`
- role information is embedded in the JWT at login

## Tables Exposed As CRUD

Only these tables are exposed through `/api/table/{table}`:

- `view`
- `record`
- `frequency`
- `karma_condition`
- `karma_consequence`
- `karma`
- `configuration`
- `app_user`
- `role`

These tables are not exposed through this CRUD contract:

- `collection`
- `collection_view`
- `view_dependency`
- `history`
- `sum`
- `transfer`
- `dna`
- `query`
- `command`

## Route Surface

For each allowed table:

- `GET /api/table/{table}`
- `POST /api/table/{table}`
- `GET /api/table/{table}/{id}`
- `PATCH /api/table/{table}/{id}`
- `DELETE /api/table/{table}/{id}`

`/api/sse/view/{view_id}` remains available and authenticated.

## Generic CRUD Rules

- `id` is server-owned
- clients cannot create or update `id`
- `POST` must reject or ignore incoming `id`
- `PATCH` must reject incoming `id`
- unknown columns must be rejected
- only whitelisted columns per table may be written
- read responses must be JSON objects or arrays of objects

## Writable Columns By Table

### `view`

Writable columns:

- `name`
- `query`

Special rule:

- after create, update, or delete of a view, `view_dependency` must be reconciled automatically

### `record`

Writable columns:

- `quantity`
- `head`
- `body`

### `frequency`

Writable columns:

- `quantity`
- `name`
- `day_week`
- `months`
- `days`
- `seconds`
- `next_date`
- `finish_date`
- `catch_up_sum`

### `karma_condition`

Writable columns:

- `quantity`
- `name`
- `condition`

### `karma_consequence`

Writable columns:

- `quantity`
- `name`
- `consequence`

### `karma`

Writable columns:

- `quantity`
- `name`
- `condition_id`
- `operator`
- `consequence_id`

### `configuration`

Writable columns:

- `quantity`
- `name`
- `language`
- `timezone`
- `style`
- `show_command_notifications`
- `command_notification_seconds`
- `delete_confirmation`
- `error_toast_seconds`
- `keybinding_mode`

### `app_user`

Accepted writable columns:

- `name`
- `username`
- `password`
- `role_id`

Important rules:

- client must never send `password_hash`
- `password` must be hashed on create
- `password` must be hashed on update when present
- `password_hash` must never be returned in `GET` responses
- non-admin users cannot change `role_id`
- new users default to the `lince` role if `role_id` is not explicitly assigned by an admin

Returned user shape must not include:

- `password`
- `password_hash`

Recommended returned user shape:

- `id`
- `name`
- `username`
- `role_id`
- `role`
- `created_at`
- `updated_at`

### `role`

Writable columns:

- `name`

Read responses may include:

- `id`
- `name`

## Authorization Rules

### General rule

All `/api/table/*` endpoints require a valid JWT.

### `role`

Allowed for all authenticated users:

- `GET /api/table/role`
- `GET /api/table/role/{id}`

Admin only:

- `POST /api/table/role`
- `PATCH /api/table/role/{id}`
- `DELETE /api/table/role/{id}`

### `app_user`

Allowed for all authenticated users:

- `GET /api/table/app_user`
- `GET /api/table/app_user/{id}`

Returned user data never includes passwords or password hashes.

Create:

- `POST /api/table/app_user`
- admin only
- if no role is provided, assign `lince`

Update:

- `PATCH /api/table/app_user/{id}`
- allowed if requester is:
  - that same user, or
  - an admin

Additional update rules:

- self-updating user can change:
  - `name`
  - `username`
  - `password`
- self-updating user cannot change:
  - `role_id`
- admin can change:
  - `name`
  - `username`
  - `password`
  - `role_id`

Delete:

- `DELETE /api/table/app_user/{id}`
- allowed if requester is:
  - that same user, or
  - an admin

### All other exposed tables

Authenticated users may perform normal CRUD:

- `view`
- `record`
- `frequency`
- `karma_condition`
- `karma_consequence`
- `karma`
- `configuration`

No role-based restrictions beyond authentication are required for those tables in this pass.

## SSE View Endpoint

Keep:

- `GET /api/sse/view/{view_id}`

Rules:

- authenticated only
- subscription is based on saved Lince `view` rows
- the endpoint returns snapshots of the saved view result
- it must continue to use dependency-driven invalidation
- when relevant tables change, the server reruns the saved view query
- only emit a new event when the serialized snapshot changed

## Internal Tables And Metadata

### `view_dependency`

`view_dependency` is internal metadata and must not receive public CRUD endpoints.

Rules:

- it is derived from `view.query`
- it is reconciled automatically after changes to `view`
- clients must not create, update, or delete dependency rows directly through the stable API

## Error Handling

Recommended response shape for errors:

```json
{
  "error": "Human-readable message"
}
```

Status guidance:

- `400 Bad Request` for invalid payload, invalid writable columns, or malformed ids
- `401 Unauthorized` for missing/invalid token
- `403 Forbidden` for authenticated but unauthorized operations
- `404 Not Found` for unknown rows
- `409 Conflict` for uniqueness violations like duplicate usernames or duplicate role names
- `500 Internal Server Error` for unexpected failures

## Migration And Seeding Requirements

Schema changes required:

- add `role` table
- add `app_user.role_id`

Seeding rules:

- ensure role `admin` exists
- ensure role `lince` exists
- ensure root seeded user exists
- ensure root seeded user is assigned role `admin`

## Compatibility Note

This specification replaces the backend raw SQL write endpoint as the stable frontend contract.

The SSE endpoint stays:

- `GET /api/sse/view/{view_id}`

The old SQL endpoint should be removed from the stable API surface after the CRUD implementation is in place.

With the current routing layout, those backend endpoints are externally available under:

- `/consult/api/auth/login`
- `/consult/api/table/{table}`
- `/consult/api/table/{table}/{id}`
- `/consult/api/sse/view/{view_id}`
