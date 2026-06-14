# Networking And Sync

The implemented package-sync shape now lives in [TRANSFER](TRANSFER.md). This note keeps the remaining networking plan that is not implemented yet.

## Remaining P2P Discovery

The current implementation can post/import Transfer packages, expose packages since a cursor, retry an outbox, and cache public/permitted gossip packages. It does not yet implement topic-based or contact-list-based peer discovery.

Remaining discovery goals:

- A Cell publishes topics it wants others to discover.
- Other Cells subscribe to those topics or to specific nodes.
- A server or known node introduces Cells to each other by topic.
- The user chooses a discovered Cell to connect to directly.
- The contact/Organ list stores peer Cells with base URL and optional public key.
- A Cell fetches visible Transfer summaries from selected peers.
- Visibility rules decide what fields are included in discovery results.

The cache is not the source of truth. It is a menu of known Needs, Contributions, Transfers, and nodes. It helps a Cell expand its understanding of the world without forcing every node to trust every other node.

## Transfer Package

The package is the portable Transfer payload used for post/import, sync, and gossip. It should use the structured Transfer model directly instead of carrying contribution/need-only fields forward.

Package work that remains:

- Serialize structured parties, items, interactions, scoped agreements, confirmations, settlements, visibility, messages, and quantity influence.
- Read package data into structured backend rows first, then expose sand-specific projections from those rows.
- Drop package-level assumptions that a Transfer has exactly one contribution side and one need side.
- Include enough event data for deterministic validation: event kind, actor, payload, previous event reference/hash, signature, and validation state.
- Apply visibility filtering before packages are emitted to peers, streams, or public gossip caches.

## Structured Projections

Networking and widgets should consume read models, not raw table rows.

Projection work that remains:

- Add Transfer summary and detail projections over structured rows.
- Keep any contribution/need-only shape as an internal adapter only while the current sand migrates.
- Add projection queries for parent/child aggregate state and dependency state.

## Future Cache Tables

The implemented `transfer_gossip_package` table stores package JSON. A richer discovery system can add explicit peer/topic/subscription tables later:

```sql
CREATE TABLE cell_peer (
    id INTEGER PRIMARY KEY,
    name TEXT,
    base_url TEXT NOT NULL,
    organ_id INTEGER REFERENCES organ(id),
    public_key TEXT,
    last_seen_at TEXT,
    trust_state TEXT NOT NULL DEFAULT 'unknown',
    CHECK (trust_state IN ('unknown', 'trusted', 'blocked'))
) STRICT;

CREATE TABLE transfer_pubsub_topic (
    id INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE,
    description TEXT
) STRICT;

CREATE TABLE transfer_pubsub_subscription (
    id INTEGER PRIMARY KEY,
    topic_id INTEGER NOT NULL REFERENCES transfer_pubsub_topic(id) ON DELETE CASCADE,
    peer_id INTEGER REFERENCES cell_peer(id) ON DELETE CASCADE,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_sync_at TEXT
) STRICT;

CREATE TABLE transfer_discovery_cache (
    id INTEGER PRIMARY KEY,
    peer_id INTEGER NOT NULL REFERENCES cell_peer(id) ON DELETE CASCADE,
    remote_transfer_id TEXT NOT NULL,
    topic_name TEXT,
    summary_json TEXT NOT NULL,
    event_head_hash TEXT,
    fetched_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    stale_after TEXT,
    CHECK (json_valid(summary_json)),
    UNIQUE (peer_id, remote_transfer_id)
) STRICT;
```

## Future Federation

Do not start with full federation. The item-level agreement and visibility model is already enough complexity.

Later work:

- Direct peer continuation after Organ/server introduction.
- Event hash-chain verification independent of the relay server.
- Coordinator migration events.
- Peer trust states and blocking.
- Field-level visibility filtering on summaries and event logs.
