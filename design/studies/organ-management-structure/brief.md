# Organ management structure study

Focus: how can all existing capabilities remain reachable while the opening view shows only my Organ and a clear way to my contacts?

The human chose “See my Organ and reach contacts” as the opening-page priority. Design decision: the most minimal useful default view, with occasional work behind named destinations. No feature removals are proposed. Selection of an alternative remains pending.

Baseline: the existing Rust Castle in `crates/desktop/src/organ_castle.rs:229`, its `ui.rs`, QR controls, social submodules, and nested Configuration, owner backup and sync panels. See `baseline.md` and `source-inventory.json` for traceability. Duplicate contact controls are consolidated; capabilities are preserved. This is a source audit, not evidence that every backend operation currently succeeds.

Fixed: Rust-generated theme tokens, bundled Lato, fixture data, control treatment, page contents and scenario names. Only navigation/grouping changes. The HTML uses native controls to study structure; production geometry and Rust rendering need separate validation.

Audience: a person managing their own Organ and connections. Frequent tasks: understand own identity, open a contact, share/add a contact. Occasional tasks: device membership, recovery, policy, delivery, public discovery and operating services.

Contexts: desktop 960×720, mobile 390×844, constrained Sand 376×540. Each alternative uses the same focused subpages. Device and contact lists open contextual detail. Uncommon controls are grouped by purpose instead of one generic Advanced bucket.

Mock boundaries: no real network, QR decoding, file selection, backup, clipboard, camera, keys or publication. Scan buttons simulate a decoded synthetic value and never add a contact. QR appearance is a labeled illustrative placeholder. Forms simulate pending/success/error and permission states; native authorization and cryptography remain authoritative. Hosted workspace and sync tools are capability-preserving navigation/field sketches, not full canvas interaction prototypes.

Acceptance: walk every manifest scenario; compare the opening view and path length to each task using equal theme/viewport/state. A source-grounded capability table is not proof of native parity. Explore further interaction detail only after selecting a structure.
