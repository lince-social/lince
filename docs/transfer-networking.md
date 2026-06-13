# Networking And Sync

A Lince Cell should be able to act as a node in a wider network. A node can publish a cache of visible Transfer information, receive caches from other nodes, and choose whether to keep or relay that information.

## P2P Discovery And Pub/Sub Cache

This is close to pub/sub:

- A Cell publishes topics it wants others to discover.
- Other Cells subscribe to those topics or to specific nodes.
- A server or known node can introduce Cells to each other.
- After introduction, Cells can sync directly.
- A Cell may cache public or permitted Transfer summaries from other Cells.
- A Cell may periodically ask for updates, or keep the cached information as stale background knowledge.
- Visibility rules still decide what fields are included in the cache.

The cache is not source of truth. It is a menu or index of known Needs, Contributions, Transfers, and nodes. It helps a Cell expand its understanding of the world without forcing every node to trust every other node.

## Discovery Server Path

The practical early path:

1. A central or Organ server lists reachable Cells and public topics.
2. A Cell asks the server for nodes matching a topic.
3. The user chooses a Cell to connect to directly.
4. Lince adds that Cell to the Organ/contact list.
5. The local Cell calls the remote Cell directly for visible Transfer summaries.
6. If both Cells choose to participate in a Transfer, they sync the Transfer event log directly or through the coordinator.

This keeps the central server useful for discovery without making it the permanent source of truth for every Transfer.

## Coordinator Event Log And Eventual Consistency

For Transfers, p2p sync should be based on the Transfer event log.

The event log is the sync unit because it explains how a Transfer reached its current state. A materialized Transfer row can be cached, but the log is what participating Cells need to compare, replay, mirror, and eventually verify with signatures.

Recommended rule:

- One coordinator orders writes for a Transfer at a time.
- The coordinator stores the operational source of truth while it coordinates that Transfer.
- Participating Cells may keep a local replica of the visible event log.
- Replicas are eventually consistent: they can be behind, then catch up by fetching events after their last synced event.
- Local drafts can exist before submission, but once a proposal is sent to the coordinator, the coordinator assigns the canonical event order.
- The first apple implementation can use my local Lince as coordinator.

This means a Transfer created from "my Cell" can live in more than one place:

- My local Lince can keep the coordinator copy.
- My Cell can keep a local draft before the coordinator accepts it.
- The Lince server Organ can keep a visible mirror as a participant.
- The coordinator copy is authoritative for event ordering until coordinator migration exists.
- Participants catch up by syncing events from the coordinator.

Hello-world sync does not need full direct p2p. It only needs the same shape:

1. My Cell creates a local proposal draft.
2. The draft is submitted to my local Lince coordinator.
3. Local Lince appends canonical Transfer events.
4. The Lince server Organ stores or displays the visible coordinator events.
5. Each participant tracks the last event it has seen.
6. When new events happen, each participant asks the coordinator for events after that cursor.

Coordination does not mean centralization. Coordination means one node is responsible for:

- Assigning event order for one Transfer.
- Validating state-changing actions.
- Rejecting stale or invalid writes.
- Maintaining the event hash chain.
- Serving visible events to participants.
- Producing the materialized Transfer state from those events.

Different Transfers can have different coordinators. A local Lince can coordinate one Transfer, an Organ server can coordinate another, and later a coordinator migration event can move responsibility from one node to another.

The system does not need a central global coordinator. A central or Organ server is useful for discovery, introduction, hosting always-online Organs, or relay, but it should not be required for every Transfer.

Possible sync cursor table:

```sql
CREATE TABLE transfer_sync_cursor (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    peer_subject_id INTEGER NOT NULL REFERENCES transfer_visibility_subject(id),
    last_event_id INTEGER,
    last_event_hash TEXT,
    last_synced_at TEXT,
    UNIQUE (transfer_id, peer_subject_id)
) STRICT;
```

Possible replica marker:

```sql
CREATE TABLE transfer_replica (
    id INTEGER PRIMARY KEY,
    transfer_id INTEGER NOT NULL REFERENCES transfer(id) ON DELETE CASCADE,
    coordinator_peer_id INTEGER REFERENCES cell_peer(id),
    local_role TEXT NOT NULL DEFAULT 'mirror',
    last_applied_event_id INTEGER,
    last_applied_event_hash TEXT,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    CHECK (local_role IN ('coordinator', 'participant', 'mirror'))
) STRICT;
```

For hello world, these can be simple or even represented inside one database. The important part is not network complexity; it is separating the concepts:

- Draft proposal.
- Coordinator event.
- Local mirror.
- Final settlement.

Direct Cell-to-Cell sync can later reuse the same event cursor flow by calling whichever peer is coordinator for that Transfer.

## Cache Tables

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

## Sync Function Example

```rust
pub async fn sync_transfer_topic_from_peer(
    db: &SqlitePool,
    peer: CellPeer,
    topic: String,
) -> Result<(), Error> {
    let summaries = fetch_visible_transfer_summaries(&peer.base_url, &topic).await?;

    for summary in summaries {
        if visibility_summary_is_allowed(&summary) {
            upsert_transfer_discovery_cache(db, peer.id, summary).await?;
        }
    }

    mark_subscription_synced(db, peer.id, topic).await?;
    Ok(())
}
```

This creates a path for "needs of the world" discovery while keeping each Cell in control of what it stores and republishes.

## Cells, Organs, And Servers

Lince should not require one central global server, but real Transfers need some coordination point.

Recommended model:

- Each Transfer has a coordinator.
- The coordinator can be a local Cell or an Organ server.
- The coordinator orders events and stores the operational source of truth.
- Other Cells can mirror or export/import Transfer events.

MVP:

- Local Cell for personal Transfers.
- Organ server for cross-user Transfers.
- Server as source of truth while the Transfer is active.

Later:

- Start on an Organ server for discovery, then allow direct Cell-to-Cell continuation.
- Add event signatures where the event author can be verified independently from the relay server.
- Add coordinator migration.

Cell-to-cell sync can start before full p2p federation:

- The Organ/contact list stores peer Cells.
- Each peer has a base URL and optional public key.
- A Cell can fetch visible Transfer summaries from a peer.
- A Cell can fetch the event log for a Transfer it participates in.
- The coordinator still orders writes for the active Transfer.
- Direct peers can mirror event logs and cache discovery summaries.

Do not start with full federation. The item-level agreement and visibility model is already enough complexity.
