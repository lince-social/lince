# API Endpoints

The HTML server mounts the API under `/api` and currently exposes three endpoints:

- `POST /api/auth/login`
- `POST /api/sql`
- `GET /api/sse/view/{view_id}`

The server listens on `127.0.0.1:6174`, so local examples use `http://127.0.0.1:6174`.

## Authentication Flow

`POST /api/auth/login` is the only unauthenticated API endpoint.

Workflow:

1. Look up the user by `username`.
2. Verify the password against the stored Argon2 hash.
3. Issue a JWT bearer token signed with the server secret.
4. Return the token for later API calls.

Request:

```json
{
  "username": "alice",
  "password": "secret"
}
```

Response:

```json
{
  "token": "<jwt>",
  "token_type": "Bearer"
}
```

Capabilities:

- Authenticates a user with username/password.
- Produces a bearer token valid for about 24 hours.
- Returns `401` for invalid credentials.

## SQL Execution

`POST /api/sql` executes a write statement through the write coordinator.

Workflow:

1. Read `Authorization: Bearer <token>`.
2. Validate the JWT.
3. Execute exactly one SQL statement.
4. Track affected tables through SQLite hooks.
5. Broadcast invalidation events for dependent subscribers.

Request:

```http
Authorization: Bearer <jwt>
Content-Type: application/json
```

```json
{
  "sql": "UPDATE collection SET name = 'Inbox' WHERE id = 1"
}
```

Response:

```json
{
  "ok": true,
  "rows_affected": 1,
  "changed_tables": ["collection"]
}
```

Capabilities:

- Runs authenticated write SQL against the app database.
- Reports affected row count and changed tables.
- Triggers downstream refresh for view streams that depend on those tables.

Current constraints:

- Only one SQL statement is allowed per request.
- Invalid SQL or invalid input returns `400`.
- Missing or invalid bearer tokens return `401`.

## View Streaming

`GET /api/sse/view/{view_id}` opens a Server-Sent Events stream for a SQL-backed view.

Workflow:

1. Validate the bearer token once when the stream starts.
2. Load the view dependencies.
3. Load the initial snapshot of the view.
4. Send an initial `snapshot` event.
5. Listen for write invalidations.
6. Recompute the snapshot when a changed table matches the view dependencies.
7. Send a new `snapshot` event only when the payload actually changed.

Request:

```http
GET /api/sse/view/1
Authorization: Bearer <jwt>
Accept: text/event-stream
```

Snapshot event shape:

```json
{
  "view_id": 1,
  "name": "My View",
  "query": "select ...",
  "columns": ["id", "name"],
  "rows": [
    { "id": "1", "name": "Alice" }
  ]
}
```

Capabilities:

- Streams live updates for SQL-backed views.
- Avoids unnecessary events when the snapshot content did not change.
- Emits SSE `error` events if refresh or serialization fails.

Current constraints:

- Only SQL-backed views can be streamed.
- Special/non-SQL views are rejected.
- Authentication is required for the whole stream.

## Error Format

All API errors return JSON in this form:

```json
{
  "error": "message"
}
```

Typical statuses:

- `400 Bad Request` for invalid SQL or invalid stream/view input.
- `401 Unauthorized` for missing, malformed, expired, or invalid tokens.
- `500 Internal Server Error` for internal failures such as auth or serialization issues.
