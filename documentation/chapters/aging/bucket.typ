== Bucket

The bucket is one configured object store.

The split between published sand widgets and general media should be modeled with key prefixes, not separate buckets.

Reserved bucket roots in v1:

- `lince/dna/sand/` for published sand widget packages derived from records
- `lince/media/` for everything else

This section records the current agreement for moving away from the GitHub `dna` repo as the runtime source of installable widgets.

=== Agreements

- The long-term source of truth for remote DNA widgets should be bucket objects plus database records, not the GitHub `dna` repo.
- The publication row should be a normal `record`.
- The human-facing listing fields should come from `record.head` and `record.body`.
- The bucket package layout should mirror the current package structure path used today.
- The bucket object key should not be stored in `record.body`.
- The bucket object reference should live in `record_resource_ref`.
- The client should not scan broad table data from every organ and infer a catalog locally.
- Each organ should expose a dedicated DNA catalog endpoint.
- The host should aggregate those organ-level DNA catalogs for the web UI.
- After approval, installed widgets should land in one shared local directory: `~/.config/lince/web/sand`.
- The older split between `~/.config/lince/web/widgets` and `~/.config/lince/web/sand` is not the desired end state.
- Community sand packages should be hidden by default.
- Community sand visibility should require an explicit user opt-in stored in the active `configuration`.
- The UI should always show which organ a sand package comes from.
- The canonical source identifier should be the organ id, not the display name.
- `official` and `community` are publication channels inside an organ's `sand/` tree.
- Anything that is not `official` should be treated as unsafe by default, even if it comes from a known organ such as Manas.

=== Package layout

Preferred package prefix layout:

```text
lince/dna/sand/{channel}/{first_two_letters}/{package_name_lower_snake_case}/
```

Example:

```text
lince/dna/sand/official/ka/kanban_record_view/
```

Recommended object layout inside that package prefix:

```text
lince/dna/sand/official/ka/kanban_record_view/
  sand.toml
  kanban_record_view_metadata.html
  kanban_record_view.lince
  ...other package contents...
```

This mirrors the current structure path in the publication flow instead of inventing a second package layout.

If the local package id continues to use dash-style normalization elsewhere in the codebase, the translation between local id and bucket package name should be explicit.

The bucket convention here is lower snake case by agreement, not by current `slugify` behavior.

=== Record model

The preferred publication shape is:

- `record` as the main row
- `record_extension` for structured DNA publication metadata
- `record_resource_ref` for the bucket-backed package reference

The task-category system discussed in Kanban is still task metadata.

Bucket-backed DNA publication metadata should remain explicit and structured.

Recommended sidecar namespace:

```text
record_extension.namespace = "lince.dna"
```

Suggested `record_extension.freestyle_data_structure` shape:

```json
{
  "published": true,
  "channel": "official",
  "version": "0.1.0"
}
```

Recommended package reference:

```text
record_resource_ref.provider = "bucket"
record_resource_ref.resource_kind = "sand"
record_resource_ref.resource_path = "lince/dna/sand/official/ka/kanban_record_view/kanban_record_view_metadata.html"
```

Suggested `record_resource_ref.freestyle_data_structure` shape:

```json
{
  "slug": "kanban_record_view",
  "package_prefix": "lince/dna/sand/official/ka/kanban_record_view/",
  "transport_filename": "kanban_record_view_metadata.html",
  "available_files": [
    "kanban_record_view_metadata.html",
    "kanban_record_view.lince"
  ],
  "package_format": "html",
  "mime_type": "text/html",
  "sha256": "optional-content-hash"
}
```

The slug belongs with the bucket-backed package reference because it identifies the published artifact, not only the record text.

In v1, a published DNA record should have exactly one primary `record_resource_ref` with:

- `provider = "bucket"`
- `resource_kind = "sand"`
- a `resource_path` under `lince/dna/sand/`

If more resources are later attached to the same record, the `sand` entry remains the canonical install target and the others are auxiliary resources.

The canonical transport target may be either:

- a plain `.html` package
- a `.lince` package archive

All preview, install, and local catalog flows should be able to fetch raw bytes, detect the format, unzip when needed, and use either transport.

=== Category direction

There is a separate broader refactor direction around categories.

The current Kanban-specific `task.categories` model is not the right long-term place for every component family that needs to differentiate records.

The preferred direction is a generic record-level category or tagging model that views can filter on.

That would cover use cases such as:

- project-scoped Kanban views
- family-specific view filtering
- identifying records that publish sand packages

Preferred direction:

- keep categories many-valued
- keep them attached to records through structured side metadata
- let views query them directly
- avoid treating category as a magic implicit routing rule outside query logic

Suggested future namespace:

```text
record_extension.namespace = "record.categories"
```

Possible payload shape:

```json
{
  "categories": ["project_alpha", "lincedna", "sand"]
}
```

This is a direction, not a blocker for the bucket DNA cutover.

The bucket DNA flow should still use `lince.dna` metadata as the canonical publication contract even if categories become queryable and useful for discovery.

=== Endpoint contracts

Preferred organ-local catalog route:

```text
GET /api/dna/catalog
```

Each row should expose the record-facing fields as `head` and `body`, not as `title` and `description`.

Suggested item shape:

```json
{
  "organ_id": "manas",
  "organ_name": "Lince Manas",
  "record_id": 123,
  "head": "Kanban Record View",
  "body": "Record-centric board with comments, worklog, and resources.",
  "slug": "kanban_record_view",
  "version": "0.1.0",
  "channel": "official",
  "unsafe": false,
  "bucket_key": "lince/dna/sand/official/ka/kanban_record_view/kanban_record_view_metadata.html",
  "package_prefix": "lince/dna/sand/official/ka/kanban_record_view/",
  "transport_filename": "kanban_record_view_metadata.html",
  "available_files": [
    "kanban_record_view_metadata.html",
    "kanban_record_view.lince"
  ],
  "package_format": "html",
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

Preferred preview/install behavior:

- fetch raw bytes from `bucket_key`
- use the transport filename to determine whether the source is `.html` or `.lince`
- parse both through the same package parser
- unzip `.lince` as needed during preview or install
- keep enough metadata to remember which transport file was the source

=== Community packages

Only `official` sand packages should be shown before the user opts in.

Community packages should require an explicit confirmation and that choice should be stored in the active `configuration`.

Suggested configuration flag:

```text
configuration.show_community_sand = 0 | 1
```

Preferred behavior:

- default `0`
- show only `official` when `0`
- allow `official` and `community` when `1`
- surface a clear confirmation before the first change from `0` to `1`
- always show the source organ name beside the channel
- use `organ_id` as the canonical source key in state, cache, and provenance records
- render `community` as unsafe regardless of which organ published it

This is a trust and UX gate, not a security boundary.

If a stronger trust model is needed later, publisher verification or per-organ trust policy would be separate concerns.

=== Local install direction

The local machine should keep one shared `sand` directory for web widgets.

That simplifies the user's mental model, but it also means the system must stop treating `sand` as meaning only "built-in official widget".

After this refactor, source metadata should remain explicit even if the filesystem directory is unified.

If `.lince` is meant to remain a first-class transport format, the local catalog should preserve enough source metadata to know whether the imported source was `.html` or `.lince`.

If preserving the original archive matters beyond provenance, local persistence should not immediately collapse every imported archive into `.html`.

Useful source labels include:

- built-in official widget
- uploaded local file
- imported from organ DNA catalog

That distinction should live in metadata, not in separate directories.

=== Migration boundary

The GitHub-backed DNA catalog described elsewhere in the documentation should be treated as transitional.

Once bucket-backed DNA publication and dedicated organ endpoints exist, runtime discovery and installation should use the bucket-plus-record flow described here.
