# Remaining Fiote work

Refined with the owner's decisions on 2026-09-30 and checked against the current working tree. Sources: the [Fiote record](Tasks.lingua), `@fiote` / `r_137BM4Q7GV2707HBTRQ3GTJ9E5`, and the shared [Karma plan](Lince.md). The owner's latest refinement defines this plan; the Record remains unchanged. This was a review of implementation and test sources, without running the installed app or tests.

Karma CRUD, reading/firing explanations, and unsaved Rule Simulation have now been delivered through ordinary Actions, including `PreviewKarmaProposal`. Reuse them. MCP, genai integration, ordinary Record editing, Message persistence, attachments, dictation, assignment dispatch, and inherited-instruction inspection also already have implementation.

The numbers retain the previous discussion's references. Point 8 is an idea outside the implementation plan. Backend and native UI work below form one feature; proposed operation names describe the design, not APIs already present.

Implementation should proceed sequentially: define the shared composition contract and balloon isolation from points 1/5; connect canvas delivery and saving from points 2/5; finish proposal/report and call integration from points 1/2; then deliver activation, Transfer workflows, receipt entry, and context inspection in points 3/4/6/7. Apply point 9's accessibility controls as each UI is built, then verify the installed workflows in point 11. Point 10 stays deferred.

## 1. Temporary UI for proposals and interaction

Fiote may compose a temporary interface from existing components to present information, ask questions, review proposed changes, or help with a task. Point 5 defines its composition format; point 2 defines how it reaches the canvas.

- [ ] Show proposed Record, Karma, or Transfer changes with the relevant existing Sands/Castles. Include the targets, current values, proposed values, and source revisions. Offer edit, apply, and dismiss controls.
- [ ] Let the person save the temporary composition as a named reusable component through the existing component library. Saving the UI and applying its proposed data changes are separate operations. Closing the UI removes its presentation and leaves already committed changes in place.
- [ ] Make every generated UI interactive immediately within ordinary authorization and validation. There is no extra enable-actions step. A particular task may still present proposed changes with Apply/Dismiss controls.
- [ ] Mount every Fiote-generated UI inside the known Fiote conversation balloon component. Lince adds and owns this wrapper and its immunity automatically, without a model request, generated instructions, or additional model tokens. Fiote supplies only the content; it cannot remove or weaken the boundary.
- [ ] Block influence effects across the balloon boundary in both directions: outside areas cannot affect its contents, and inside areas cannot affect outside components. Cover forces, sorting, scaling, area membership/entry/exit, property-change effects, and sound/event triggers. Components and areas inside the same balloon may interact normally. Use component ownership, not just overlapping screen coordinates, to decide which side an instance belongs to.
- [ ] Establish that boundary before mounting content or evaluating effects, and preserve it during movement, resizing, closing, saving, and reopening. Bind generated layout/events to instances within their own balloon; ordinary data queries and authorized Record/Transfer Actions remain available.
- [ ] Use the existing `PreviewKarmaProposal` Action for unsaved Rules. Attach its final values, broken checks, cycles, coverage, and stop reason to the corresponding proposal. Changing the proposal or its source data makes the result stale.
- [ ] For Record or Transfer proposals outside that Action's supported inputs, extend the shared Simulation bridge only where needed. Simulated effects remain in copied state, with controlled external responses and authorized evidence.
- [ ] Let Fiote try another option after a selected check fails and explain the rejected alternatives using the report's evidence. Simulation stays optional; applying a proposal uses a separate ordinary Action.
- [ ] Reuse existing backend Simulation tests. Add coverage for immediate interaction, display/save/close, stale proposals, refused actions, and immunity in both directions, including overlapping areas, unlimited reach, repeated Record views, and independent balloon instances.

The previous plan's claim that Fiote lacked access to Rule proposal Simulation is superseded. The remaining work is the composed UI, linking that UI to reports/actions, and supporting other proposal types where needed.

**Boundary meaning:** Immunity isolates canvas influence and component events. An intentional authorized edit still changes the shared Record/Transfer, updates other views of it, and may trigger its existing Karma. Isolating the data as well would require copied Records or staged proposals as a separate feature.

**Existing mechanism to extend:** `External` blocks outside sources from affecting inside targets; `Internal` blocks inside sources from affecting inside targets; `All` blocks both kinds of source from affecting inside targets. `Containment` blocks outgoing forces but deliberately leaves Record transitions unblocked. None alone provides the requested two-way boundary while preserving internal interaction. Extend the shared area/effect mechanism and apply the balloon policy across its consumers; do not simply select `All` or rely on a model-authored area. The balloon presentation exists, but this mandatory host policy is remaining work.

References: [native tool guidance](../../crates/transport/src/native/catalog.rs), [Rule proposal contract](../../crates/engine/src/karma_preview.rs), [native component library](../../crates/desktop/src/custom_castle/library.rs), [existing balloon](../../crates/desktop/src/fiote.rs), [immunity modes](../../crates/desktop/src/area_effects.rs), [area transition checks](../../crates/desktop/src/area_mutation.rs), [containment tests](../../crates/desktop/src/area_effects/tests.rs).

## 2. Shared commands from the backend to the canvas

Fiote and future Karma presentation consequences should use one mechanism for putting a component on a person's canvas. This needs a shared backend/interface contract; the current workspace lives in the native interface, so sending an arbitrary database Action does not already edit it.

- [ ] Define ordinary authorized Actions for requesting component presentation and its supported updates/removal. Address the user, workspace, receiving interface session, destination area when relevant, composition, and instance explicitly.
- [ ] Have the native interface consume those requests through its ordinary component loader and workspace operations. Send operations against identified instances instead of replacing the workspace file; preserve concurrent local edits.
- [ ] Give each request an identity and report queued, applied, or refused status. Repeated delivery updates the intended instance without spawning extra copies. No available recipient means pending/unavailable presentation, not successful display.
- [ ] Use the same path for Fiote's temporary compositions and saved components. The native loader must enforce point 1's balloon host for every Fiote-generated UI, including replayed requests; neither the generated payload nor a requested destination may bypass it. Connect Karma's future show/place consequence to the shared path when that work is ready. Broader device routing and offline presentation remain in point 10.
- [ ] Let Fiote open the existing Communication/call component through this mechanism. A real call must also use the ordinary call context/start/join/end operations; displaying the component alone does not mean a call started or someone answered.
- [ ] Show the normal participant/call state and controls, and verify authorization, changed targets, cancellation, duplicate delivery, and preservation of local canvas edits.

This is shared infrastructure for points 1 and 5, with a later Karma consumer. General board manipulation is deferred in point 10.

References: [transport protocol](../../crates/transport/src/protocol.rs), [workspace](../../crates/desktop/src/workspace.rs), [call controls](../../crates/desktop/src/communication/calls.rs).

## 3. Generic Fiote activation as a Karma consequence

A Fiote's description and inherited instructions define its job. A habit reviewer, a UI suggester, and any other initiative are configurations of the same Fiote mechanism. There is no separate habit-review scheduler or special habit-agent implementation.

**Decided:** Each Rule occurrence produces one activation request carrying its nonzero value. The request wakes the selected Fiote; it does not set or reset that Record's stored quantity. Zero produces no activation.

**Decided:** While a Fiote is running, later activations combine into one pending run. Each occurrence still has its own request identity. Preserve the triggering Rules and carried values so the pending run can explain what woke it; do not silently add values with different meanings.

- [ ] Add an ordinary Fiote activation Action and a typed Karma consequence that uses it. Retain the actor, trigger identity, Fiote UID, and carried value; for Karma, also retain the Rule/revision, occurrence, and consequence position. Use that evidence for authorization and duplicate-request handling.
- [ ] Reuse Fiote's existing dispatch/runtime machinery. Start a run from the Fiote's effective instructions and supply the activation metadata as task input. A fresh automatic run should pin the descriptions current at its start; it can read earlier results through ordinary tools.
- [ ] Start promptly when idle; when busy, collect activations into the single pending run. Keep its causes inspectable, bound what is supplied to the model, and freeze that batch when the next run starts. Further activations then belong to a new pending run.
- [ ] Require normal permission to activate the selected Fiote and recheck permission/configuration before dispatch. A wake request does not grant additional read/write access or permission to change Fiote's instructions.
- [ ] Refuse new requests for a disabled Fiote. Keep an enabled but locked/unavailable Fiote's pending run inspectable as waiting work; disabling it cancels pending work. On restart, recover unstarted requests and mark interrupted runs for inspection rather than blindly repeating their effects.
- [ ] Add the consequence to the ordinary Karma editor, plus Fiote controls for its prompt, enabled state, Run now, Stop, and pending activations. Existing Frequency/Rule controls supply the schedule.
- [ ] In Simulation, record the activation intent and use a controlled response if available. A preview does not start a real provider request or edit the live canvas; missing model-response coverage is reported.
- [ ] Check Frequency-based activation, zero/nonzero values, duplicates, coalescing, prompt changes, revoked access, stop/restart, and reactions caused by Fiote's own edits.

Why use a request: a persistent quantity of `1` cannot by itself distinguish today's activation from tomorrow's. The current generic `Activate` Action only sets quantity to `1`; it does not dispatch Fiote. The chosen design keeps the Rule occurrence as the durable evidence for waking it.

References: [current activation Actions](../../crates/engine/src/actions.rs), [Karma consequences](../../crates/nucleus/src/karma/consequence.rs), [assignment dispatch](../../crates/cell/src/fiote/assignments.rs), [prompt snapshots](../../crates/cell/src/fiote/behavior.rs).

## 4. Full Transfer CRUD through ordinary Actions

The goal is complete Fiote access to the Transfer operations a person can perform, using the same backend Actions. The generic tool catalog already exposes the Action schemas; finish the workflow guidance, any missing shared operations, and acceptance coverage.

- [ ] Create Transfer drafts with their items, parties, terms, timing, dependencies, and disclosure; read the permitted Transfer projection and history; revise drafts/terms; and discover the exact schema for each operation.
- [ ] Cover invitations, counteroffers, publication, agreement, activation, delivery/receipt/dispute, settlement, loans/extensions, private accounting, dependencies/children, cancellation, and corrections through their existing Actions. Add a shared ordinary Action only where the requested operation has no such path.
- [ ] Read current terms, participant, and revision before a change. Preserve signing requirements and disclosure rules. Report accepted, refused, or awaiting remote delivery accurately.
- [ ] Present suggested Transfer edits through point 1. Use the existing Transfer Simulation paths when prediction is wanted; extend the bridge rather than creating another Transfer runtime.
- [ ] Complete the delete operation for an unused draft: add a shared draft-discard Action and native button if no equivalent exists. The recommended eligibility is the creator's unpublished/unaddressed draft with no commitments or settled effects. Validate that eligibility again at commit time.
- [ ] For negotiated/active Transfers, use the appropriate cancellation operations. For settled effects, use corrections/reversals and retain the signed history. Generic `DeleteRecord` currently refuses Transfer mutation.
- [ ] Verify creation, reading, revision, draft discard, and the main lifecycle through Fiote tools, including changed terms, wrong participants, hidden data, retries, and interruption. Reuse the existing Transfer acceptance fixtures.

Draft discard is a concrete backend/UI gap, not a promise that every Transfer can be erased. The lifecycle actions determine what “remove this Transfer” can accomplish in its current state.

References: [ordinary Actions](../../crates/engine/src/actions.rs), [Transfer Protein source](../../crates/protein/src/lib.rs), [tool catalog](../../crates/transport/src/native/catalog.rs), [Transfer plan](Transfers.md).

## 5. Compose existing Sands, Castles, events, and areas

**Agreed scope:** Fiote combines existing native components into a new reusable composition. This works through data loaded by the running application and does not require recompiling Lince for each composition.

- [ ] Publish a discoverable catalog of supported parts, editable properties, data bindings, event slots, and area behavior. Include the exact typed composition schema.
- [ ] Extend the current native composition format to represent the required existing components and named event/action bindings. The current format supports selected parts; it is not yet a universal catalog of all Sands, Castles, and events.
- [ ] Validate the entire composition with one shared contract used by backend creation and native loading. Current backend checks are structural, while the native loader performs deeper validation; Fiote-created components need the full validation before success is reported.
- [ ] Bind events to existing native behaviors and ordinary Actions with typed parameters. Keep the same access checks and area transition semantics used by hand-built UI, with point 1's mandatory boundary around the composition. Resolve local layout/event targets within that instance rather than accepting references to arbitrary outside canvas instances.
- [ ] Render the result temporarily through point 2. Provide a name field and Save component control using the existing library, plus normal workspace placement/persistence for a kept instance.
- [ ] Save the composition's layout, bindings, configuration, and Fiote origin so it can be reopened as an independent instance. Every saved or copied Fiote-generated composition receives the same Lince-owned balloon and immunity on loading; saving does not remove the boundary. Keep licenses and credits available for its reused or embedded dependencies.
- [ ] Check unsupported parts, malformed bindings, invalid area/layout references, interactive controls, refused actions, saving/reopening, and independent copies.

New Rust behavior is a separate extension question, explained below. It is not needed to deliver this agreed composition scope.

References: [native composition model](../../crates/desktop/src/custom_castle.rs), [library encoding/loading](../../crates/desktop/src/custom_castle/library.rs), [backend validation](../../crates/engine/src/custom_component.rs), [existing component tests](../../crates/desktop/tests/custom_components.rs).

## 6. Turn notes, dictation, and receipts into Facts

- [ ] Complete “I spent 42 reais on lunch”: extract amount, currency/unit, date, and meaning; search permitted Records/Concepts; rank plausible matches and explain the selected match.
- [ ] Use existing exact entry/quantity Actions and classification for the Fact. Keep the original message/receipt as evidence. Resolve a missing target or ambiguous interpretation before committing.
- [ ] Offer a new Record through the proposal UI when no suitable Record exists. Avoid duplicates caused only by differing names.
- [ ] Show the amount, target, category, source, and resulting change in an editable preview. Support correction through ordinary entry revision/void operations.
- [ ] Check typed text, dictated text, receipt images, matching ambiguity, decimals/units, refused access, corrections, and retries without duplicate expenses.

Attachments and final-block dictation already exist. The remaining work is extraction, matching, review, and commit. Verify actual receipt-image acceptance with the selected model.

References: [attachment adapter](../../crates/fiote/src/acp/content.rs), [dictation](../../crates/desktop/src/speech.rs), [entry Actions](../../crates/engine/src/actions.rs).

## 7. Show Fiote and subagent context

- [ ] Extend instruction inspection to show the pinned ancestor/Fiote instructions, assigned task or activation causes, included messages, attachment references, available tools, and retained summaries/results supplied to each session.
- [ ] Distinguish stored conversation history from the current supplied context. Lince currently loads the latest 12 messages; that window is not a summary.
- [ ] Identify visible child sessions by parent, task, thread, and working/waiting/stopped state. Define a child-session reporting/linking contract; prompt ancestry alone is not an execution tree.
- [ ] Show only the external context/usage a connected agent exposes, and label unavailable internal context accurately.
- [ ] Verify session/child isolation, instruction revisions, restart, bounded inspection, and exclusion of credentials and inaccessible content.

Existing prompt inspection and reported usage are the foundation. Pet animations and a new compaction operation are outside this implementation plan.

References: [prompt sources](../../crates/cell/src/fiote/behavior.rs), [history loading](../../crates/cell/src/fiote.rs), [status types](../../crates/fiote/src/config.rs), [inspection controls](../../crates/desktop/src/fiote/session/management.rs).

## 9. Concrete accessibility features

The first delivery is a conversation-driven route to existing operations, with keyboard controls and screen-reader output.

- [ ] Make the message composer and Send, Stop, Attach, Dictate, Stop/transcribe, and Cancel dictation controls reachable in a predictable Tab/Shift+Tab order, with visible focus. Enter/Space activates a focused button; text inputs retain their normal multiline behavior.
- [ ] Make Fiote questions, generated balloon controls, and proposal fields operable from the keyboard. Use real labels for targets, current/proposed values, required fields, and errors. Provide Save component, Apply, and Cancel/Dismiss without requiring a canvas drag.
- [ ] Expose replies, questions, errors, run status, and proposal values in the platform accessibility tree. Announce a finished reply, new actionable question, or error once; avoid announcing every streamed token or stealing focus.
- [ ] Use the person's screen reader for spoken read-back. Dictation continues to insert editable, unsent text; sending remains a separate control. A separate text-to-speech provider or live voice mode is not part of this delivery.
- [ ] Keep these controls usable in a normal Fiote thread even when its generated UI is spatial or unavailable. Render the same candidate fields and invoke the same normal Actions.
- [ ] Verify keyboard-only typed/dictated requests, question answering, proposal correction/application, a refusal, and cancellation with a real screen reader on the supported native platform.

References: [native text/button controls](../../crates/desktop/src/description.rs), [message rendering](../../crates/desktop/src/thread_castle/message_view.rs), [speech controls](../../crates/desktop/src/speech.rs).

## 10. Board controls for later, after shared Karma work

Temporary composition display uses point 2, and its mandatory immunity belongs to point 1's first delivery. Broader board editing waits for the shared presentation/area/Karma contract and further owner refinement.

- [ ] Define Fiote's access to existing canvas structure and requests to move, resize, hide/show, or remove an instance. Decide what happens with pins, sorting, physics, and influence areas.
- [ ] Define which events may be configured in a composition and which can be Karma consequences. Preserve the distinction between layout edits and transitions that change Record state.
- [ ] Refine multiple-device/session routing, offline requests, stale placements, and keeping or expiring automatically shown UI. Do not silently choose whichever workspace happens to be focused when delivery occurs.
- [ ] Connect saved-component placement and scheduled presentation to Karma's eventual consequence implementation. Reuse the shared command path and verify ordering, repeats, access changes, and reconnect.

These are later tasks with decisions still open. They do not expand the current plan into arbitrary board automation.

References: [deferred Karma presentation](Lince.md), [workspace](../../crates/desktop/src/workspace.rs), [area transition tools](../../crates/transport/src/native/catalog.rs).

## 11. Installation and acceptance

- [ ] Verify the installed executable/wrapper and the actual user-service environment. Exercise login, readiness checks, Stop, and resume outside the development shell.
- [ ] Exercise immediately interactive UI, immunity in both directions, saving/reopening with the same protection, proposal reports, generic activation/coalescing, Transfer CRUD, receipt entry, context inspection, and keyboard/screen-reader interaction.
- [ ] Verify real microphone/device selection and a real receipt-image request with configured providers. Check selected audio/file paths where supported; readiness checks remain free of generation.
- [ ] Keep shared messages/questions/attachments/dictation usable without Fiote where applicable.
- [ ] During implementation, run focused correctness/security tests and relevant `cargo check` targets with warnings denied. Check bounded composition/context size, pending activations, and delivery work where those paths change.

References: [Cell workflow tests](../../crates/cell/src/fiote/tests.rs), [native tool tests](../../crates/transport/tests/native.rs), [packaging](../../flake.nix).

## Settled decisions and later refinement

Activation requests carry each Rule occurrence's nonzero value, and busy requests combine into one pending run. Generated UI is immediately interactive inside a mandatory, automatically supplied balloon with immunity in both directions. These decisions are recorded in points 1 and 3.

Board routing, placement, and lifetime decisions remain deferred in point 10. A general runtime plugin host is an optional idea below; it is not required for saving compositions of existing components.

## Ideas outside the implementation plan

**Point 8 — pet compaction and animations:** Keep petting Fiote to compact context, sleeping/waking, and working/idle animations as an idea only. This plan adds no compaction operation, pet implementation, or associated acceptance work.

**Generating genuinely new Rust behavior:** Fiote can generate source, but the current installed Lince cannot execute an arbitrary new Rust Sand from that source. There are three different delivery paths:

| Path | What is compiled? | Rebuild Lince for each new component? |
| --- | --- | --- |
| Composition of existing parts | Existing parts were compiled with Lince; a new composition is data | No |
| New built-in Rust Sand/Castle | New source and the application containing it | Yes |
| Future runtime plugin host | Add the host to Lince once; compile each new plugin separately | No, while the plugin fits the supported host interface |

For a future plugin route, my recommendation is Rust compiled to WebAssembly, exchanging typed UI/events and ordinary Action requests with a native host. The host must provide the rendering and application APIs; a Wasm module does not automatically become a Bevy Sand. A compiler is still needed locally or elsewhere for each generated module. Rust supports [compilation to WebAssembly](https://doc.rust-lang.org/rustc/platform-support/wasm32-unknown-unknown.html).

Native dynamic libraries are another possible route, but direct exchange of Rust/Bevy types is a poor default for generated extensions because Rust's [native ABI has no stability guarantee](https://doc.rust-lang.org/reference/items/external-blocks.html#abi). A deliberately designed foreign-function boundary could support native plugins, with its own loading and failure model.

Lince already uses [Wasmi for its embedded terminal engine](../../crates/desktop/src/terminal/vt.rs). That is a specific integration, not a general Sand plugin host. A general host, its typed API, compiler distribution, resource limits, packaging, licenses, and supported platforms would be a separate feature to design if compositions prove insufficient. No runtime-plugin implementation is authorized by this plan.
