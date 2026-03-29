== Extensions

This document is a planning note for a future public extension hub for Lince.

Read `web_components.typ` first for:

- runtime contracts
- host and bridge assumptions
- Sand Classes
- standalone versus official widget packaging

Read `kanban.typ` for one concrete example of an official widget that is not only "some HTML", but a hybrid runtime surface with a stronger contract.

Current code is more authoritative than this document when there is conflict.
This note is intentionally incomplete and keeps unresolved issues visible instead of pretending they are solved.

=== Goal

The intended direction is:

- `lince` is a producer only
- `dna` is the public extension hub and canonical catalog
- `dna` owns checksum generation
- `dna` owns canonical writes, promotion, renames, and deletion
- community submission is open
- official publication is curated
- `upsert` is the normal write operation
- hard delete is allowed and explicit

This means the extension hub is not the same thing as widget authoring.
It is the catalog, packaging, ingestion, and governance layer around authored artifacts.

=== Why this needs its own plan

The present web component documentation already distinguishes:

- official `sand/` widgets
- standalone imported widgets
- different Sand Classes

But an extension hub has to classify more than runtime style.
It also has to classify:

- trust level
- package family
- ingestion path
- canonical storage
- compatibility and runtime expectations

That is why Sand Classes alone are not enough.

=== Primary axes

The extension hub should model at least three different axes:

#table(
  columns: (1.5fr, 1.7fr, 2.8fr),
  [Axis], [Examples], [Why it matters],
  [Channel], [`official`, `community`], [This is the hard trust and governance boundary. It should not be inferred from class or runtime style.],
  [Family], [`sand`, `db`], [Different families have different payloads, validation rules, and user expectations.],
  [Class hint], [`Engineer`, `Clown`, `Monk`, `Mercenary`, `Astromancer`, hybrids], [Useful for discovery and reasoning about runtime shape, but too abstract to be the main storage category.],
)

Rules:

- channel is not class
- family is not class
- class is not a permission boundary
- class is descriptive metadata, not the canonical folder layout

=== Relationship to Sand Classes

Sand Classes remain useful, but only as an abstraction layer for widgets.

They help answer:

- where truth lives
- how truth reaches the surface
- who shapes the UI
- where interaction state lives
- how foreign or portable the runtime is

They do not answer:

- whether an artifact is official or community
- whether the payload is a widget or a database example
- how checksums are owned
- how deletion or promotion works
- how the hub should store the artifact on disk

This matters because a class-like label can look like a package type when it is not.

Examples:

- `kanban` is `Engineer with Clown traits`, but that does not make "Engineer" or "Clown" a package family
- a `Mercenary` widget and a `Monk` widget may both still be ordinary `sand` packages
- a database example does not naturally fit any Sand Class at all

=== Kanban as warning

`kanban.typ` is useful here because it shows a real official widget is:

- hybrid
- host-configured
- instance-aware
- stream-backed
- action-oriented
- not portable in the same way as a standalone imported HTML widget

That means the extension hub cannot treat every extension as if it were just a static asset plus a friendly name.

At minimum, official widget metadata will likely need to describe:

- permissions
- required host configuration
- runtime contract style
- compatibility expectations
- whether the package is meant for import, direct hosting, or internal official use

=== Planned hub responsibilities

The intended split is:

- `lince` may produce code, HTML, source snapshots, or other raw extension payloads
- `lince` does not own canonical publication
- `dna` ingests submissions from a lounge area
- `dna` validates the submission according to family rules
- `dna` computes checksums
- `dna` performs upsert into the canonical tree
- `dna` performs hard delete when explicitly requested
- `dna` regenerates indexes

Community submission being "free to push" should mean:

- contributors can submit without maintainer curation before ingest into the community catalog
- `dna` still applies schema and safety validation
- the hub still has a canonical index and checksum record
- maintainers may still rename, replace, or delete packages when they decide to

It should not mean:

- no validation
- no naming rules
- no canonical metadata

=== Near-term on-disk direction

The practical near-term direction should be family-first and channel-explicit on disk.
Packages should be sharded by the first two letters of the package name, similar to the broad shape used by `nixpkgs`.
There should be no JSON catalog file.
Instead, the hub should maintain sharded TOML maps for fast lookup.

Suggested direction:

```text
dna/
  lounge/
    <package-name>/
      index.html?        # required for sand packages
      sand.toml?         # sand package metadata
      *.sqlite
      ...
  sand/
    map/
      <prefix>.toml
    official/
      <prefix>/
        <package-name>/
          <package-name>.html
          <package-name>_metadata.html
          sand.toml
          sha256.txt
    community/
      <prefix>/
        <package-name>/
          <package-name>.html
          <package-name>_metadata.html
          sand.toml
          sha256.txt
  db/
    map/
      <prefix>.toml
    official/
      <prefix>/
        <package-name>/
          ...
          sha256.txt
    community/
      <prefix>/
        <package-name>/
          ...
          sha256.txt
```

Example:

```text
sand/official/he/hello_world/
```

Here the package name is the stable package identity used in the path.

Rules:

- package names are first-come, first-served
- the package name is the stable package id
- package names must be `lower_snake_case`
- there must not be two packages with the same name in the same family across `official` and `community`
- `official` has naming priority over `community`
- when a community package conflicts with an official package, the community package must be renamed
- maintainers may rename community or official packages when needed
- the prefix is derived from the first two letters of the `lower_snake_case` package name
- if the package name has fewer than two letters, use the available letters
- classes do not affect the on-disk layout

This keeps the hard official versus community split visible while avoiding one flat directory with too many entries.

=== TOML map

The hub should keep a sharded TOML map for each family.

Suggested direction:

```text
sand/map/<prefix>.toml
db/map/<prefix>.toml
```

Lookup rule:

1. Normalize the package name.
2. Derive the two-letter prefix.
3. Open the corresponding TOML map.
4. Read the package record from there instead of scanning the whole tree.

Suggested `sand` map shape:

```toml
[packages.hello_world]
channel = "community"
path = "community/he/hello_world"
version = "0.2.0"
sha256 = "..."
author = "Example Author"
description = "Example package"
tags = ["demo", "widget"]
class = "Engineer"
html = "hello_world.html"
metadata_html = "hello_world_metadata.html"
```

Rules:

- the map is the fast lookup layer
- the canonical package directory remains the artifact source of truth
- the map must be updated on create, upsert, rename, promotion, and delete
- the map must stay sharded by prefix so it remains practical when the hub grows

=== Rename history

Package renames should be recorded.

The simplest direction is a per-family migrations ledger such as:

```text
sand/migrations.txt
db/migrations.txt
```

Each rename entry can be append-only and human-readable, for example:

```text
2026-03-29T12:00:00Z community/tes -> community/test
2026-03-29T12:10:00Z community/hello_world -> official/hello_world
```

This does not need to be a heavy migration system.
It only needs to preserve package rename history in a form that is easy to inspect.

=== Package metadata

The hub should not use `manifest.json` as the package metadata file.

Use family-specific TOML files instead:

- `sand.toml` for web components

For `sand`, the package should normally contain:

- `index.html`
- `sand.toml`
- optional extra assets if the package format later allows them

For now, extra assets should be allowed broadly.
That includes arbitrary supporting files and directories.
Asset hygiene can stay as a manual maintainer responsibility rather than an automated ingest restriction.

In canonical storage, the ingested package should normally contain:

- `<package-name>.html`
- `<package-name>_metadata.html`
- `sand.toml`
- optional extra assets if the package format later allows them

The family-specific TOML file is where package metadata belongs.
There is no need for one generic package manifest for all families in the first version.

For `db`, metadata remains intentionally unspecified for now.
At this stage, `db` is just storage for SQLite database files and is not the center of the automated metadata plan.

==== `sand.toml`

`sand.toml` should be closer in spirit to `Cargo.toml` package metadata than to an ad-hoc JSON manifest.

Suggested first version fields:

- `name`: required
- `channel`: required, either `official` or `community`
- `version`: required
- `author`: required
- `description`: required
- `details`: optional
- `initial_width`: optional
- `initial_height`: optional
- `permissions`: optional list of strings
- `tags`: optional list of strings
- `class`: optional string or hybrid class label

Rules:

- `name` must be `lower_snake_case`
- `name` must be at most `200` characters
- `name` must match `^[a-z][a-z0-9]*(?:_[a-z0-9]+)*$`
- `name` must not be one of the reserved names `official`, `community`, `sand`, `db`, `lounge`, `map`, `migrations`, `con`, `prn`, `aux`, `nul`, `com1` through `com9`, or `lpt1` through `lpt9`
- `version` must be valid semantic versioning text
- `initial_width`, if present, must be an integer from `1` to `6`
- `initial_height`, if present, must be an integer from `1` to `6`
- the lounge directory name and `sand.toml` `name` should agree

`sand.toml` should be the source of truth for package name and channel during ingest.

Defaults for metadata-expanded HTML generation:

- `details` defaults to `description`
- `initial_width` defaults to `4`
- `initial_height` defaults to `3`
- `permissions` defaults to `[]`

==== Future compatibility metadata

Simple standalone `sand` packages do not need much compatibility metadata beyond ordinary package fields.

Official or host-bound widgets may later need optional compatibility fields such as:

- `required_permissions`: optional list of required host permissions
- `required_host_meta`: optional list of required host metadata keys such as `server_id` or `view_id`
- `host_contract_version`: optional string describing the expected host runtime contract
- `min_lince_version`: optional semantic version describing the minimum supported Lince version

If these fields are absent, the conservative assumption should be that the package is a simpler standalone-style package rather than a strongly host-bound official runtime surface.

If these fields are present:

- ingest should validate their syntax and basic semantics
- hub ingest should reject malformed compatibility metadata
- runtime compatibility should be enforced when the package is installed or executed, not by guessing during catalog ingest

==== Sand HTML files

When a `sand` package is ingested:

- the lounge `index.html` should be copied into canonical storage as `<package-name>.html`
- `dna` should also generate `<package-name>_metadata.html`

The purpose of `<package-name>_metadata.html` is to provide the current embedded metadata block expected by the existing standalone widget contract.

When generating that embedded metadata block:

- manifest `title` should come from `sand.toml` `name`
- manifest `author` should come from `sand.toml` `author`
- manifest `version` should come from `sand.toml` `version`
- manifest `description` should come from `sand.toml` `description`
- manifest `details` should come from `sand.toml` `details`, or fall back to `description`
- manifest `initial_width` should come from `sand.toml` `initial_width`, or default to `4`
- manifest `initial_height` should come from `sand.toml` `initial_height`, or default to `3`
- manifest `permissions` should come from `sand.toml` `permissions`, or default to `[]`

If the source HTML already contains embedded metadata, `dna` may still regenerate the metadata version so that the canonical metadata-expanded file is deterministic.

This keeps:

- the original author HTML, preserved under the package name
- a metadata-expanded HTML file, compatible with the current embedded-manifest expectations

=== Package identity

The simplest direction is:

- the package id is the package name
- the package id does not change
- upsert replaces the current canonical package at that path
- hard delete removes that path

This is a deliberately simple model.
It avoids introducing a second hidden identifier before there is a real reason for one.

=== Validation

For now, only `sand` needs a stronger validation story.

The minimum first-version validation for `sand` should be:

- `index.html` exists
- `sand.toml` exists
- `index.html` passes a loose HTML validity check through `HTMLHint`
- `sand.toml` matches the expected schema
- the package path and the `name` declared in `sand.toml` agree
- the `channel` declared in `sand.toml` is valid
- the `version` declared in `sand.toml` is valid semantic version text
- `permissions`, if present, is a list of strings
- the package `name` fits `lower_snake_case`
- the package `name` is at most `200` characters

The HTML validation should stay loose.
It only needs to confirm that the document is valid enough to be treated as HTML, more like an HTML linter than a strict runtime certification.

The ingest command should show short user-facing error messages on validation failure.
Recommended messages:

- `missing sand.toml: sand packages require sand.toml`
- `missing index.html: sand packages require index.html`
- `invalid package name 'Hello World': use lower_snake_case with at most 200 characters`
- `invalid channel 'beta': expected 'official' or 'community'`
- `invalid version '1.0': expected semantic versioning such as '1.0.0'`
- `version must increase: current canonical version is '0.2.0'`
- `community package 'hello_world' conflicts with official package 'hello_world': choose another name`
- `html validation failed: document is not valid enough for sand ingest`
- `reserved package name 'official': choose another name`

`HTMLHint` should be the first-version linter.
The practical tooling direction is to have `mise ingest` install or expose `HTMLHint` so the ingest command has the linter available.

=== Checksums

`dna` owns checksum generation.

The first version does not need checksum history or lineage tracking.
It only needs the current checksum of the canonical package written to `sha256.txt`.

The checksum should be computed across the canonical package files in alphabetical relative-path order.

The deterministic hashing rule should be:

- sort canonical file paths alphabetically
- feed each relative path and its file bytes into the hash in that order

This avoids ambiguity once a package contains more than one file.

On upsert:

- require a semantic version bump relative to the current canonical package
- recompute the checksum
- overwrite `sha256.txt`
- update the corresponding TOML map entry

On hard delete:

- remove the package directory
- remove the corresponding TOML map entry
- leave the filesystem and migrations ledger as the remaining source of truth

=== Lounge ingestion

The ingest path should be `lounge/`, not `.lounge/`.

The simplest direction is:

- `lounge/<package-name>/...`

The ingest command can infer the family from the package contents.

The first version inference rule should be:

- if `sand.toml` is present, treat the package as `sand`
- `sand` requires `index.html`
- if `sand.toml` is absent, the package is not part of the automated sand ingest flow

For now, `db` remains out of the automated metadata validation path.
Maintainers may still place SQLite database files in the `db/` tree manually.

That is enough for now because `sand` is the only fully-specified automated package flow in this plan.

=== Practical operations

==== Create

For creation:

1. Put the package under `dna/lounge/<package-name>/`.
2. Run the `dna` ingest command.
3. `dna` treats the package as `sand` if `sand.toml` is present.
4. `dna` validates the package according to the `sand` rules.
5. `dna` reads the target channel from package metadata.
6. `dna` checks for a name collision in that family across `official` and `community`.
7. If the package is community and the name is taken by an official package, ingest must reject it until the community package uses another name.
8. `dna` computes the checksum.
9. `dna` creates the canonical directory if it does not already exist.
10. `dna` writes the canonical package into the chosen family and channel.
11. `dna` writes or updates the corresponding TOML map entry.

This gives the operator the simple workflow of "put it somewhere, run one command, done".

==== Upsert

For updates:

1. Reuse the same package name.
2. Submit the new package contents.
3. Run ingest again.
4. `dna` validates again.
5. `dna` reads the target channel from package metadata.
6. `dna` requires the submitted semantic version to be greater than the current canonical version.
7. `dna` replaces the canonical package at the same path.
8. `dna` recomputes checksum.
9. `dna` updates the corresponding TOML map entry.

==== Rename

For renames:

1. A maintainer chooses the new package name.
2. The rename must include a semantic version bump because the package metadata and canonical filenames change.
3. `dna` moves the canonical package to the new path.
4. `dna` updates the package metadata name.
5. `dna` appends a line to the relevant family migration file.
6. `dna` updates the old and new TOML map entries.

This is the main administrative escape hatch for collision handling and curator intervention.

==== Promotion

For promotion from `community` to `official`:

1. A maintainer selects the existing community package.
2. If the target official name is already used by another official package, promotion is rejected.
3. If the target official name conflicts with a community package, the conflicting community package must be renamed first and that rename must be written to the migration file.
4. Promotion must include a semantic version bump because the package metadata changes.
5. `dna` moves the package from `community` to `official`.
6. `dna` updates the package metadata channel.
7. `dna` appends a line to the migration file recording the channel move.
8. `dna` updates the TOML map entry.

Promotion should be a move, not a copy.
The package keeps the same name unless a maintainer explicitly renames it.

==== Hard delete

For deletion:

1. Submit a delete request or invoke an explicit delete command with the family, channel, and package name.
2. `dna` removes the canonical package directory.
3. `dna` removes the TOML map entry.

The plan does not try to soften the consequences of hard delete.

=== Recommended practical direction for now

The most practical direction today is:

- keep only `sand` and `db`
- store packages by family, then channel, then two-letter prefix, then package name
- use `sand.toml`
- use sharded TOML maps for lookup
- treat the package name as the stable id
- keep package names unique per family, with `official` taking precedence over `community`
- use `lounge/` as the operator drop zone
- keep classes only as descriptive metadata
- keep `official` and `community` as the hard boundary
- let `dna` own checksum and canonical writes
- let `lince` remain a producer only
- require `lower_snake_case` names
- require semantic versions for `sand`
- use loose `HTMLHint` validation for `sand`
- treat `db` as manual storage for now rather than part of the automated metadata flow

This is much simpler than a generalized hub model, and it matches the current stage of the project better.

=== Criticisms of the current direction

Several criticisms remain even after simplifying the design:

- First-come, first-served naming is simple, but the real answer to disputes is maintainer override rather than a neutral policy.
- Using the package name as the stable id is pragmatic, but it means renames need explicit migration bookkeeping.
- The two-letter prefix sharding is practical, but it makes names and path normalization a hard policy boundary instead of a forgiving one.
- Official widgets like Kanban still need richer runtime metadata than a generic standalone HTML package, even if the storage layout stays simple.
- A single `sha256.txt` is enough operationally, but it intentionally gives up richer provenance.
- TOML maps improve lookup, but they add another layer that can drift if ingest tooling is buggy.
- Allowing arbitrary assets keeps the system flexible, but it also pushes all asset hygiene and review onto maintainers.

=== Concrete decisions

The remaining policy points should be treated as decided for the current plan:

- invalid package names are rejected if they fail the `lower_snake_case` pattern, exceed `200` characters, or use a reserved name
- official-widget compatibility metadata is optional, but if present it must pass schema validation during ingest
- `sand.toml` may later grow `homepage`, `repository`, and compatibility fields without changing the current package identity model
- every canonical package change requires a semantic version bump, including rename and promotion
- if `db` later becomes automated, it should get its own TOML metadata file and matching sharded TOML maps

=== Working conclusion

The extension hub can be much simpler than the first draft.

The current best direction is:

- `sand` and `db` only
- `sand.toml` drives the automated ingest flow
- sharded TOML maps avoid full directory scans
- package name as stable id
- two-letter path sharding
- one `lounge/` drop zone
- `dna` owns checksum and canonical writes
- `official` and `community` stay explicit
- `sand` gets its channel from `sand.toml`
- promotion is a move, renames are recorded in migrations, and both update the TOML maps

Sand Classes still matter for understanding widget runtime style, but they no longer need to carry any storage burden in this plan.
