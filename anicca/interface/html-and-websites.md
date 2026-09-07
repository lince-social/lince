# Installed HTML and Website Sands

Purpose: Define external package identity, capabilities, isolation, storage, networking, updates, and Website authority.

Owner source: [Interface in Lince](../Lince.lingua); no separate Sands or
Interoperability Record currently exists.

Status: Historical joined authority/composition evidence retained. Embedded HTML, Website and their product surfaces are unavailable: the embedded browser they were designed against is gone, and each needs a way to run without embedding a browser in Lince before it returns. See [the build rule](build.md#no-embedded-browser). The contract below records what was designed, not what ships.

Read when: changing the bridge ABI, external packages, Website permissions, or runtime admission.

[Corpus map](README.md) · [Current context](current.md) · [Sand plan](plans/sands.md)

---

### Bevy target and historical boundary

New native Sands use Bevy directly. The old external package/bridge design
below does not require a native projection ABI, renderer-neutral tree or
matching HTML for every Bevy component. An actual external integration still
needs validation, isolation and explicit authority. A native Bevy plugin is
trusted code, not a sandbox for an installed package. Current extensions are
registered editable components/effects and trusted Rust plugins. Untrusted
executable installation and sandbox design are explicitly deferred; the old
requirements below must not become a hidden prerequisite. Specialized content
replacements are low-priority end-of-v1 work; arbitrary HTML execution is not
provided by Bevy or authorized by this decision.

### External HTML and Sand packages

This contract described an optional embedded adapter that no longer exists; [the build rule](build.md#no-embedded-browser) removed it, and installed HTML needs an execution story that does not run HTML inside Lince before any of this applies again. With no such capability, importing or restoring browser-backed content may inspect its metadata and preserve a non-executing unavailable reference, but never start a hidden browser. Static HTML export and a Facade viewed in an external browser never needed an embedded runtime at all.

External HTML is a first-class Sand source, but visual integration and trust
are separate concerns. Imported content can look and behave like it has always
belonged to Lince while still crossing an explicit capability boundary. There
are four entry forms:

- a local HTML file becomes a content-hash-pinned Sand definition;
- a Sand package carries HTML, assets, Behavior metadata, its capability
  manifest, lineage, version, licenses, and credits;
- a `.lince` package carries a workspace or group of referenced Sand
  definitions and placements;
- a remote URL becomes a Website Sand rather than silently copying or
  modifying the remote page.

#### Website Sand

A Website Sand is a deliberately untrusted browser surface. It is fully
interactable and can be moved, resized, grouped, connected at its wrapper, and
influenced like another Sand. Box owns those wrapper capabilities. The remote
page receives no Lince identity, credential, Protein, Action, lane, host state,
native IPC, or ambient bridge and cannot inspect its parent composition. Lince
cannot inspect or restyle the cross-origin page or read its private state.

Website mode permits ordinary user-directed HTTPS navigation and the remote
services, scripts, forms, and subresources that the site itself needs. This is
possible with the same fundamental risk as opening a browser tab: the site may
track, deceive, fingerprint, waste resources, or contain a browser/WebView
exploit. Lince can protect its own authority and local data; it cannot certify
arbitrary Internet code as harmless. A persistent, non-spoofable origin label
and security menu therefore remain visible even when ordinary Sand chrome is
borderless.

Adding a URL does not execute it immediately. The person first sees the origin
and chooses to enable Website mode after an honest explanation that the site
can contact the Internet and store site data like a browser tab. This grant is
to run as a Website, never a grant of Lince authority, and can be revoked by
unloading the page and clearing or retaining its isolated site data as chosen.

Static HTML and Lince integration are different modes, not permission toggles
inside a live Website:

- **Static HTML** is pinned/imported content with no network, remote storage,
  or ambient script authority. It is the safest mode for archives and Facades.
- **Website** runs the remote origin like a normal Web page but has zero Lince
  authority. It exposes only host-owned wrapper facts such as URL, title,
  loading state, focus, and bounds.
- **Installed external Sand** is content-hash-pinned code with a reviewed,
  versioned manifest. This is how a third party combines external network
  access with Protein, Actions, or typed Sand ports. An arbitrary live Website
  never upgrades itself into an integrated Sand.

An installed external Sand may therefore declare an input such as
`record: RecordSummary`, receive it from a visible Protein field mapping, and
emit `record-clicked: RecordRef` into Box. Another Sand or Castle may consume
that event, and an explicitly connected route may request a typed Action. Host
IPC or `postMessage` is only transport: the host validates the port, value,
instance, rate, size, capability, actor, and Action request before delivery.
The event name does not grant access to the Record table, and the external Sand
cannot subscribe to a Protein or invoke an Action that its definition and Box
connections did not expose.

Under Plan A, Website was a separate embedded browser surface and request
context whose accelerated texture was composited by the native runtime. That
surface is gone with the embedded browser; a Website Sand now needs a way to
show a remote site without embedding a browser in Lince. In that former
implementation it remained mounted and executing outside the camera even when
Lince did not draw its texture. Because the page was a browser surface rather than a child
iframe, framing headers do not apply in the same way; sites may still reject
embedded browsers, protected media, authentication, automation, or unsupported
Chromium builds, and Lince offers an explicit open-in-browser fallback.

In the browser client and Plan B — modelled, and NOT PLANNED as a way to reach
a Cell's own interface (see
[architecture.md](architecture.md#reaching-this-cells-own-interface-through-a-browser-not-planned))
— Website uses a
[sandboxed cross-origin iframe](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/iframe).
A site may refuse embedding with
[`frame-ancestors`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/frame-ancestors)
or `X-Frame-Options`; Lince respects that
decision and offers to open it externally rather than proxying the page or
stripping its protection. Services such as YouTube work only through their
supported embed URLs. Plan B is the browser projection; it does not introduce a
second Linux WebView desktop.

Website storage is useful and permitted, but is not Lince host state. Cookies,
local storage, IndexedDB, Cache Storage, and service-worker data live in a
separate Website profile, partitioned by origin and isolated from Lince and the
person's normal browser profile. Same-origin Website Sands may share their site
session; a private Website instance uses an ephemeral profile. Configuration
shows per-origin use and quota and can clear one origin or the complete Website
profile. Browser iframe privacy rules may still make an embedded login behave
differently from a top-level tab, and Lince reports that incompatibility rather
than weakening storage isolation silently.

Normal Website navigation is HTTPS-only by default. A Website must never reach
Lince native capabilities, custom protocols, `file:` URLs, or
authenticated Lince host endpoints. Every local HTTP and WebSocket endpoint
also rejects a
foreign `Origin` and requires an unguessable, scoped credential for privileged
requests; CORS and the browser's
[same-origin policy](https://developer.mozilla.org/en-US/docs/Web/Security/Defenses/Same-origin_policy)
are not treated as CSRF protection.

Where an embedded engine or dedicated WebView exposes reliable request interception, Website mode
also blocks loopback, link-local, and private-network destinations and
rechecks redirects and DNS resolution against rebinding. A normal browser
iframe does not give its parent complete control over the destinations its
cross-origin child requests. The browser client therefore states the same
local-network risk as an ordinary browser tab instead of claiming containment
it cannot enforce; its hard boundary is that no such request carries Lince
authority and no Lince endpoint accepts it. Downloads, popups,
external-protocol links, clipboard, camera, microphone, geolocation,
notifications, screen capture, and unpartitioned storage begin denied and
require a visible per-origin decision where the platform can enforce it. A
download is host-mediated and never gives the page a filesystem path.

Trust is provenance-based:

- official Sands are trusted and automatically receive only the capabilities
  declared in their signed first-party manifest, without repeated prompts;
- locally authored Sands run in developer mode and make their requested
  capabilities visible;
- installed third-party packages are pinned, reviewed by manifest, and
  granted capabilities explicitly;
- raw local content is isolated, receives no Lince credentials, and has no
  Protein, Action, filesystem, terminal, clipboard, media, or network
  capability by default. A Website Sand has only its separate browser profile
  and host-owned wrapper described above.

Capability checks are enforced by the host and engine, not by hiding UI or
trusting iframe JavaScript. Network access is itself a declared capability,
scoped by destination and method for installed Sands. A Website's ordinary Web
traffic is not a Lince capability and never carries Lince data. Every installed
Sand bridge message is schema-checked, size-bounded, and attributed to the Sand
definition and instance that caused it. Unknown API versions and unknown verbs
fail closed; there is no compatibility path for obsolete Sand APIs.

The landed package gate also checks that an Installed projection has exactly
one declared HTML document, external CSS and module Behavior, declared semantic
nodes, and a closed graph of local relative static JavaScript imports. Remote,
absolute, parent-traversing, dynamic, missing, non-module and projection-
undeclared imports fail admission. The joined Gallery's JavaScript decoder
refused an inbound mount with an unknown field and then accepted the valid
version-1 Protein-shaped mount, so this boundary was exercised in the embedded
browser as well as Rust tests.

The landed C2 fixture also proves recursive Installed composition without
giving JavaScript semantic authority. Rust supplies the normalized
`video-call-room` graph and typed bindings; the Installed adapter clones its
declared template for two instances, derives eight unique DOM ids from
instance and local node identity, and repairs `for`, `aria-labelledby`,
`aria-describedby`, `aria-controls` and `aria-owns` references. Its declared
module exports distinct mount and teardown functions, removes the delegated
listener, clears the scoped roots and remounts visibly. The same nested Button
event crosses the exact installed grant and the same Action remains
host-validated. Website receives none of this bridge or composition-internal
authority.

Official and user-owned definitions update live because their identity and
lineage are local and controlled. Installed external executable packages do
not: an update produces a new content hash, shows the capability and license
diff, and requires deliberate adoption. Any vendored embedded library ships
its required LICENSE, NOTICE, and credits inside the Sand package.

The [native laboratory](laboratory.md) records what the current joined runtime
already does. Statements there are evidence, not a promise to retain legacy
component or frame shapes.
