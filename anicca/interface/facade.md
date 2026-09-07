# Public Live Facade

Purpose: Specify publication of a deliberately narrow, read-only projection of one Box composition.

Owner source: [Interface in Lince](../Lince.lingua); no separate Interoperability
Record currently exists.

Status: Late-v1 product work; earlier browser parity is historical evidence,
not proof of publication or export from the new Bevy interface.

Read when: implementing publication, public Protein streaming, or visitor-local state.

[Corpus map](README.md) · [Current context](current.md) · [Interface plan](plans/interface.md)

---

### Bevy authoring and external publication

Publication controls are ordinary Bevy Sands. A visitor's browser is a real
external boundary, so an exporter may translate an explicitly supported subset
of authored Bevy composition into safe public content. This does not require
a parallel HTML implementation of every native Sand, or a renderer-neutral
layer in the native interface. Unsupported components must be explained before
publication; arbitrary Bevy plugins cannot automatically run in a browser.

### Public Facade

This later v1 feature never depended on an embedded browser. The exported projection runs in the visitor's browser, not inside the publisher's desktop; a native publication surface validates and publishes without an embedded preview. [The build rule](build.md#no-embedded-browser) keeps those paths separate. An in-desktop preview would need a way to render the projection without embedding a browser in Lince, and a Facade does not become a general browser client for a Cell.

A **Live Facade** is a published, read-only rendering of one Box composition at
a public URL. Caddy and DNS may terminate and route the public origin, but
Lince still owns the publication manifest, public assets, read-only data
contract, and safe browser runtime. Publishing freezes the available Sand
definitions, layout, areas, connections, and configuration until the owner
publishes a new revision. A visitor receives no edit mode, Sand store, add or
remove operation, Action bridge, terminal, filesystem authority, identity
credential, or private Lince endpoint.

The composition can remain alive without becoming writable. It subscribes to
predeclared, read-only Protein projections and updates them in real time. A
visitor cannot submit an arbitrary Protein query: publication names the saved
Protein items, allowed fields, limits, and stable result keys, and the public
service exposes only those projections. The Action route is absent, not merely
hidden. The stream has revision/resume information, bounded messages,
backpressure, reconnect behavior, and honest stale/offline/removed states.

Interaction that changes only the visitor's browser remains available:
opening a Record selected from a Kanban-like composition, changing the current
Instinct page or chapter, expanding sections, filtering an already-delivered
projection, panning, zooming, and running read-only force or sorting areas.
This state begins in memory. A Facade may opt into namespaced browser storage
for preferences such as the last page, with a visible reset and a small quota;
it never sends that state back as an Action. Mutation areas, write ports, and
Behaviors requiring durable authority make publication validation fail rather
than quietly becoming inert.

A ready-made Kanban Facade is therefore the same saved compound Sand a person
could open and decompose in Box: Protein area, repeated card group, field
arrows, grouping/sorting areas, and Record detail composition. It remains as a
convenient Sand-store entry. Export retains the supported composition's data
meaning, layout and style roles; it may translate their Bevy representation at
this boundary. It is not a second native Kanban or a promise that every native
component exports automatically. Build the public artifact from an allowlist
without private resources, mutation routes or executable native plugins;
hiding authoring controls is not an authority boundary.

The first Live Facade admits official/read-only compound Sands and inert,
sanitized assets. It does not admit Website Sands, arbitrary remote resources,
installed network-capable Sands, or raw advanced CSS that can impersonate
Lince chrome or escape its bounds. It runs on an origin separated from private
Lince administration, with a restrictive CSP whose only connection is the
scoped public Protein stream, no ambient cookies or Lince credentials, bounded
storage, and sanitization of rendered Record content.

Public data must be projected into a separate public Organ before serving it.
That Organ contains only the published subset; a bug or compromise must not
turn a field filter into access to the private Cell. Publication shows the
selected Records, fields, definitions, assets, and estimated size before the
owner confirms it, and revocation closes the stream and removes future
availability without pretending already downloaded public data can be erased.

This Live Facade and the existing content-addressed archive Facade are two
delivery modes. The archive has no live stream and can be fetched privately by
hash. A direct public URL and WebSocket necessarily reveal network metadata
such as visitor IP and timing to Caddy or whichever service answers it; Lince
can avoid accounts, cookies, analytics, and application-level viewer ids, but
cannot truthfully promise that a directly contacted server learns nothing
about the request. Use the archive/relay path when that stronger privacy
property matters.
