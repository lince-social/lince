== Extensions

This document is a planning note for a future public extension hub for Lince.

Read `web_components.typ` first for:

- runtime contracts
- host and bridge assumptions
- runtime shape guidance
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
- different runtime shapes

But an extension hub has to classify more than runtime style.
It also has to classify:

- trust level
- package family
- ingestion path
- canonical storage
- compatibility and runtime expectations

That is why runtime descriptions alone are not enough.

=== Primary axes

The extension hub should model at least three different axes:

#table(
  columns: (1.5fr, 1.7fr, 2.8fr),
  [Axis], [Examples], [Why it matters],
  [Channel], [`official`, `community`], [This is the hard trust and governance boundary. It should not be inferred from runtime style.],
  [Family], [`sand`, `db`], [Different families have different payloads, validation rules, and user expectations.],
  [Runtime requirements], [`standalone iframe HTML`, `host-bound stream widget`, `foreign integration`, `engine-driven surface`], [Useful for install warnings and human understanding, but too descriptive to be the main storage category.],
)

Rules:

- channel is not runtime shape
- family is not runtime shape
- runtime shape is not a permission boundary
- runtime notes do not define the canonical folder layout

=== Relationship to runtime guidance

The runtime guidance in `web_components.typ` remains useful, but only as documentation for how a widget behaves.

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

This matters because a runtime label can look like a package type when it is not.

Examples:

- `kanban` is a server-shaped board with rich local state, but that does not make "server-shaped" a package family
- a foreign integration widget and a client-owned renderer may both still be ordinary `sand` packages
- a database example does not need runtime-shape labeling at all

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
- `dna` rebuilds the search catalog TOML

Community submission being "free to push" should mean:

- contributors can submit without maintainer curation before ingest into the community catalog
- `dna` still applies schema and safety validation
- the hub still has a canonical search TOML and checksum records
- maintainers may still rename, replace, or delete packages when they decide to

It should not mean:

- no validation
- no naming rules
- no canonical metadata

=== Near-term on-disk direction

The practical near-term direction should be family-first and channel-explicit on disk.
Packages should be sharded by the first two letters of the package name, similar to the broad shape used by `nixpkgs`.
There should be no JSON catalog file.
Instead, the hub should maintain one search TOML for `sand`.

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
    catalog.toml
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
- runtime notes do not affect the on-disk layout

This keeps the hard official versus community split visible while avoiding one flat directory with too many entries.

=== Catalog TOML

The hub should keep one search TOML for `sand`.

Suggested direction:

```text
sand/catalog.toml
```

Lookup rule:

1. Normalize the package name.
2. Open `sand/catalog.toml`.
3. Read the package record from there instead of scanning the whole tree.

Suggested `sand` catalog shape:

```toml
[packages.hello_world]
title = "Hello World"
description = "Example package"
path = "community/he/hello_world"
```

Rules:

- the catalog is the fast lookup layer
- the canonical package directory remains the artifact source of truth
- `sand` should have only one search TOML in the first version
- the catalog must be updated on create, upsert, rename, promotion, and delete

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
- `title`: optional
- `icon`: optional
- `details`: optional
- `initial_width`: optional
- `initial_height`: optional
- `permissions`: optional list of strings
- `tags`: optional list of strings

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

- `title` defaults to a humanized form of `name`
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

- manifest `icon` should come from `sand.toml` `icon`
- manifest `title` should come from `sand.toml` `title`, or fall back to a humanized form of `name`
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
- update the corresponding catalog entry

On hard delete:

- remove the package directory
- remove the corresponding catalog entry
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
11. `dna` writes or updates the corresponding catalog entry.

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
9. `dna` updates the corresponding catalog entry.

==== Rename

For renames:

1. A maintainer chooses the new package name.
2. The rename must include a semantic version bump because the package metadata and canonical filenames change.
3. `dna` moves the canonical package to the new path.
4. `dna` updates the package metadata name.
5. `dna` appends a line to the relevant family migration file.
6. `dna` updates the old and new catalog entries.

This is the main administrative escape hatch for collision handling and curator intervention.

==== Promotion

For promotion from `community` to `official`:

1. A maintainer selects the existing community package.
2. If the target official name is already used by another official package, promotion is rejected.
3. If an official ingest or promotion takes a name currently held by `community`, `dna` should auto-rename the displaced community package, patch-bump it, and write that rename to the migration file.
4. Promotion must include a semantic version bump because the package metadata changes.
5. `dna` moves the package from `community` to `official`.
6. `dna` updates the package metadata channel.
7. `dna` appends a line to the migration file recording the channel move.
8. `dna` updates the catalog entry.

Promotion should be a move, not a copy.
The package keeps the same name unless a maintainer explicitly renames it.

==== Hard delete

For deletion:

1. Submit a delete request or invoke an explicit delete command with the family, channel, and package name.
2. `dna` removes the canonical package directory.
3. `dna` removes the catalog entry.

The plan does not try to soften the consequences of hard delete.

=== Recommended practical direction for now

The most practical direction today is:

- keep only `sand` and `db`
- store packages by family, then channel, then two-letter prefix, then package name
- use `sand.toml`
- use one `sand/catalog.toml` file for search
- treat the package name as the stable id
- keep package names unique per family, with `official` taking precedence over `community`
- use `lounge/` as the operator drop zone
- keep `official` and `community` as the hard boundary
- let `dna` own checksum and canonical writes
- let `lince` remain a producer only
- require `lower_snake_case` names
- require semantic versions for `sand`
- use loose `HTMLHint` validation for `sand`
- keep runtime expectations in documentation and compatibility fields
- treat `db` as manual storage for now rather than part of the automated metadata flow

This is much simpler than a generalized hub model, and it matches the current stage of the project better.

=== Criticisms of the current direction

Several criticisms remain even after simplifying the design:

- First-come, first-served naming is simple, but the real answer to disputes is maintainer override rather than a neutral policy.
- Using the package name as the stable id is pragmatic, but it means renames need explicit migration bookkeeping.
- The two-letter prefix sharding is practical, but it makes names and path normalization a hard policy boundary instead of a forgiving one.
- Official widgets like Kanban still need richer runtime metadata than a generic standalone HTML package, even if the storage layout stays simple.
- A single `sha256.txt` is enough operationally, but it intentionally gives up richer provenance.
- A single `sand/catalog.toml` is simpler than many map files, but it becomes one shared file that changes on every catalog mutation.
- Allowing arbitrary assets keeps the system flexible, but it also pushes all asset hygiene and review onto maintainers.

=== Concrete decisions

The remaining policy points should be treated as decided for the current plan:

- invalid package names are rejected if they fail the `lower_snake_case` pattern, exceed `200` characters, or use a reserved name
- official-widget compatibility metadata is optional, but if present it must pass schema validation during ingest
- `sand.toml` may later grow `homepage`, `repository`, and compatibility fields without changing the current package identity model
- every canonical package change requires a semantic version bump, including rename and promotion
- if `db` later becomes automated, it should get its own metadata file and, if needed, its own search TOML

=== Working conclusion

The extension hub can be much simpler than the first draft.

The current best direction is:

- `sand` and `db` only
- `sand.toml` drives the automated ingest flow
- one `sand/catalog.toml` avoids full directory scans
- package name as stable id
- two-letter path sharding
- one `lounge/` drop zone
- `dna` owns checksum and canonical writes
- `official` and `community` stay explicit
- `sand` gets its channel from `sand.toml`
- promotion is a move, renames are recorded in migrations, and both update `sand/catalog.toml`

Runtime guidance still matters for understanding widget behavior, but it does not need a dedicated taxonomy field or storage role in this plan.

=== Next board step

After the ingest and catalog rules are in place, the next practical UI step should be board-side package pickup from published organ records whose artifacts live in the bucket under the `lince/dna/sand` tree.

The intended direction is:

1. Add one more choice to the existing `Add card` popover for remote catalog pickup.
2. Label it something direct such as `Hub` or `DNA`.
3. Keep the existing `Importar` and `Local` flows unchanged.
4. Open a new Maud-rendered modal instead of navigating away or inventing a second screen.

The modal should follow the same broad shape as the current local catalog modal:

- search field at the top
- short catalog summary text
- scrollable list of matching package names
- one clear action per result to install and add the card

Suggested HTML shape in the page template:

```text
button#add-card-dna-button
div#dna-packages-modal-backdrop
section#import-modal.import-modal--catalog
input#dna-packages-search
div#dna-packages-summary
div#dna-package-list
```

This should be rendered in Maud next to the existing local catalog modal, not assembled ad-hoc in client JavaScript.

The search path should use one catalog TOML rather than full repository scans.

Practical rule:

1. Normalize the user query to the package-name shape.
2. Take the first two letters.
3. Fetch `sand/catalog.toml` from `main`.
4. Filter the package records in that TOML file on the client or through the backend proxy.
5. Render the matching package rows in the modal.

The first version can stay simple:

- search by package name first
- treat empty search as "show nothing yet" or "type at least two letters"
- only target `sand`
- prefer `official` entries first when both channels are ever shown together

The fetch source should be the `main` branch of the public `dna` repository.
The safer runtime path is a backend proxy in `lince`, even if the upstream source is GitHub, because that avoids client-side CORS assumptions and keeps remote fetching inside the server boundary.

Suggested remote sources:

- `sand/catalog.toml` for search
- `sand/<channel>/<prefix>/<package-name>/sand.toml` for package metadata
- `sand/<channel>/<prefix>/<package-name>/<package-name>_metadata.html` for installation

The installation path should reuse the current imported-widget flow as much as possible.

Practical direction:

1. The user selects a package row in the modal.
2. `lince` fetches that package's `sand.toml` and `<package-name>_metadata.html` from `dna/main`.
3. `lince` turns that into the same preview payload shape already used for local/imported widgets.
4. The user sees the normal preview and confirms.
5. On confirm, `lince` stores the downloaded widget into the local package catalog and creates the card.

This means the remote hub pickup should land in the same local installed-catalog path as other widgets after download.
The hub is the source, but the board still works with a local installed package once the user adds it.

The shortest implementation sequence should be:

1. add `Hub` or `DNA` to the `Add card` popover
2. add the Maud modal shell
3. add backend endpoints that proxy GitHub `main` TOML and HTML fetches
4. add client search + result rendering using `sand/catalog.toml`
5. add preview + confirm using the existing imported-widget card creation path

==== Board UX plan

The board-side experience should be:

1. Enter edit mode.
2. Open `Add card`.
3. Choose `DNA`.
4. The modal opens with an empty result list and a message such as `Digite pelo menos duas letras para buscar no hub.`
5. After the user types two or more characters, the board searches the hub.
6. The modal shows matching package rows with enough metadata to decide quickly.
7. Selecting a row opens the same preview surface already used for imported widgets.
8. Confirming the preview downloads the package into the local installed catalog and creates the card.

The first version should keep the interaction narrow:

- one search field
- one result list
- one preview-confirm path
- no multi-select
- no background synchronization

==== DNA search files

For the board flow, `dna` should expose one TOML search file in `main`:

```text
sand/catalog.toml
```

The purpose of `sand/catalog.toml` is:

- provide the search rows for the modal
- avoid repository directory scans
- keep one compact lookup file for the whole `sand` family

Suggested shape:

```toml
[packages.hello_world]
title = "Hello World"
description = "Example package"
path = "official/he/hello_world"
```

The package name lives in the TOML key.
The row metadata for search is only:

- `title`
- `description`
- `path`

==== Search behavior

The search contract should be simple and deterministic:

1. trim the query
2. lowercase it
3. normalize spaces and dashes into `_`
4. reject characters that do not fit package-name search
5. if fewer than two characters remain, do not hit the network
6. fetch `sand/catalog.toml`
7. filter the package rows by package name first
8. if desired, also match `title` and `description`

Result ordering should be:

1. exact package-name prefix match
2. package-name substring match
3. title match
4. description match
5. alphabetical by title

The first version should fetch the whole `sand/catalog.toml` file for each search refresh or from cache.
That keeps the search contract simple.

==== Modal contents

Each result row in the Maud modal should show:

- icon
- title
- package name
- short description

Each row should have one explicit action:

- `Preview`

The modal should not install immediately from the search list.
Preview first, then confirm through the existing preview modal.

==== Backend route plan

The backend should own all remote GitHub fetches.
The browser should never fetch raw GitHub URLs directly.

Suggested new API surface:

```text
GET  /api/packages/dna/catalog
GET  /api/packages/dna/search?q=<query>
GET  /api/packages/dna/{channel}/{package_name}/preview
POST /api/packages/dna/{channel}/{package_name}/install
```

Responsibilities:

- `catalog`: fetch and cache `sand/catalog.toml`
- `search`: normalize the query, fetch `sand/catalog.toml`, filter rows, and return lightweight summaries
- `preview`: fetch `sand.toml` plus `<package-name>_metadata.html`, parse them into the same preview payload shape already used by `/api/packages/local/{package_id}` and `/api/packages/preview`
- `install`: fetch the same remote package, persist it into the local widget catalog, and return the installed preview payload

This keeps the remote-hub logic inside the backend and lets the front-end reuse the existing package UI model.

==== Remote source rules

The first version should fetch only from:

- accessible organs exposed in the local host catalog
- published `record_extension(namespace = "lince.dna")` entries
- canonical `record_resource_ref(provider = "bucket", resource_kind = "sand")` artifacts under `lince/dna/sand`

The backend should translate those publication records to authenticated bucket fetches.

The expected remote files for a package are:

- `sand.toml`
- `<package-name>_metadata.html`

The board should not download the plain `<package-name>.html` file for installation in the first version.
The metadata-expanded HTML is the correct transport because it already satisfies the current embedded-manifest package contract used by local install and preview flows.

==== Local install rule

When `lince` installs a package downloaded from `dna`:

1. fetch `<package-name>_metadata.html`
2. parse it as a normal Lince package
3. persist it locally under the filename `<package-name>.html`

This rule is important.
The remote transport file is `<package-name>_metadata.html`, but the local installed filename should still be `<package-name>.html`.
Otherwise the local package id would drift to `<package-name>_metadata`, which is wrong.

The installed package should land in the same local catalog used today for uploaded widgets:

```text
~/.config/lince/web/widgets/<package-name>.html
```

After that, the package behaves like any other locally installed widget.

==== Collision and overwrite rule

For the first version, the simplest local rule should be:

- if the package is already installed locally and the downloaded version is newer, overwrite the local installed copy
- if the package is already installed locally and the downloaded version is the same, replace it anyway if the user explicitly installs it from the hub
- if the package exists only as a built-in official sand widget, allow the downloaded local copy to shadow it

This is explicit user intent.
Choosing a package from the hub is enough reason to let the local installed copy win over the built-in rendered official copy.

==== Caching rule

The backend should cache remote TOML and HTML responses in memory.

The first version can stay simple:

- cache `catalog.toml` for 5 minutes
- cache preview payloads for 1 minute

Cache invalidation does not need to be perfect.
The branch is `main`, not a versioned immutable archive, so short-lived stale reads are acceptable for the first version.

==== Error handling

The modal and preview flow should surface short concrete errors:

- `Falha ao buscar o catalogo do hub.`
- `Nenhum pacote corresponde a essa busca.`
- `Falha ao baixar o pacote selecionado.`
- `O pacote remoto nao possui metadados validos para instalacao.`
- `Falha ao instalar o pacote do hub no catalogo local.`

The modal should stay open on search and preview errors.
Only a successful install should close the remote modal and continue into the normal add-card success path.

==== Data shape reuse

The implementation should reuse the existing local package data shapes as much as possible.

Practical rule:

- search rows should look like a minimal summary built from package name plus `title`, `description`, and `path`
- preview responses should look like the existing `PackagePreview`
- install responses should also return `PackagePreview`

This keeps the new `DNA` path close to the existing local/imported widget path and avoids inventing a second package model in the front-end.

==== Implementation order

The practical implementation order should be:

1. extend `dna` ingest/rebuild so it writes `sand/catalog.toml`
2. add `DNA` to the add-card popover in Maud
3. add the Maud modal shell for remote search
4. add backend routes for `catalog`, `search`, `preview`, and `install`
5. add backend GitHub fetch + in-memory cache
6. add client-side search state and result rendering from `sand/catalog.toml`
7. route row click into the existing preview-confirm flow
8. persist the downloaded package locally as `<package-name>.html`
9. create the card using the existing imported-widget card creation path
