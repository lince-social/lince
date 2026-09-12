# Websites and external content

The owner wants a Website Sand with isolated storage and permissions, and asks whether it can work without CEF. The current direction removes the embedded browser. These two points need a workable design; the markdowns must not claim that a Bevy surface already solves it.

Until a renderer and execution model are chosen, a restored Website or installed HTML reference can show its identity and why it is unavailable. Inspecting a package or exporting static HTML does not require executing its contents. A system-browser opening option is a possible decision for the owner, not a replacement silently chosen here.

## What the website wish means

The surrounding Sand belongs to Box and can be moved, resized and grouped. The remote page has no Lince credentials, Protein access, Actions, filesystem access or visibility into its parent composition. Keep its origin visible and explain when a site cannot be embedded.

Adding a URL first shows its origin and asks the person to enable the website; it does not execute immediately. Extra capabilities start denied and are reviewed per origin. Revoking Website mode unloads it and offers to keep or clear isolated site data. Native siblings of an unavailable child can remain usable without giving that child authority.

Site data would belong to an isolated profile, separated by origin from Lince and the person's ordinary browser. Configuration would show usage and offer clear/reset and private storage. Clipboard, downloads, popups, camera, microphone, location and other capabilities need visible controls that the chosen implementation can actually enforce. A website grant must never become a Lince data grant.

A design must state its actual network limits, including local-network requests, redirects and access to host endpoints. Respect sites that refuse embedding. Do not claim stronger isolation than the chosen renderer provides.

The retained design starts with HTTPS navigation and denies file URLs and custom protocols. Privileged local HTTP and WebSocket endpoints reject foreign origins and require scoped credentials; browser origin rules alone do not prevent forged requests. Test profile separation, spoofed messages, navigation, denied capabilities and resource exhaustion against the actual chosen implementation.

## Packages and export

A local file, an installed executable Sand, a workspace archive and a live URL have different meanings. Package identity, declared capabilities, assets, licenses and credits can be validated before execution. Installed executable updates need deliberate adoption with a visible change in code and requested permissions.

If untrusted executable Sands are reopened later, validate every message and its source, size, operation and grant. Keep local package imports within their declared assets and retain the last valid state after failure. Trusted native Rust plugins are not a sandbox for downloaded code.

Inspect, review permissions, adopt updates, revoke, disable and delete are distinct controls. Identify imported content from its validated shape, not just a filename suffix. Refuse unknown versions or operations, spoofed instances and undeclared imports. An installed Sand's declared ports and grants must not be reachable by an arbitrary Website or through a more privileged neighbor.

Static archives and public Facades run in the recipient's browser. [Facade](facade.md) explains their public-data boundary. The proposed website and content tasks are gathered in [Specialized content](plans/interface.md#specialized-content).
