== Bucket

The bucket is one configured object store.

The split between DNA files and general media should be modeled with key prefixes, not separate buckets.

Reserved top-level prefixes in v1:

- `dna/` for published web widget packages derived from records
- `media/` for everything else

This section records the current agreement for moving away from the GitHub `dna` repo as the runtime source of installable widgets.

=== Agreements

- The long-term source of truth for remote DNA widgets should be bucket objects plus database records, not the GitHub `dna` repo.
- The publication row should be a normal `record`.
- The human-facing listing fields should come from `record.head` and `record.body`.
- The bucket object key should not be stored in `record.body`.
- The bucket object key should live in `record_resource_ref`.
- The client should not scan broad table data from every organ and infer a catalog locally.
- Each organ should expose a dedicated DNA catalog endpoint.
- The host should aggregate those organ-level DNA catalogs for the web UI.
- After approval, installed widgets should land in one shared local directory: `~/.config/lince/web/sand`.
- The older split between `~/.config/lince/web/widgets` and `~/.config/lince/web/sand` is not the desired end state.

=== Record model

The preferred publication shape is:

- `record` as the main row
- `record_extension` for structured DNA publication metadata
- `record_resource_ref` for the bucket-backed package file

If a category label is reused for discovery, such as `lincedna`, it should be treated only as a coarse label.

It should not be the sole source of truth for whether a record is a published DNA package.

The task-category system discussed in Kanban is still task metadata.

Bucket-backed DNA publication metadata should remain explicit and structured.

Recommended sidecar namespace:

```text
record_extension.namespace = "lince.dna"
```

Suggested `record_extension.data_json` shape:

```json
{
  "published": true,
  "channel": "official",
  "version": "0.1.0"
}
```

Recommended package file reference:

```text
record_resource_ref.provider = "bucket"
record_resource_ref.resource_kind = "sand"
record_resource_ref.resource_path = "dna/official/ka/kanban-record-view/kanban-record-view.html"
```

Suggested `record_resource_ref.data_json` shape:

```json
{
  "slug": "kanban-record-view",
  "filename": "kanban-record-view.html",
  "mime_type": "text/html",
  "sha256": "optional-content-hash"
}
```

The slug belongs with the bucket-backed package reference because it identifies the published artifact, not only the record text.

In v1, a published DNA record should have exactly one primary `record_resource_ref` with:

- `provider = "bucket"`
- `resource_kind = "sand"`
- a `resource_path` under `dna/`

If more resources are later attached to the same record, the `sand` entry remains the canonical install target and the others are auxiliary resources.

=== Endpoint contracts

Preferred organ-local catalog route:

```text
GET /api/dna/catalog
```

Each row should expose the record-facing fields as `head` and `body`, not as `title` and `description`.

Suggested item shape:

```json
{
  "record_id": 123,
  "head": "Kanban Record View",
  "body": "Record-centric board with comments, worklog, and resources.",
  "slug": "kanban-record-view",
  "version": "0.1.0",
  "channel": "official",
  "bucket_key": "dna/official/ka/kanban-record-view/kanban-record-view.html",
  "filename": "kanban-record-view.html",
  "mime_type": "text/html",
  "updated_at": "2026-04-04T12:00:00Z"
}
```

Preferred host aggregation routes:

```text
GET /host/packages/dna/catalog
GET /host/packages/dna/{organ_id}/{record_id}/preview
POST /host/packages/dna/{organ_id}/{record_id}/install
```

The preview and install steps should converge into the same package parsing and local materialization flow already used for standalone package imports.

=== Local install direction

The local machine should keep one shared `sand` directory for web widgets.

That simplifies the user's mental model, but it also means the system must stop treating `sand` as meaning only "built-in official widget".

After this refactor, source metadata should remain explicit even if the filesystem directory is unified.

Useful source labels include:

- built-in official widget
- uploaded local file
- imported from organ DNA catalog

That distinction should live in metadata, not in separate directories.

=== Migration boundary

The GitHub-backed DNA catalog described elsewhere in the documentation should be treated as transitional.

Once bucket-backed DNA publication and dedicated organ endpoints exist, runtime discovery and installation should use the bucket-plus-record flow described here.
