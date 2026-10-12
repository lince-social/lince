# Fiote native harness

Fiote now owns its Rust conversation and tool loop. Native ChatGPT subscription, API key, and compatible local connections require no installed agent, Codex, Node, or ACP launcher. Optional ACP, external provider adapters, and scoped MCP remain available. Native connections use existing Lince Actions and authority; their working directory starts empty.

## Completed acceptance

The approved native harness implementation and acceptance checks are complete. A fresh Nix release package passed compilation, its package check hook, installation, and wrapping. The package check hook ran zero binary tests; the targeted tests below provide the behavioral coverage. Stylo requires Python during compilation, so Python is included in the UI package's native build inputs. The release wrapper contains rendering libraries and Bash for Command Castle, with no required agent executable; Python is a build dependency.

The packaged desktop ran outside the development shell with PATH=/nonexistent, an isolated database, and a private virtual display. Actual UI clicks saved a native compatible local connection without a key, checked its model catalog, connected it, selected fixture-local, and completed Test response with Release native hello. A normal Message sent through the Fiote conversation also received the streamed reply; the completed Message and replay history were verified on disk. The test endpoint recorded model discovery, non-streamed inference, and streamed conversation inference. Screenshots are target/fiote-release-response.png and target/fiote-release-conversation-response.png; logs are target/fiote-release-ui.log and target/fiote-release-endpoint.log.

The virtual display needed software Vulkan from the same pinned Mesa package as the release build. Its display has no hardware DRI3, and the host software driver requires a newer glibc than the package. Using the matching driver solved the test setup without a product rendering change. A separate canvas issue was observed when the management Castle and an opened Record Castle occupied the same position: one intercepted clicks intended for the other. The normal conversation acceptance passed with a standalone Record Castle in an empty workspace. That generic canvas interaction issue remains outside this harness implementation.

## Implemented context

Provider-specific protocols live in crates/fiote/src/communication. auth.rs implements OpenAI's documented native OSS registration with stable host IDs, dynamic loopback ports, PKCE/state/nonce, verified OIDC identity, issued client IDs, isolated accounts, protected credentials, rotating refresh, and logout revocation. chatgpt.rs implements streamed Responses, account model discovery, namespaced functions, reasoning replay, eligible attachments, terminal-event validation, cancellation, bounded retries, and readable failures. It retains finalized output items when the terminal event omits them, tolerates missing SSE content-type headers, and validates completed JSON replies. Tools remain unavailable until successful completion. native.rs returns the shared Provider interface. Genai remains the Rust API/local transport; modelbridge and its bundled provider subprocess are removed. Broken optional adapters produce diagnostics without hiding native providers.

runtime.rs owns the common tool loop and counts tool descriptions in the context budget. conversation.rs automatically summarizes older complete exchanges using the same model and no tools, keeping instructions and six recent user turns. Token windows use a conservative byte estimate capped at 512 KiB; unknown windows use that ceiling. Actual model tokenization can still reject input. Full original Messages remain saved.

Cell's fiote/history.rs persists per-reply tool journals and summary fingerprints. Reopening retains complete tool receipts; edited/deleted source Messages invalidate derived history. Interrupted effects receive an explicit unknown outcome and are never automatically repeated. Credentials use the existing encrypted vault, account-specific slots, and process-shared refresh locks. If another process changes the vault, unlock it again rather than refreshing stale credentials.

Desktop connection controls offer native subscription/key/local setup, saved account selection, Check, Test response, model selection, advertised reasoning/fast controls, reauthorization, and logout. Subscription, API, and local profiles share the normalized model catalog and selection controls. Scripted UI tests activate the actual controls through Cell, select a discovered model, and receive a completed response from a local HTTP fixture.

## Validation

99 distinct targeted tests passed: 42 Fiote protocol/runtime tests, 45 Cell Fiote tests, 10 desktop Fiote tests, and two optional ACP workflows using Lince's own Rust protocol fixture. Ten native tests also passed with release optimization and outside Nix with no programs on PATH. cargo check passed for Fiote, Cell, desktop, and Lince, including all Fiote and Cell targets, with warnings denied. Redacted logs are in target/fiote-harness-tests.log, target/fiote-harness-final-check.log, target/fiote-harness-release.log, and target/fiote-harness-acp.log. The installed third-party ACP/model test remains intentionally unused for native acceptance.

Live acceptance passed on 2026-10-07 through the existing authenticated Firefox session and normal OpenAI consent interface. Native OIDC sign-in and model discovery succeeded. The account's offered gpt-6-astra model, with low reasoning and fast disabled, returned exactly Hello from native Fiote (35 input / 10 output tokens). A second test ran outside Nix with PATH=/nonexistent: the real model discovered create-record, invoked the normal Lince Action, and produced exactly the expected stored Record in an isolated in-memory database before confirming success. Temporary sessions were remotely revoked. No CLI credentials, installed harness, or API billing fallback were used. This account does not advertise gpt-6.1-sol or fast mode.

The isolated real-account probe is crates/fiote/examples/native_acceptance.rs. Run through nix develop .#interface --command cargo run -p lince-fiote --example native_acceptance -- --model MODEL, or choose --default-model explicitly. Authorize in the normal browser. It lists account models, requests a small completed response, records response/usage only, and revokes its temporary token. It never imports CLI credentials or switches to API-key billing.

The real Action probe is crates/cell/examples/native_action_acceptance.rs. Run through the same Nix shell with cargo run -p lince-cell --example native_action_acceptance -- --model MODEL or --default-model. It uses the production tool registry and runtime, permits only discovery and creation of one specified fixture Record, verifies the database result, and revokes its temporary session. Both compiled probes can run directly outside the development shell.

## Limits and reuse

Subscriptions need a provider-supported native contract. ChatGPT's documented OSS route is implemented; unrelated subscriptions do not become interchangeable through OAuth alone. Claude subscriptions prohibit third-party native harness use under current provider rules; API keys or an optional user-owned agent remain routes. ChatGPT plan preview supports eligible text/images/files, with no audio/video/transcription or hosted tools. Local connections need a running model endpoint; Fiote does not bundle model weights. Native Fiote registers file tools and a shell command tool when a working directory is configured; those commands depend on programs installed on the host.

Genai, OpenID Connect, and SSE crates supply the existing protocol primitives. The small Retry-After algorithm was adapted from MIT jcode commit a61c38ee9dd945fe9f930fa09c7f3454a2ae4110. Its license and attribution, plus OIDC/SSE dependency terms, are included in the Fiote Sand. eventsource-stream declares MIT OR Apache-2.0 without a standalone license file; the selected MIT terms and package author are included. No pi_agent_rust code was copied because of its restrictive license rider.

Sources: [OpenAI registration](https://developers.openai.com/siwc/token-sharing-open-source/sign-in), [models/inference](https://developers.openai.com/siwc/token-sharing-open-source/models-and-inference), [accounts](https://developers.openai.com/siwc/token-sharing-open-source/profiles-and-sessions), [preview limits](https://developers.openai.com/siwc/token-sharing-open-source/preview-limitations), [Claude credential rules](https://code.claude.com/docs/en/legal-and-compliance#authentication-and-credential-use).

## Native canvas authoring

The implementation now extends the existing native canvas broker with authoring operations. Areas, registered Sands and Castles expose their rendered part tree, used tokens, layout, instance edits, typed event routes, transform groups and registered Action behaviors. People and Fiote use the same revisioned operations. A person can ask “build me a component for this workflow” without configuring a repository, creating a design study, or launching another Lince. The architecture recommendations below still distinguish authored state from renderer-managed internals and later workflow tooling.

### Using the implemented capability

Fiote discovers inspect-canvases and the canvas Action through lince_describe. Registry lists actual native factories and their state schemas. Inspect returns placements; InspectElement returns their paginated part tree, measured geometry, layout rules, registered settings and token provenance. Read every next_offset when a tree spans several pages. The visible Record Castle and its source area form one authoring instance; its title, sections, property blocks and inputs have stable semantic names. Its source controls remain inspectable separately within that instance.

Mutate/Patch applies an atomic list of SetPart, ResetPart, InsertPart, ConfigurePart, RemovePart, SetLink, RemoveLink, SetBehavior, RemoveBehavior, SetGroup and RemoveGroup changes. Enum spellings on the wire use kebab-case. SetPart replaces that part's override object; retain the existing fields that should survive. Layout values use explicit units. New parts use user/ identities, a registered component or component=null for a flow group. Existing controls retain their entities, bindings, drafts and focus during styling and reparenting. Pending Record or file drafts prevent destructive factory replacement or deletion.

Canvas placement operations include Add, Configure, Move, Resize, Duplicate, Rebind, Group, Remove and guarded Undo. Duplicate keeps external Record bindings and creates a new placement identity. Rebind changes declared Record bindings without rewriting literal UI text or Action arguments. Native state that embeds a binding in its own configuration must be configured together with its declared bindings. Instance IDs, part IDs, Record UIDs, transform-group IDs and library Record IDs remain distinct.

Export returns a canonical lince.canvas_component document, its SHA-256 hash and the inspected revision. It preserves component size while leaving its destination position free. The source=authored layer contains the base native factory plus authored parts, layout, token overrides and uses, links, groups and behaviors. Recovery defaults allow reset to restore inheritance after workspace reload. Save the returned document through create-custom-component. This is a native recipe; it is also a concrete artifact a personal Fiote can read before changing Rust defaults later.

In edit mode, select an item, open General, and choose Edit native component. The native inspector provides part selection and pagination, row/column and wrapping controls, token setting/inheritance, dimensions, flow-group insertion, an advanced typed patch editor, duplicate, guarded undo and library save. Record data remains editable through the ordinary Record controls and tools.

The registry now includes complete authored influence-area settings and native state adapters for documents, IDE, explorer, recorder, media, drawing, assertions, simulation, shader, layout containers and Kanban. Kanban recipes include their source/column regions, layout and spawn-target links; copies remap region identities. Imported resources reuse an existing local asset package and report a missing package rather than substituting content. Renderer-managed scene internals, arbitrary Rust closures and unavailable native kinds are capability limits, not editable ECS data.

Setting or removing a token override operates on its instance/part. Color and metric token uses can be rebound. Inspection reports the effective value, inherited source and source part when applicable. Relation color rules retain their ownership. Canvas geometry, native layout rules and Node flow constraints use separate editing paths, so competing dimensions are rejected with their controlling source.

Event behaviors have stable attachment IDs, part/event targets, registered Action parameters and an enabled flag. Native buttons emit activate. Event routes have explicit source/target part and event names, and their dispatch is bounded even with feedback routes. Configuration does not invoke an Action. Influence areas remain inactive unless activate=true, including their query/spawn producer; Record changes retain the existing native preview/arming boundary. Generated host isolation and local-session authority remain protected.

Renderer-owned controls can be removed from presentation and restored while their bounded native state stays with their owner. Authored children are actually deleted with their owned subtree. Removing a placement disposes its owned generated controls/regions and retains linked Records and saved definitions. A deletion with incoming references needs an explicit cascade. Global theme defaults and compiled factory behavior are separate destinations from instance overrides.

A personal Fiote workflow can use these capabilities to prototype on the canvas, edit the Lince repository when its working directory is configured, and test the result later. Starting a second Lince, choosing normal mode or Laboratory, comparing screenshots, and orchestrating source promotion are deferred workflows. They are not prerequisites for native CRUD. No HTML, browser prototype, external coding agent, or web-to-Rust translation belongs in this path.

### Native authoring validation

67 targeted regression tests passed: five nucleus canvas-authoring tests, six engine canvas tests, 11 native transport tests, 32 desktop canvas tests, six Fiote workspace tests and seven Cell working-directory tests. The optional external ACP working-directory test was ignored; the native harness tests do not require it. These suites were run through cargo xtask in the interface Nix environment with warnings denied.

The final cargo xtask check passed for nucleus, engine, transport, lince-fiote, lince-cell, lince-desktop and lince with all targets and warnings denied. git diff --check passed. No database migration or schema change was needed.

The native tests exercise real Record property-block reparenting, preserved editor entities/drafts/focus, human inspector controls, authored Action activation and disabling, bounded feedback routes, invalid-batch rollback, token reset after reload, nested saved components, layout ownership, Kanban region identity remapping and disposal, library save/recreation, guarded undo, stale revisions, durable receipts and host/session authority. The transport fixture discovers and invokes canvas inspection, patch and export alongside actual file creation, editing and reading in one native Fiote tool registry. Working-directory tests verify inheritance, thread updates and remote-session restrictions. Fiote workspace tests cover file limits, escape rejection and command cancellation.

This validation uses native fixture interactions and disposable files/databases. It does not claim a new live-model conversation, screenshot comparison, or independent Lince launch. Those remain the deferred personal workflow.

### What currently works, and what does not

Native Fiote already has the conversation and Action loop needed to orchestrate this. It can query Records, read collaborative snapshots, inspect earlier Messages, change text and properties, and execute discovered Actions. Assigned task descriptions are supplied to its instructions; conversation parent Record references are supplied for further reading. Current requirements must still be read through tools: a conversation summary is not an authoritative copy of the Record.

The native tool catalog exposes inspect-canvases, canvas Registry/Inspect, present-component, and create-custom-component. The canvas contract supports adding and configuring placements, moving, resizing, removing, workspace changes, receipt inspection, and undo. It has stable placement identities, expected revisions, retry receipts, bounded schemas, and a broker to the connected Bevy canvas. Typed compositions contain named parts, geometry, registered native components, Record bindings, and ordinary Action event bindings. Generated content receives a Fiote conversation balloon and isolation boundaries, and can be saved to the existing component library.

The builtin Record state retains its Record reference, mode, and optional call configuration. Its authored layer now describes edits to real rendered controls. Patch reconciles those edits in place; Configure can still recreate a factory when its native state changes. Draft protection remains in that destructive path. Popups, data-generated controls and compiled observers retain their native ownership rules; full arbitrary ECS serialization is deliberately outside this contract.

With a configured effective working directory, native Fiote registers create_file, read_file, write_file, edit_file, list_directory, and run_command. It can therefore edit and check the Lince repository from its thread today without an external coding agent. The directory is not a sandbox for shell commands: run_command inherits Lince's operating-system authority. File editing capability does not provide a verified conversion from a canvas arrangement to a native default. Automatic conversion, independent-process comparison and activation of changed Rust defaults remain deferred.

Relevant implementation: crates/cell/src/fiote.rs and fiote/behavior.rs; crates/fiote/src/tools.rs and tools/; crates/transport/src/native.rs and native/catalog.rs; crates/nucleus/src/canvas.rs and component.rs; crates/engine/src/canvas.rs; crates/desktop/src/canvas_host/ and component_push/composition.rs. These are reuse points, not a reason to introduce a second agent harness or canvas API.

### Architecture contract and follow-up recommendations

The following sections retain the architecture targets and failure cases for future work. The implemented operations and limits are described above. In particular, default promotion, automatic structural diffs, reusable binding-slot parameterization and per-definition update policies are recommendations rather than new services delivered by this change. Current reuse keeps explicit bindings and supports an explicit Rebind; receipts report application and persistence through the existing canvas state.

### One composition contract, several destinations

Extend the existing typed composition contract with a semantic internal tree for native items. Give each editable part a stable identity such as title, planning.start, planning.due, or work.actions. Describe its registered factory, child slots/order, layout constraints, token uses and overrides, disclosure rules, typed connections, and behavior attachments. Capture authored constraints separately from resolved rectangles. Keep runtime Entity IDs, closures, credentials, and Record contents out of reusable definitions.

Distinguish reusable definition identity/revision, instance identity/revision, part identity, Record binding, canvas/workspace identity, group identity, and behavior attachment identity. Display labels and child indexes are not identities. Separate the reusable definition, instance overrides/bindings, and transient runtime state. An instance edit must not change every instance or library default. Existing instances retain a known definition revision until an explicit update policy applies.

Registered descriptors declare property types/ranges, allowed children/cardinality, required parts, supported tokens, connection slots, behavior kinds, operation support, state-transfer rules, and effect classifications. Unsupported internals must be reported with a concrete reason and added to the shared contract; an editable outer rectangle does not make the item fully editable. Arbitrary Rust closures are not serializable behaviors. Generated and transient parts must expose their owner and editing policy instead of disappearing from inspection.

A Record reference belongs to an instance binding. A reusable default needs a parameter such as current-record, not the UID of the task used to design it. Bindings must distinguish Record data links, component structure, event routing, and influence behavior. Moving a control changes structure; changing its action target or an assertion can change data and authority. Do not treat all of these as interchangeable links.

Use the existing native factories and layout engines to render this contract. The live instance, saved library component, and checked-in default must consume the same definition. Current builtin and canvas composition formats should share the new semantic model through explicit adapters; adding a third independent format would increase drift. Existing free canvas geometry remains useful for arranging alternatives, but internal property layout needs flow, wrapping, minimum sizes, alignment, and intrinsic sizing.

### Task threads and native authoring

Keep lightweight durable provenance to the originating thread, with created identities, composition revisions, receipts, and saved library references. Later turns should recover their targets without relying on model summaries. A dedicated design-session subsystem, named alternatives, or coding-job interface is optional; ordinary component authoring must work without them.

At the start, Fiote reads the task, relevant thread Messages, selected instance, current registry, and any affected Record bindings. It either edits the instance the user identified or creates an isolated working instance from the current default. Put alternatives in an available canvas area or a named design workspace without moving unrelated work. Show the originating conversation and which revision each alternative represents.

Human controls and Fiote use the same discovery, inspect, CRUD, export, undo/reset, and library operations. A patch contains the target identity, expected revision, stable request ID, and an atomic set of supported changes. Return the actual applied diff and receipt. Extend the current canvas broker to apply patches on the Bevy thread at an appropriate schedule boundary; never let an async model tool mutate World directly. Keep results bounded and inspect subtrees on demand rather than sending the whole ECS world to the model.

Layout edits should retain existing editors and subscriptions, including unsaved text, caret, focus, selection, scroll, and pending saves. Reconcile parts by stable identity. Type changes and removal of an active editor require an explicit state disposition; do not quietly destroy it. Patch conflicts should return the current revision and affected differences so Fiote can reread. Undo must also check intervening edits rather than overwriting the person's later work.

Keep coupled controls together by default: a date input and its calendar button form one property; a calendar popup has an owner and event boundary. Reparenting must preserve event targets, keyboard behavior, accessibility order, popup placement, and subscription lifetimes. Changes unsupported by the registry should return a capability gap. Fiote can later implement the missing primitive in Rust when asked, rather than pretending an arbitrary closure is serializable.

### Native inspector and base feedback

The canvas needs a general native inspector showing the selected part tree, tokens and provenance, typed links, groups, behaviors, constraints, measured sizes, and revision. Include creation, removal, reordering, scope selection, undo/reset, and library save. People should be able to make the same changes directly. Selection context must identify “those buttons” unambiguously and remain pinned during a patch. Instance editing, reusable component saving, and global defaults have distinct destinations.

Expose canonical typed/textual export and structural differences as base feedback, including authored layout, token references/overrides, links, behavior parameters, and binding slots. Separate derived/effective values from authored values. Hashes identify exact exported revisions but do not prove interaction correctness. Native geometry can be inspected in the current canvas. Screenshot capture and automated Laboratory interaction tools for Fiote are optional later capabilities, not base requirements.

Native correctness tests still need to check typing, disclosure, wrapping, calendar ownership, focus restoration, saving, and reloading. A design isolated from canvas influence can still execute ordinary Actions against shared Records; use fixtures for tests that alter data. Closing a balloon or undoing layout does not undo data effects.

Measure after layout has settled, recording viewport, theme, font assets, display scale, Castle width, content fixture, and disclosure state. Compare save/reload and fresh construction under identical conditions, accounting explicitly for renderer rounding. Preserve authored numeric precision and responsive constraints. A screenshot at one width cannot prove responsiveness or guarantee identical physical pixels on another device.

### Optional personal Fiote workflow: repository edits and comparison

“Save this component” and “make this the default in Lince code” have different destinations. The latter should pin an exact reviewed composition revision and content hash, its binding parameters, and measurement fixtures. Further canvas edits create a new revision; they must not silently alter the revision being promoted.

Prefer a checked-in typed definition consumed by the normal Rust factory. Changing that source-controlled definition is a codebase change and preserves the same recipe reviewed on the canvas. Keep new behavior implementations in Rust and reference them through the registry. If a factory is still hardcoded, first make it consume the shared contract rather than asking Fiote to recreate the geometry from a screenshot. General arbitrary ECS-to-Rust generation is not the recommended architecture.

Fiote can use its existing repository tools to prepare the change when its effective workdir is the Lince repository. Verify that these tools are actually offered to a normal native thread. Read AGENTS.md and respect other in-progress changes. Detect changes to target files, avoid destructive resets, and do not automatically commit, publish, or replace the running application merely because a layout was approved. A dedicated promotion service, new coding harness, or development-job UI is not required for canvas CRUD.

Personal instructions may later coordinate cargo check with warnings denied, relevant native tests, and starting another Lince through cargo xtask or Nix. Hard comparison evidence includes canonical authored composition, typed properties, relationship/behavior graphs, token provenance, bindings, and geometry. Soft evidence includes screenshots under matching viewport, font, theme, scale, content and disclosure. Exclude expected differences such as new instance identities and transient state explicitly. Neither document equality nor screenshots alone establish complete correctness. Choice of normal mode or Laboratory depends on the test and remains deferred.

Rust source edits do not hot-swap compiled behavior into the running binary. A workflow must report source changes separately from activation and restart. Deferring independent-process testing does not defer correctness tests for CRUD itself.

### CRUD semantics and deletion policies

Creation, duplication, property updates, part insertion/reordering/reparenting, token overrides, typed connections, group operations, behavior attachments, and removal/restoration use explicit shared operations. The implemented wire schemas are discoverable through the canvas Action. Atomic batches let callers move a control and reconnect it without exposing a half-valid intermediate state.

Deleting a placement preserves its Record and reusable definition. Deleting a part identifies owned children, observers, subscriptions, popups, attachments, and incoming links. Reject dangling required references by default; explicit detach, replacement or cascade policies state their scope. Required/protected parts return the violated invariant when removal is invalid. Removing an override restores inheritance; disabling a behavior and removing its attachment are different operations.

Copies receive new instance identities and remap internal references. External Record references remain bindings unless the caller requests a different policy. Saving a reusable component parameterizes instance bindings, retains authored token uses and overrides, and inventories required factories, assets and referenced definitions. Missing dependencies must be reported instead of silently substituting widgets. Credentials and hidden Record contents never enter the template.

Distinguish selection/transform groups, layout containers, ownership/composition, and influence containment. Define exclusivity and removal policy for each group kind. Reparenting and grouping must state local versus canvas coordinates and whether world positions are preserved. Disallow ownership cycles; validate behavior/link cycles according to their domain instead of indiscriminately forbidding feedback relationships.

Connections distinguish Record bindings/assertions, event routes, layout ownership, group membership, and influence relationships. Validate endpoint types, existence, required cardinality and readable scope. Layout editing does not authorize changing linked Records: existing Record Actions remain the path for data effects.

### Token provenance and layout ownership

Reuse native token definitions, inheritance and scoped overrides. Inspection reports token identity/type/range, actual uses, local authored value or binding, inherited source, effective value, and behavior-derived overrides. Setting/removing an override or changing a supported token binding must be first-class operations. Do not freeze every inherited value into an override on export. For portable components, preserving inheritance versus freezing selected values is an explicit save policy.

Editing defaults to the targeted instance/part. Kind defaults and global theme edits use separate scopes. Reparenting can change inheritance: return the affected effective changes and permit preserving selected values through explicit overrides. Relation color rules and other dynamic sources must identify their controller so Fiote does not repeatedly write a value that immediately loses.

Canvas geometry, token dimensions, layout engines, group transforms and influences can control the same position or size. Declare one authored owner per property and derive the remaining representations. A mutation of a derived property returns its controlling source and supported edit options instead of allowing systems to fight every frame. Use existing native layout/token/influence systems; no parallel evaluator. Preserve numeric precision within native types, responsive constraints and intrinsic sizing, and report clamping or ineffective values explicitly.

### Behavior attachments and effect boundaries

Expose registered behavior definitions and attachments with stable identity, enabled state, typed parameters, targets, activation conditions, event boundaries and owning scope. Areas need their real configuration: filters/query or spawn sources, sorting/layout, strength/direction, immunity and property-change behavior where supported. An outer rectangle and generic strength field are insufficient coverage.

Configuring a behavior is distinct from activating it. Creating/editing a composition must not automatically invoke bound commands or replay one-shot events. Report effects that can resume on enabling a live behavior and preserve existing area disarm/review semantics. A preview remains observational. Reusing a component with live effects requires clear binding and activation policy; isolation from canvas forces does not isolate ordinary Actions against shared data.

Cleanup and reconciliation must preserve event targets, keyboard behavior, accessibility order and popup ownership. New compiled behavior is Rust work through repository tools. CRUD composes and configures what the native registry implements rather than accepting arbitrary executable strings as native behaviors.

### Transaction, concurrency and persistence failures

Validate the whole proposed graph and resolve dependencies before applying it on the Bevy thread. Bound graph depth, part counts, patch size and work per frame. Stage changes so a failure can restore the authored composition without discarding editor state. Atomicity covers composition/reconciliation, not arbitrary data changes, file operations or effects already executed by a behavior.

Use target revisions and dependency revisions where needed. The existing canvas-wide revision works initially; scoped revisions can later avoid conflicts from unrelated canvas edits. Reuse stable request IDs and receipts including target/operation identity. After interruption or timeout, inspect the receipt and actual state before retrying. Distinguish stale revision, invalid graph, unavailable capability, missing target, runtime failure, persistence failure and unknown outcome.

A receipt distinguishes applied in memory from persisted on disk. Serialize saves in revision order so a delayed old save cannot overwrite a newer edit, and define crash recovery authority. Durable receipts/revisions must reconcile interrupted work honestly. Undo applies guarded semantic inverse patches and must not overwrite later changes by another Fiote or the person. Durable identities survive workspace reload without relying on Entity IDs.

Existing Fiote balloon and event-isolation protections remain, while updating a user-selected existing instance preserves its identity and context. Do not force every edit through replacing the target with a newly generated balloon. Creation can use the existing generated presentation path; editing uses the identified instance. Inspection, export, mutations and referenced data obey the same session authority as native controls.

### Criticism and recommendations

The main architectural risk is having a flexible preview and a separate hardcoded production Castle. One contract and factory are the remedy; exact reproduction should be demonstrated by reload and fresh construction, not promised by the model. Another risk is exposing arbitrary ECS mutation as design tooling. A bounded semantic registry makes composition inspectable while retaining native behavior invariants.

Rebuilding the Castle after every request would repeatedly lose focus and drafts. The implemented Patch reconciliation path retains native editors; Configure remains a guarded factory replacement when native state changes. Conversely, attempting to serialize every ECS component would entangle presentation, transient state, and application authority. Serialize the supported authored composition, with explicit instance state transfer where needed.

Fiote already has code tools, so a new coding agent or general shell subsystem is unnecessary. A task authorizing canvas refinement does not by itself authorize executing repository commands or promoting a global default. Use the person's stated destination and existing authorization; do not ask again when they already explicitly requested codebase promotion. Make development capabilities and their host authority visible in Lince.

Prove the shared contract through vertical slices: Record Castle, an editable Sand, and an influence area exercise editors, tokens, groups, and effects. Then extend factory coverage. The acceptance target remains full native authored-item coverage; these slices are sequencing, not a permanent limit. Avoid inventing a universal visual scripting language before existing components are faithfully editable.

### Sequential implementation and acceptance

1. Inventory factories, semantic parts, tokens, links, groups and behaviors; publish a capability matrix. Extend shared schemas and ownership rules without parallel layout or influence engines.
2. Implement inspection/export and revisioned atomic CRUD through the existing broker for the initial vertical slices. Native controls and Fiote must use the same operations. Prove editor state preservation and removal cleanup.
3. Complete token, relationship, group and behavior CRUD with provenance, reference remapping, deletion policies, activation boundaries and layout ownership. Invalid combinations must fail before effects.
4. Integrate ordered persistence, durable receipts, guarded undo and library reuse. Verify rebinding, save/load fidelity, interruption recovery and stale edit rejection.
5. Extend coverage to remaining factories and generated parts with explicit owner-aware policies. An item is complete only when its semantic composition can be faithfully inspected, edited, saved and recreated; genuinely runtime-only parts are reported as such.
6. Verify native Fiote file tools with disposable fixture files and a configured workdir. A normal thread must discover tools and produce actual file changes without an external coding agent. The dedicated file tools lack delete_file; deletion is currently available through run_command. A typed deletion tool may improve usability later.

Base acceptance includes asking Fiote to build a workflow component, refining its internal parts/tokens/links/groups/behaviors, saving and reusing it with new bindings, and removing/restoring it without deleting linked Records. Tests cover invalid input, cycles, unsupported capabilities, atomic failures, concurrency, state preservation, subscription/popup cleanup, ordered persistence, authority and bounded large compositions. Running a second Lince and promoting a prototype into defaults remain separate, deferred workflow acceptance.

Native authoring is implemented separately from personal Fiote workflow instructions. Starting another Lince, screenshot comparison and automatic source promotion remain deferred. Repository file and command tools already belong to the native Fiote harness when a working directory is configured.
