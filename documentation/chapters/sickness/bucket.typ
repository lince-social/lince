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
- The UI should always show which organ a sand package comes from.
- The canonical source identifier should be the organ id, not the display name.
- The publication channel should remain explicit as `official` or `community`.
- `official/community` should remain a publication label for now, not a cryptographic trust proof.
- The UI should show both the source organ and the publication channel.
- All built-in official sands should be created locally at startup.
- Built-in official sands should not be pushed into the bucket automatically in any startup flow.
- Bucket publication should be a manual action through the sand export or sand publisher flow.
- If startup creation fails for one sand, the process should log it and continue booting.
- Published sand packages should use versioned bucket paths.
- Version comparison should be driven by package metadata, not by filename alone.
- Same version plus different bytes should be treated as a hard failure during publication.
- Signature-based official verification should be documented as the future trust model, but it should not be required in the current runtime flow.

=== Package layout

Preferred package prefix layout:

```text
lince/dna/sand/{channel}/{first_two_letters}/{package_name_lower_snake_case}/{version}/
```

Example:

```text
lince/dna/sand/official/ka/kanban_record_view/1.4.0/
```

Recommended object layout inside that package prefix:

```text
lince/dna/sand/official/ka/kanban_record_view/1.4.0/
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
  "version": "1.4.0"
}
```

Recommended package reference:

```text
record_resource_ref.provider = "bucket"
record_resource_ref.resource_kind = "sand"
record_resource_ref.resource_path = "lince/dna/sand/official/ka/kanban_record_view/1.4.0/kanban_record_view_metadata.html"
```

Suggested `record_resource_ref.freestyle_data_structure` shape:

```json
{
  "slug": "kanban_record_view",
  "channel": "official",
  "version": "1.4.0",
  "package_prefix": "lince/dna/sand/official/ka/kanban_record_view/1.4.0/",
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
  "channel": "official",
  "version": "1.4.0",
  "bucket_key": "lince/dna/sand/official/ka/kanban_record_view/1.4.0/kanban_record_view_metadata.html",
  "package_prefix": "lince/dna/sand/official/ka/kanban_record_view/1.4.0/",
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

=== Channel semantics

The web UI should keep `official/community` as an explicit publication channel.

Preferred behavior:

- the DNA browser should always show the source organ name beside the package channel
- the publication channel should be returned directly by the catalog endpoint
- `official` should be treated as a publisher claim in the current model
- `community` should remain the fallback for everything that is not intentionally published as `official`
- the canonical source key in state, cache, and provenance remains `organ_id`

This is a publication and browsing distinction, not a trust proof.

The web version should not hardcode a list of official hosts in the preferred current direction.

If a stronger trust model is needed later, signature-based release verification should be layered on top of the same record and bucket structure rather than replacing it.

=== Signature-based official verification

This is the preferred future trust model, but it should not be required in the current runtime flow.

The goal is to preserve the current `official/community` publication model while adding a cryptographic way to confirm that an `official` release was signed by a trusted publisher.

Recommended shape:

- keep `record` as the publication root
- keep `record_extension(namespace = "lince.dna")` for release metadata
- add `record_extension(namespace = "lince.signature")` for signature metadata
- keep `record_resource_ref` as the canonical artifact pointer

Recommended signing model:

- `Ed25519` key pairs
- a stable `publisher_id`
- a `key_id` for rotation
- a signed release payload that includes artifact hash and release metadata

Recommended signed payload shape:

```json
{
  "schema": "lince.dna.release.v1",
  "publisher_id": "lince:manas:official",
  "slug": "kanban_record_view",
  "channel": "official",
  "version": "1.4.0",
  "package_format": "lince",
  "entry_path": "index.html",
  "sha256": "artifact-content-hash",
  "created_at": "2026-04-04T12:00:00Z"
}
```

Recommended `record_extension(namespace = "lince.signature")` payload shape:

```json
{
  "algorithm": "ed25519",
  "schema": "lince.signature.v1",
  "publisher_id": "lince:manas:official",
  "key_id": "ed25519:lince:manas:2026-04",
  "signature": "base64-signature",
  "signed_payload": {
    "schema": "lince.dna.release.v1",
    "publisher_id": "lince:manas:official",
    "slug": "kanban_record_view",
    "channel": "official",
    "version": "1.4.0",
    "package_format": "lince",
    "entry_path": "index.html",
    "sha256": "artifact-content-hash",
    "created_at": "2026-04-04T12:00:00Z"
  }
}
```

Key storage model:

- private signing keys stay outside the database and outside the repository
- public verification keys live in trusted client configuration or the shipped web build
- signature metadata lives with the publication record so mirrors do not need a second trust database

Verification model:

- verification should be automatic during catalog view, preview, and install
- there should be no user approval step for signature checking itself
- if a release claims `official` but the signature is missing or invalid, the UI should not bless it as verified official

The stable long-term design point is that `official/community` remains the publication channel, while signature validation becomes the separate proof layer.

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

=== Startup local creation

Built-in official sands should be regenerated locally on every startup.

Automatic startup creation should only target the local machine's `~/.config/lince/web/sand`.

It should not publish into the bucket automatically.

Preferred startup behavior:

- build or render every built-in official sand into the local `sand` directory
- overwrite or upsert the local generated copies
- if one package fails to build or write, log it and continue startup

There should be no startup decision point around bucket publication in the current simplified model.

=== Manual publication direction

Bucket publication should be explicit and manual.

Preferred behavior:

- the user chooses a local sand package in the sand export or sand publisher widget
- the host validates the package automatically
- if publication succeeds, the host uploads the versioned package to the bucket and upserts the publication record
- no startup flow should publish into the bucket on the user's behalf

=== Versioning direction

Published sands should use versioned bucket paths while keeping one stable canonical publication record per slug.

Preferred model:

- stable record identity per slug
- package version from the widget manifest
- versioned bucket prefix per release
- canonical `record_resource_ref` points to the current release
- `record_extension(namespace = "lince.dna")` stores the current canonical version

Recommended release policy:

- same version and same bytes: no-op
- same version and different bytes: hard fail
- newer version: upload new versioned files and repoint the canonical resource ref
- older version: refuse downgrade

This preserves stable discovery while still allowing historical artifact paths.

=== Automatic verification

Sand verification should be automatic.

That means:

- parse and validate package metadata automatically during preview, install, and publish
- verify package format automatically for both `.html` and `.lince`
- if signature-based official verification is later enabled, perform that verification automatically too
- do not require extra user approval for package or signature validation steps

=== Retention and pruning

Recommended default retention policy for published sand artifacts:

- keep the current canonical version
- keep the previous two versions
- never prune versions younger than 30 days
- prune only after a successful publish of a newer version

Local built-in sand copies under `~/.config/lince/web/sand` are reproducible artifacts.

Those local generated copies do not need the same historical retention as bucket-published releases.

=== Migration boundary

The GitHub-backed DNA catalog described elsewhere in the documentation should be treated as transitional.

Once bucket-backed DNA publication and dedicated organ endpoints exist, runtime discovery and installation should use the bucket-plus-record flow described here.
