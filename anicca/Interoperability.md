Interoperability (@interoperability: 0, is #chapter, #instinct, #part-of @interface, #done) { r_7NJYRJYKVFGVG3DT27WRQFRCWF

## Interoperability

Lince interoperates through explicit boundaries rather than pretending every
foreign system is native Ledger truth. Blood owns system-to-system exchange;
external Sands own embedded interaction; Facades own public presentation;
portable canvas formats are future import/export adapters, not Lince's schema.

### Playground and Organ Facades

A workspace or Sand can already be exported as a self-contained `file.html`
archive of one state. That static mode makes no requests and carries no access
tokens. It remains the safest portable artifact and can be opened without a
running Lince host.

An Organ Facade is the public T2 surface described by
[the Organ Profile](Ontology.md#the-organ-profile-in-three-tiers). It may
use one of two visibly distinct delivery policies:

- **static** freezes public data into the archive and has a strict no-network,
  no-token guarantee;
- **dynamic public** is a stream of signed, content-addressed generations of
  deliberately published, read-only, bounded projections. The publisher pushes
  generations to distributable caches/relays under a chosen refresh, size,
  retention, and withdrawal policy. The publisher may generate each version by
  running its declared bounded Protein locally; a viewer fetches the result
  from a cache and never sends an arbitrary Protein query to the publishing
  Cell. It is not a live replica and gives the visitor no general Protein or
  Action authority.

Both modes render in a bounded surface that cannot imitate Lince system chrome
and show their byte size, generation time, and expected freshness before
fetch. Genuine inspection must not tell the publisher who looked or when, and
must not make the publisher's Cell absorb viewer traffic. Facade retrieval
therefore carries no cookie, access token, stable viewer identifier, referrer,
or executable tracking request; privacy-preserving transport prevents the
publisher from observing the viewer's network address. Caches enforce fixed
artifact and request budgets and can serve immutable generations without
contacting the origin. The renderer schema-checks and sanitizes each response
and cannot substitute a query to reach unpublished data.

Withdrawal stops discovery and future cache service after the declared cache
window; it cannot erase a public generation that someone already downloaded.
The publication UI states that consequence before publishing.

If someone needs current direct access rather than an honestly timestamped
public generation, they use Lince live/replica sharing with explicit identity,
authorization, and encryption. Facade privacy is not weakened to imitate that
mode.

### Blood and portable canvas representations

[Blood](Ontology.md#9-federation-and-blood-talking-to-other-systems) remains
the integration path for foreign services and data models. A Blood adapter
maps provenance and semantics explicitly; an embedded Web page does not become
a Blood integration merely by loading in a Website Sand.

The [OCIF specification](https://github.com/ocwg/ocif-spec) and
[OCIF library](https://github.com/ocwg/ocif-lib) are prior art and examples
only. They can help illustrate stable canvas-node identity and graph-shaped
documents, but Lince has no task to evaluate, integrate, import, export, or
remain compatible with OCIF. It never limits Box and creates no future
completion condition.

### Future public commerce experiments

Payment integration could eventually let a Facade conduct a bounded buy/sell
flow. One proposed Lince Institute shop creates and assigns production Needs
just in time when an order arrives, initially for T-shirts, stickers, 3D
keychain accessories, and hoodies. This is preserved as future exploration,
not part of the current Interface completion contract.

## What is left

### Interoperability

- [ ] Reconcile Ontology's current T2 no-network wording with the two explicit
  modes above before implementing dynamic Facades.
- [ ] Define the public projection manifest, query ceilings, caching,
  publisher-push protocol, content signatures, generation/freshness policy,
  withdrawal behavior, safe renderer set, and abuse limits. Cache misses and a
  malicious flood must not be forwarded into a publishing Cell.
- [ ] Define privacy-preserving retrieval and a no-tracking cache policy. Test
  that the publisher cannot correlate a viewer or view time, that Facade code
  cannot add tracking requests, and that the delivery layer exposes no stable
  application identity.
- [ ] Add export and preview surfaces that state whether a Facade is static or
  dynamic and prove that neither can escape its bounded chrome.
} r_7NJYRJYKVFGVG3DT27WRQFRCWF
