# Local NOMAD sources

An independent native Lince plugin for a local [Project NOMAD](https://github.com/Crosstalk-Solutions/project-nomad) server. It reads NOMAD over HTTP without authentication tokens, cloud services, AI generation, or direct database access.

This package has its own Cargo workspace and lockfile. It is **not a member of Lince's workspace, bundled by the installer, or installed/enabled by building it**. Its path dependencies reuse the existing plugin protocol and Lingua parser/renderer.

## Build and verify

From the repository root:

```sh
cargo build --locked --manifest-path crates/plugins/plugin-nomad/Cargo.toml --target-dir target/nomad
cargo test --locked --manifest-path crates/plugins/plugin-nomad/Cargo.toml --target-dir target/nomad
cargo clippy --locked --manifest-path crates/plugins/plugin-nomad/Cargo.toml --target-dir target/nomad --all-targets -- -D warnings
```

When running Cargo from this package directory, `.cargo/config.toml` also places artifacts under the repository's `target/nomad/`. Rust 1.89+ is required for the standard library's advisory file lock API; dependencies may require a newer compiler.

## Connected Records

Copy `settings.example.json` to your own settings file and set `base_url` to the local NOMAD server. Set `kiwix_url` to its Information Library address (NOMAD normally publishes Kiwix on port 8090). Both URLs support a reverse-proxy path prefix.

Give `instance_id` a durable, distinct name for each NOMAD. It keeps source identities stable when the server changes address. When omitted, the normalized NOMAD base URL determines that identity. An empty `kiwix_url` links installed archives to NOMAD's library settings instead of guessing an address.

To generate a connected source publication, use absolute paths for the configuration and output directory:

```sh
target/nomad/debug/lince-plugin-nomad --sync /absolute/settings.json /absolute/nomad-sources
```

For continuous refresh:

```sh
target/nomad/debug/lince-plugin-nomad --watch /absolute/settings.json /absolute/nomad-sources 60
```

`--watch` reloads settings each cycle, reports failures on stderr, preserves the last successful export, and stops on Ctrl-C. The interval must be at least five seconds. Each successful refresh prints a JSON report containing the output path, Record count, digest, and whether the file changed.

When ready to connect this to Lince, import the generated `.lingua` file or configure existing Lingua File Sync for its directory. Nothing here connects it automatically. This is the mode to use for a native graph:

- One entrance Record per NOMAD, containing categories, installed archives, and enabled optional catalogs.
- Category Records contain download tiers. Tiers contain source Records. Reused sources have one identity across categories; differing contextual descriptions and size estimates are preserved in `catalog_variants`.
- `nomad-includes-bundle` links a tier to the tier it inherits. It means download bundle inclusion; it does not claim a learning prerequisite. Its meaning is included in `lince.vocabulary`.
- Installed archives and maps use `source-reference` to their catalog descriptions. Older installed ZIM editions can be matched by the catalog resource ID when the catalog advertises a newer edition.
- Knowledge-base collections contain their source files. Indexed ZIM files reference the corresponding installed archive. User uploads get properly encoded local download links.
- Native assertions and clickable `[[title|uid]]` body links connect Records. `lince.resource` extensions provide source representations, and `plugin.lince.nomad.source` preserves source identity, kind, original metadata and relation targets.

Record UIDs and slugs are deterministic, scoped to the configured instance and upstream identity. Updates change those Records' contents while preserving their identities. Generated files reopen after relocation without depending on their previous directory. All quantities start at zero; availability and indexing do not assert learning progress. Trail routes, exercises, annotations and prerequisites remain separately authored Records that can reference these stable UIDs.

Lingua reserves ASCII parentheses, braces and carets in Record titles. Export displays those characters using typographic equivalents and retains the exact upstream title in `original_title`. The upstream identity and other metadata are unchanged.

The output is a source-owned snapshot. Keep authored trail material in separate files. Refresh refuses to overwrite edits to the generated file, including edits written back by Lince File Sync. Reconcile those edits into authored Records before resuming. If you deliberately discard or move an export, use a fresh output directory or review its adjacent state file. No force-overwrite option is provided. Writes are serialized with a process lock, atomically replaced, and journaled for recovery after interruption. An unchanged snapshot does not rewrite the publication.

## Incoming native Sync provider

The same executable implements `lince.plugin-rpc.v1` over stdin/stdout using the existing Lince plugin host. Inspect its manifest without installing it:

```sh
target/nomad/debug/lince-plugin-nomad --manifest
```

It contributes:

| Contribution | Purpose |
| --- | --- |
| `lince.nomad/sources` | Incoming source snapshots, with resumable bounded pulls |
| `lince.nomad/source` | Namespaced source metadata schema |
| `lince.nomad/export` | Action to refresh the connected publication |

Future explicit installation follows the existing native plugin convention:

```sh
target/nomad/debug/lince-plugin-nomad --install /absolute/Lince-data-directory
```

That command copies the executable and manifest; normal Lince package review/enable controls still apply. It is not run by any build hook.

The `sources` provider accepts the settings JSON, `validate`, and `pull`. Its incoming response uses the current `ImportedRecord` contract: external ID, title, body, and metadata. The host currently creates plain Records from that contract. **Incoming Sync preserves relation targets as metadata; connected export creates native assertions.** They are two representations of the sources; use the connected export for trail graphs.

Incoming snapshots are immutable: the external ID includes the content digest, while `metadata.identity` carries the stable canonical source UID. Thus a changed upstream source becomes a new retained snapshot instead of hitting the current host's changed-object conflict or overwriting local notes. Missing upstream objects do not delete retained incoming snapshots. A cursor records a snapshot fingerprint and offset; changes restart enumeration safely, unchanged completed snapshots return no Records, and the host deduplicates replayed snapshots.

The export Action takes these arguments:

```json
{
  "settings": {
    "base_url": "http://127.0.0.1:8080",
    "instance_id": "my-nomad",
    "kiwix_url": "http://127.0.0.1:8090"
  },
  "directory": "/absolute/nomad-sources"
}
```

## HTTP coverage and boundaries

| Endpoint | Usage |
| --- | --- |
| `GET /api/zim/curated-categories` | Categories, tiers, inherited tiers and shared resources; accepts the API array or a catalog manifest envelope |
| `GET /api/zim/list` | Installed archives, including nullable metadata |
| `GET /api/maps/curated-collections` | Map collections and resources, when `include_maps` is enabled |
| `GET /api/maps/regions` | Installed maps, when `include_maps` is enabled |
| `GET /api/zim/wikipedia` | Available Wikipedia editions, when `include_wikipedia` is enabled |
| `GET /api/rag/files` | Knowledge-base files, when `include_documents` is enabled |

Optional backends default to disabled, so an uninstalled AI or maps backend is not required. An enabled endpoint failing aborts the refresh; it is never treated as an empty library. The example enables maps and Wikipedia; enable documents only when that backend is available.

These shapes were checked against NOMAD revision [`c903da48c97bc02f5cdfeff2ebcde00a82dfec8e`](https://github.com/Crosstalk-Solutions/project-nomad/tree/c903da48c97bc02f5cdfeff2ebcde00a82dfec8e): `admin/types/collections.ts`, `admin/types/zim.ts`, `admin/types/rag.ts`, the ZIM/map/RAG services, and `admin/start/routes.ts`. The local HTTP fixtures retain those response shapes, including shared catalog entries, inherited tiers, older installed editions, optional fields, and upload paths needing URL encoding.

Verification passed with 14 integration tests, Clippy with warnings denied, and formatting. A separate local HTTP smoke check serving that revision's complete knowledge, maps, and Wikipedia catalog files generated 153 connected Records (203,533 bytes); a repeated refresh returned the same digest without rewriting the publication. The tests also exercise the actual native plugin host, projection/reference resolution, relocated reopening, remote revisions and removals, bounded pagination, interruption recovery, concurrent writers, and local-edit preservation. No live NOMAD installation was listening on this machine, so that deployment remains unverified. Trail source identity, provenance and zero-quantity behavior were checked; no plugin-specific `|>` requirement was found in the Lince Records.

The plugin fetches only configured NOMAD endpoints. Catalog download URLs and server filesystem paths are retained as source data; it does not read those paths on the Lince machine or fetch those downloads. Credentials, token settings, redirects and environment proxies are not used. Resources whose original download requires entitlement keep that metadata; no entitlement is bypassed. Content attribution and reuse terms remain those of the original source, and unknown terms are not inferred from NOMAD's software license.

This scope imports catalog and file references, not every article in every archive. Kiwix serves the archived content. Kolibri course trees, learner accounts/progress, FlatNotes, map marker editing, embeddings and outbound modifications are separate adapters and are not advertised as supported here.

Responses are limited to 4 MiB per endpoint and 5,000 Records per graph. Incoming pulls respect the host's 100-item, 1 MiB frame, 16 KiB body and 64 KiB metadata limits. Connected exports respect Lingua's 16 MiB limit. Unknown upstream fields are tolerated; malformed required data, conflicting definitions, missing inherited tiers, cycles, oversized responses and unhandled upstream file pagination fail explicitly. No downloaded content or generated publication is checked into this package.
