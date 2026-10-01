# Fiote — implementation and remaining work

Updated for the owner's implementation authorization and refinements on 2026-09-30, and the real-Codex, normal-message attachment and application-launch requests on 2026-10-01. The source was the Fiote Record, `@fiote` / `r_137BM4Q7GV2707HBTRQ3GTJ9E5`, formerly in `Tasks.lingua`. Another chat removed that file during this work; this implementation does not edit or relocate owner-authored Records. This Markdown records implementation details, verification and remaining acceptance. Owner decisions in this conversation supersede older agent plans.

## 1. Execution ledger

The previously authorized implementation is present in the shared working tree. There are 82 passing focused tests; native and media-enabled integration compile checks passed with warnings denied. Four MCP socket cases were blocked by this sandbox; three external-agent cases require a fixture or installed provider. The new all-conversation attachment audit, any missing implementation and the required application-launch investigation are planned below, not implemented or tested by the 2026-10-01 documentation update. Installed acceptance remains a separate, ordered checklist in section 3; implementation and compilation do not establish provider or device acceptance.

| Order | Feature | Implementation present | Verification |
| --- | --- | --- | --- |
| 1 | Reusable influence-area isolation | `Isolation` blocks both crossing directions and preserves same-side interaction; ordinary area controls expose it; sound uses the shared influence guard | Focused area and sound tests passed |
| 2 | Shared native composition | Typed parts, layout, buttons and named events; shared size/depth/count validation; backend validates Action payloads and nested Record access | Shared contract and backend presentation tests passed |
| 3 | Presentation, balloon and saving | Shared `PresentComponent`; automatic existing Fiote balloon, inner canvas/event scope, Save/Close, library and placement restoration | Backend and native bridge/save/reopen tests passed |
| 4 | Proposals and Communication | Reuse normal native Record/Karma/Transfer views, revision/preview Actions and the existing Communication controls | Pending workflow acceptance; no automatic model-response Simulation adapter |
| 5 | Generic activation | Normal Action, Karma consequence/editor, durable requests, busy coalescing, waiting/cancel/recovery and native controls | Engine and Cell coalescing/waiting/restart tests passed |
| 6 | Transfer CRUD | Existing Actions plus creator-only unused-draft discard and native control; Fiote tool guidance and CRUD acceptance test | Signed backend and native CRUD tool tests passed |
| 7 | General conversation and Facts | Normal conversational tools, exact entry Actions, request identities and source-Message attribution; conversation does not force Fact creation | Conversation, discussion, retry and evidence tests passed; real receipt acceptance remains |
| 8 | Context inspection | Pinned descriptions plus supplied-input snapshots, tool calls/results, attachment references, task/activation metadata and reported child links | Cell context, attachment and child-inspection tests passed; external-agent acceptance remains |
| 9 | Accessibility | Labelled fields/buttons, native keyboard controls, live questions/status/errors and one completion announcement rather than streamed tokens | Native labels/status/masking/completion tests passed; screen-reader/microphone acceptance remains |
| 10 | Normal-message attachments in every conversation | Existing shared attachment controls/storage and Fiote content conversion; audit and complete every composer/send path when implementation resumes | Pending ordinary-conversation and real Fiote tests for audio, video, PDF, CSV, text and photo |
| 11 | Runnable installed application | Previous launch failed before UI; reproduce when the owner has arranged a sole-agent run, diagnose and fix any application/configuration defect simply | Required before live UI acceptance; not permanently waived as a concurrent-agent problem |

Other human chats own the broader Karma/command/ontology changes. Reuse their ordinary Actions and presentation infrastructure. Do not replace their work, edit `.lingua`, edit README/AGENTS, create worktrees or spawn coding agents.

## 2. Implementation contracts

### 2.1 Canvas influence and event isolation

`Isolation` is an ordinary influence-area behavior. For a shield, it blocks a source/target pair exactly when one center is inside and the other is outside. Both inside and both outside remain allowed. It differs from `All`, which also blocks interactions within the area. `Containment` retains its existing meaning and Record-transition exception.

The shared guard applies to attraction/repulsion, sorting, scaling, Record transitions and sound membership. A blocked sound target is excluded from membership sampling; applying immunity must not fabricate an exit sound.

Every typed composition receives the existing Fiote balloon automatically. Lince constructs an inner canvas and a boundary for all named component events before mounting its content. Canvas ownership keeps outside areas detached even if geometry overlaps, a source has unlimited reach, or two balloons show the same Record. Each balloon has its own inner canvas/event scope. Its host is excluded from ordinary geometric shield sampling: two outside-owned components still interact normally when their geometry overlaps the balloon. Internal components may interact normally; nested compositions establish their own scope.

The wrapper, immunity and event scope are Lince-owned features. They are not generated instructions and require no model request or additional model tokens. They are restored on workspace load and component-library reopening/copying. A typed saved composition cannot be rewritten into the legacy unprotected component format through collaborative text changes.

**Data meaning:** an intentional authorized Action still updates the real Record/Transfer and its other views, and may trigger existing Karma. This boundary isolates canvas influence and component events. It does not create a private copy of the database.

Implementation: [area behavior](../../crates/desktop/src/area_effects.rs), [sound](../../crates/desktop/src/sound_area.rs), [balloon host](../../crates/desktop/src/component_push/composition.rs), [event scopes](../../crates/desktop/src/scoped_events.rs).

### 2.2 Composing and saving existing components

The shared document uses format `lince.custom_component` and a `composition` with `name` and `parts`. Each part has a local `id`, integer center `position`, integer `size`, a typed `component`, and optional `events`. The first supported catalog is:

| Kind | Configuration and behavior |
| --- | --- |
| `record` | Stable Record binding; `full`, `description` or `call` view |
| `text` | Editable native text |
| `karma` / `frequency` | Existing editors, optional search |
| `transfer` | Existing Transfer Castle, optional search |
| `calendar` | Existing native Calendar and its normal date interaction |
| `area` | Ordinary influence area with configured immunity and strength; wider in-balloon area editing remains outside this catalog |
| `button` | Label plus exact ordinary Action JSON |
| `composition` | A nested typed composition |

A button Action uses the normal `action` discriminator. A named event binding contains `event` and an ordinary `action` object. Bindings belong to the part's instance and balloon scope; they do not address outside canvas entities. Event actions currently use explicit parameters, while existing native event consumers retain their ordinary behavior. This catalog is not a claim that every native Sand or every editable runtime property has a generated-data representation.

Limits: 256 KiB per shared document, 256 total parts, 8 composition levels, 32 named event bindings per part, bounded names/coordinates/sizes. Fiote tool arguments have their existing smaller 128 KiB limit. Backend creation/presentation and native loading validate the shared contract; the backend also parses every button/event Action and checks nested Record bindings through ordinary access rules. Slugs are resolved to stable bindings before saving.

`PresentComponent` uses the existing local backend-to-interface channel and stable target/component slots. Identical pushes reuse an instance; changed configuration may replace it. A missing native receiver is an explicit error. A receiver count means delivery was queued, not that a person saw the UI. Delivery is local to the connected interface.

Fiote-origin presentation of a single supported component is wrapped automatically too. Compositions are immediately interactive through ordinary authorized Actions. Save component persists their name/layout/bindings through the existing library; Close removes the presentation while committed data changes remain. Workspace placement and library copies reconstruct protected, independent instances. Native text/search edits, layout and the supported area immunity/strength are captured when saving. A saved generated composition retains its originating Fiote/conversation; Open originating conversation uses the ordinary thread UI. Wider native area properties are outside this typed catalog.

Reused native components retain their existing licenses/credits. No embedded dependency or arbitrary executable is added by a composition.

Implementation: [shared schema](../../crates/nucleus/src/component.rs), [document contract](../../crates/nucleus/src/component/composition.rs), [backend presentation](../../crates/engine/src/component_presentation.rs), [backend saving](../../crates/engine/src/custom_component.rs), [native library](../../crates/desktop/src/custom_castle/library.rs).

### 2.3 Proposals, Simulation and calls

Fiote can display a report with existing typed views/text and provide edit/apply/dismiss buttons invoking normal Actions. Saving the UI and applying its data changes are separate operations. Use existing revision checks for Karma/Transfer edits and `PreviewAreaTransition` / `ApplyAreaTransition` for guarded Record changes; a proposal is not authority to bypass a stale-data refusal.

`PreviewKarmaProposal` already supports unsaved Rule proposals. Its final values, failures, cycles, coverage and stop reason are evidence for comparing options. A changed proposal/source requires a fresh preview. Existing Transfer Simulation is reused where supported. An unmodelled outward effect is reported as missing coverage; a preview never starts a real Fiote provider or presents live UI.

A `record` component in `call` mode opens the normal Communication controls. The person uses the normal context/start/join/end path and media/device choices. Presenting the panel does not start or answer a call, or enable microphone/camera/screen capture. Real call and device acceptance remains necessary in an installed media-enabled application.

References: [Rule proposal bridge](../../crates/engine/src/karma_preview.rs), [Transfer Simulation](../../crates/simulation/tests/transfers.rs), [Communication](../../crates/desktop/src/communication/calls.rs).

### 2.4 Generic Fiote activation

A Fiote's description/inherited instructions define its job. Habit review and UI suggestions are configurations of the same activation mechanism.

- `ActivateFiote` carries a stable `request_id`, target and exact nonzero `value`. Zero creates no request. It never changes or resets the Fiote Record's quantity.
- Karma authors use `@fiote: activate-fiote`; the editor offers this consequence. Each Rule occurrence/consequence position has its own durable request identity and carried value.
- Requests preserve actor and cause. Karma causes include Rule/revision and occurrence evidence. Duplicate identities return the original receipt; conflicting reuse is refused.
- The existing Cell dispatcher starts a fresh thread with current pinned instructions. If any thread for that Fiote is working, later requests remain one pending batch. When it starts, that batch is frozen; later requests form the next pending batch.
- Values retain their individual meanings. Up to 32 sampled causes plus total count are supplied to the model; all request rows remain durable. Do not silently sum values. There is a 4096-request pending limit per Fiote and the existing eight-running-thread limit.
- Permissions, Fiote configuration and Karma Rule revision/paused state are rechecked before dispatch. A wake request grants no extra authority.
- Disabled/unavailable runtimes refuse new requests. Locked/missing provider credentials keep accepted requests visible as waiting. Disabling cancels pending requests. Stop current and pending runs cancels waiting work and interrupts active work; future Rule occurrences may wake an enabled Fiote again.
- Unstarted requests survive restart; formerly running work is marked interrupted for inspection rather than replayed. Uncertain completed effects are not automatically repeated.
- Run now, Refresh activations, Stop current and pending runs, activation values/causes/state and Open activation thread are native management controls. Frequency/Rule editors supply scheduling.

Simulation reports activation as an outward intent with missing model-response coverage. A preview does not run the provider. Scheduling uses the existing Frequency/Rule mechanism.

Implementation: [activation Action/storage](../../crates/engine/src/fiote_activation.rs), [Cell dispatch](../../crates/cell/src/fiote/activations.rs), [native controls](../../crates/desktop/src/fiote/session/management.rs).

### 2.5 Transfer CRUD through normal Actions

Fiote's native tool catalog discovers the same Action schemas used by the interface. Existing Actions cover draft creation/revision, parties/terms/timing/dependencies, invitations, counteroffers, publication, agreement, activation, delivery/receipt/dispute, settlement, loans/extensions, private accounting, children, cancellation and corrections. Read the permitted projection and current terms/participant/revision before a change. Preserve signatures and disclosure. Queued remote delivery is awaiting acceptance.

`DiscardTransferDraft` completes removal of an unused creator draft. It requires normal Transfer update authority, the creator's signing identity, expected revision and a stable request identity. Eligibility is checked again in the write transaction: hidden, unaddressed, no agreement/occurrence/delivery commitments, no parent/source lineage, no committed promises or other Transfers depending on this draft or its Promises. Dependencies entirely inside the unused draft do not prevent its removal. A draft's creation already adds quantity one to its backing Record; eligibility therefore uses real commitments rather than a zero-quantity assumption. It records signed evidence and tombstones the Record while retaining history; duplicate requests return the original receipt. Native Transfer details offer the corresponding review/apply control.

Negotiated/active Transfers use cancellation; settled effects use correction/compensation/reversal. Generic Record deletion remains inappropriate for those lifecycle operations.

Implementation: [discard](../../crates/engine/src/transfer_discard.rs), [native control](../../crates/desktop/src/transfer_castle/detail.rs), [tool guidance](../../crates/transport/src/native/catalog.rs). The separate [Transfer plan](Transfers.md) owns broader Transfer work.

### 2.6 General conversation, Facts and evidence

The normal conversation can discuss, ask questions, search Records, compose UI, or perform instructed Actions. Fact creation is requested/configured behavior, not a mandatory extraction pipeline for every message.

For “record 42 reais for lunch”, discover the exact entry schema, search permitted targets and units, clarify ambiguity, then use `CaptureEntry` with exact amount and stable request identity. Use classification and normal revision/void operations for corrections. Ask or present an editable proposal when a target/meaning is missing; do not guess a target or create a duplicate Record because its name differs.

Fiote-origin Facts retain agent/thread attribution and the source user Message UID. The Message preserves its text and attachment references. Backend entry request identities and tool retry receipts prevent an uncertain retry becoming a second expense. General discussion need not create any entry Fact. Attachments and dictation reuse existing conversation features; dictation remains editable and unsent until the person sends it.

Receipt images use the configured provider's existing attachment capability. A model's extraction/matching quality, real currency/unit interpretation and image acceptance require provider acceptance; the deterministic test verifies the native conversation/Action/evidence/retry route.

References: [conversation runtime/tests](../../crates/cell/src/fiote/tests.rs), [origin attribution](../../crates/engine/src/operation_origin.rs), [native tools](../../crates/transport/src/native.rs), [dictation](../../crates/desktop/src/speech.rs).

### 2.7 Context and reported child sessions

Instruction inspection retains pinned Fiote/ancestor description revisions and indicates unapplied prompt changes. The snapshot records the input supplied by Lince: messages/attachment metadata, tool names, task and activation metadata, thread/Fiote and snapshot time. Direct sessions update it before each model request, including accumulated tool calls/results and the source user Message UID. Attachment bytes are omitted, so inspecting a large receipt does not duplicate it or consume the snapshot budget. Inspection is bounded to 1 MiB and excludes stored credentials.

The normal direct history window is 12 messages, not a summary. Fresh external sessions receive Lince's starting history; later external sessions may retain their own history. Inspection distinguishes the latest direct request from an external starting-input snapshot. External agents retain and expose their own context according to their protocol; Lince does not claim access to unreported internal memory or tool results.

`ReportFioteChild` links distinct existing conversation threads with task and reported state. It does not launch a child agent. Reporting through native tools is scoped to the connection's current parent thread; the backend validates thread kinds, access, states and bounded acyclic ancestry. Inspection exposes up to 128 visible links, indicates whether a context snapshot exists, and prefers a known Lince running state over a reported state. Prompt ancestry and child execution links remain separate concepts.

Implementation: [snapshots/children](../../crates/cell/src/fiote/context.rs), [inspection UI](../../crates/desktop/src/fiote/session/management.rs).

### 2.8 Accessibility

Reuse native focus/Tab navigation and ordinary buttons. Message, component-name, proposal/question and Fiote settings fields have explicit accessible labels. Secret fields use the password role and masked accessible values. Save/Close, Send/Stop, question answer/decline/cancel and existing attachment/dictation controls remain keyboard controls.

Questions and status/errors are live accessible output. A writing reply stays silent while streaming; completion/interruption announces its final text once without moving focus. The person's screen reader handles spoken output. No separate TTS provider or live voice mode is added.

Implementation: [accessible controls](../../crates/desktop/src/accessibility.rs), [message completion](../../crates/desktop/src/thread_castle/message_view.rs), [questions](../../crates/desktop/src/fiote/session/questions.rs).

### 2.9 Normal-message attachments — shared feature, pending completion audit

Files belong to the normal Message alongside its text. The same attach/remove/send/read/save behavior must work in every Lince conversation, including ordinary Record threads, conversations between people/private conversations and Fiote threads. Sending a file must not require a Fiote-specific command or a separate message type exposed to the person.

Existing code provides **Attach files**, **Paste image**, attachment draft cards, **Preview / play**, **Save attachment**, stored Message content and provider/ACP conversion. However, the inspected thread composer mounts `message_content::draft` only in the non-social branch; private/social conversation coverage must be checked and completed. Presence of shared code is not acceptance of every conversation path or every model format.

When implementation resumes, inspect all native composers, ordinary send Actions, backend storage/read access, receiving views and Fiote history/provider conversion. Reuse the shared implementation and add only missing frontend/backend pieces. A normal text-plus-file message must preserve its text, filename, MIME type and exact bytes; sent attachments remain accessible after reopening/restart and to permitted conversation participants. Removed files must not be sent. Failed, busy or refused sends preserve the draft. A local-file reference alone does not prove the original attachment was delivered.

Initial acceptance uses small files within the existing limits: 4 MiB total Message content and at most 16 content parts. Large-file transport is not part of this test. Preserve ordinary access rules and the existing bounded loading/storage behavior. Add focused correctness/access/limit checks for any missing implementation; do not introduce another upload service or duplicate Fiote composer.

Separate **normal attachment delivery** from **LLM analysis**. The former must work for all six requested file types. The latter depends on the selected provider, model and adapter, and must be tried through Fiote's normal UI. Unsupported audio/video/PDF input must produce an explicit capability result while preserving the attachment and conversation; it must not be silently omitted or reported as analyzed. Do not substitute transcripts, extracted text, frames or a different provider and call that original-file analysis. Additional provider trials, if needed, use an already available, normally authenticated UI connection and identify that provider separately.

References: [shared composer](../../crates/desktop/src/message_content.rs), [attachment display/retrieval](../../crates/desktop/src/message_content/attachments.rs), [thread send paths](../../crates/desktop/src/thread_castle.rs), [Message validation/storage contract](../../crates/nucleus/src/message.rs), [ACP input conversion](../../crates/fiote/src/acp/content.rs).

## 3. Verification and concrete remaining tasks

Use `cargo check`, with warnings denied, and focused tests. Do not use `cargo build`. Completed checks are summarized here; keep the remaining steps detailed until their acceptance passes.

### A. Completed automated checks

`cargo check -p lince-cell -p lince-desktop --tests --offline` and the final check with `--features lince-desktop/native-media` both passed in the cached native environment with warnings denied. They check the shared engine/transport integration; focused tests also compile the final native changes.

| Suite | Passed | Evidence and reproduction |
| --- | --- | --- |
| Shared contract | 2 | Roundtrip, size/depth/ID/geometry/strength validation and activation parse/render; `cargo test -p nucleus --test component_composition --offline` |
| Backend presentation | 6 | Actual Action/event parsing, Record access, stable bindings, Rule execution, missing receiver, automatic protection and refusal to remove it; `cargo test -p engine --test component_presentation --offline` |
| Activation Action/Karma | 2 | Exact values, all numeric zero forms, unchanged quantity, occurrence identity and duplicate/conflicting requests; `cargo test -p engine --test fiote_activation --offline` |
| Signed draft discard | 3 | Creator/revision/addressing guards, outside versus internal dependencies, signed tombstone and retry; `cargo test -p engine --test transfer_discard --offline` |
| Native tools | 10 | Normal Transfer create/read/revise/discard, Karma access, collaborative edits, visibility and signer provenance; `cargo test -p transport --test native --offline` |
| Cell Fiote | 33 | Busy coalescing and exact causes, waiting/restart, prompt/session isolation, optional expense versus discussion, retry/source-Message evidence, latest tool input, attachment metadata and child inspection; `cargo test -p lince-cell --lib fiote:: --offline` also encounters the blocked/ignored cases below |
| Native desktop | 26 | Local bridge, instance reuse, ordinary Action buttons/events, Save/Close/reopen/copies, internal force versus crossing isolation, outside overlap, sound, event scope, labels/status/password masking and completion announcements; focused selection below |

Engine/transport suites were run with the workspace's native feature selection. Checks use the cached native environment, `-D warnings` and an isolated Cargo target to avoid other human chats' build locks. For the native selection, run `fiote::session::tests`, `component_push::tests`, `area_effects::tests`, `sound_area::tests`, `scoped_events::isolation_tests`, `accessibility::tests` and `streamed_tokens_stay_silent_and_completion_announces_once`. The 26 passing cases were run together from the freshly compiled test executable, excluding the socket-blocked credentials case. Do not describe the full Cell or native suite as passing.

Fiote activation uses migration 0207 and draft discard uses 0208. Their versions were checked against concurrent migrations. Other chats' broader Karma/social work remains separate.

Sources: [contract](../../crates/nucleus/tests/component_composition.rs), [presentation](../../crates/engine/tests/component_presentation.rs), [activation](../../crates/engine/tests/fiote_activation.rs), [discard](../../crates/engine/tests/transfer_discard.rs), [native tools](../../crates/transport/tests/native.rs), [Cell](../../crates/cell/src/fiote/tests.rs), [composition](../../crates/desktop/src/component_push/tests.rs), [area](../../crates/desktop/src/area_effects/tests.rs), [sound](../../crates/desktop/src/sound_area/tests.rs), [accessibility](../../crates/desktop/src/accessibility.rs).

### B. Finish environment-dependent automated acceptance

1. On an environment allowing local sockets, rerun three Cell cases: `agent_tools_open_without_a_model_provider_and_lock_revokes_them`, `assignment_starts_one_visible_session_and_survives_restart_without_replay`, and `mentioning_a_fiote_replies_in_the_record_with_only_recent_context`. They stop at MCP tool-server setup with `Operation not permitted (os error 1)` here. Rerun the native `native_provider_credentials_remain_separate_from_agent_login` case too; its `OpenTools` connection fails for the same reason. The local MCP listener binds `127.0.0.1:0`; these failures remain explicit, not silently skipped.
2. With `LINCE_TEST_AGENT_BIN` pointing to the local protocol fixture, run the ignored `agent_login_check_and_conversation_workflow` and `agent_question_answers_stay_on_the_question_and_cancel_when_the_turn_stops` cases. They need the fixture executable and no model. With an installed authenticated ACP agent, run `installed_agent_edits_code_and_record_and_resumes_thread`; this uses a real model turn. The three cases were ignored, not passed.

### C. Required launch and shared attachment coverage, before live acceptance

1. The owner will arrange the implementation/test session so this is the only agent working. Do not spawn agents or make worktrees. Reproduce application startup then; do not assume another agent caused the earlier read-only failure.
2. Running Lince and controlling its real UI are required acceptance conditions. Launch normally under the person's existing desktop account and normal data/configuration. Verify the installed version matches the implementation being tested. Do not count a headless test, an older installed artifact or a CLI-only model reply as installed UI acceptance.
3. If startup still fails, treat it as a reproducible launch bug and investigate before running the LLM workflow. Capture the failing operation/path, application error and wrapper/service environment; distinguish wrong application paths/permissions from an enforced execution restriction. Inspect normal writable application data/cache/state directories and desktop/session access.
4. Fix an application or packaging/configuration defect with the smallest conventional solution: correct runtime paths using the existing platform/XDG conventions, create normal application directories with appropriate permissions, or correct the launcher/service environment as evidence requires. Installed binaries may remain read-only; application state belongs in writable user directories. Preserve the normal account, stored data and existing Codex login. Avoid a new privileged helper, a second launcher architecture, blanket permission changes, or moving the account/data into a disposable home to conceal the error. An actual tool-enforced restriction must be identified explicitly; do not claim an application fix removes it.
5. Audit and, if missing, implement the shared all-conversation attachment path from section 2.9, including ordinary human conversations. Complete the applicable frontend/backend pieces and focused checks, then verify through **Attach files → choose file → enter accompanying text → Send → reopen Message → Save attachment**. Compare saved bytes with the original. Reuse passing implementation instead of rewriting it.
6. After a startup/attachment fix, run the relevant focused checks and `cargo check` with warnings denied, including native media for audio/device paths. Then continue the live UI sequence below. This update only plans that work; it does not implement fixes or launch another test.

### D. First live acceptance: existing Codex subscription and a real hello

This is a UI workflow using the person's existing Codex subscription. Do not replace it with an API-key provider, copy authentication tokens or send a CLI-only test message. Startup and missing shared attachment implementation may be fixed during the later implementation session under section C. If the normal interface cannot use the existing account, stop before inference and report the observed blocker.

1. Launch the installed Lince normally as the same person who uses Codex CLI. Inspect the actual interface and record the installed version; a compiled working tree does not establish which version is installed.
2. Click **Manage Fiote**, choose an existing Fiote or **Create Fiote**, then **Provider, model and settings** and **Change provider / sign in**. Choose the Codex subscription connection if offered. Lince's external-agent interface speaks ACP, defaulting to `goose acp`; a compatible Codex provider/bridge must actually be available. The `codex` CLI itself advertises `app-server`, not an ACP command, so its presence alone does not validate the Lince connection.
3. Click **Check connection · no tokens**. The Codex-backed agent should reuse its existing cached ChatGPT login under the same user/environment. Confirm session readiness through the interface. If it asks for account authentication, use its normal sign-in interface; if existing-account login cannot complete normally, stop here. A CLI login-status result alone does not prove the provider can generate.
4. Click **Save and load choices**. Select **GPT-6 Luna** (`gpt-6-luna`) and **Low** reasoning when offered, then the offered **Fast / normal** choice. Start with Fast enabled if it is available, recording the actual selected value. Model, reasoning effort and Fast mode are separate settings. If a requested control is not offered, report that limitation instead of assuming an unshown setting was applied. Official [Codex model guidance](https://learn.chatgpt.com/docs/models) and [speed guidance](https://learn.chatgpt.com/docs/agent-configuration/speed) describe availability; the live adapter's choices establish what Lince can set.
5. Click **Check and open conversation**. Enter **Hello** in the normal composer and click **Send** once. Wait for completion without restarting or retrying an uncertain turn.
6. Inspect the displayed assistant reply and completed state, then reopen the conversation to confirm the saved reply. Record the chosen provider/model/reasoning/speed, sent Message and received reply. A ready connection, echoed input or error text does not count as a model response. Do not use a model's own claimed identity as proof of the selected model.

Inspection/attempt on 2026-10-01: installed `codex-cli 0.159.3`; `codex login status` returned **Logged in using ChatGPT**. Official [authentication guidance](https://learn.chatgpt.com/docs/auth) confirms cached logins are reused. Launching installed `lince` exited before its UI with `ReadOnlyFilesystem` (OS error 30). The desktop user bus returned `Operation not permitted`; installed Goose ACP also failed to open its session database directory because the home filesystem is read-only in this sandbox. Provider discovery, UI account readiness, model/settings selection and the hello/reply remain unverified. The live workflow stopped; no model request or credential copying was performed. Source code is unchanged by this inspection.

### E. File attachments and audio through the same conversation UI

Run this after the real hello succeeds. Prepare small fixtures with known contents and answers, then send each as an actual attachment with accompanying text in a normal Message. First verify the ordinary human-conversation delivery path, then repeat the six formats in a Fiote conversation through **Attach files → choose file → enter the request → Send**. Inspect the attachment card and sent Message, wait for the Fiote outcome, reopen it and save the attachment to verify its bytes. A response must demonstrate access to file contents rather than repeat its filename or the question.

| Order | File | Request and evidence |
| --- | --- | --- |
| 1 | UTF-8 text (`.txt`) | Ask for a distinctive sentence or marker stored only in the file; check the answer against its known text |
| 2 | CSV (`.csv`) | Ask for row count and a simple numeric total; verify columns, values and the known total rather than guessed summaries |
| 3 | PDF (`.pdf`) | Use a small document with known text; ask for a fact and its page, verifying actual PDF input/read access |
| 4 | Photo (`.jpg` or `.png`) | Ask about a known visible object/count or receipt amount; check the image content and the saved original |
| 5 | Audio (`.wav` or another offered format) | Send a short clip with a known spoken phrase; ask what was said. Separately exercise **Preview / play → Close preview / stop**. Record analysis support and playback results independently |
| 6 | Video (`.mp4` or another offered format) | Send a short clip with known events in order; ask what happened first/last. Preserve the original clip and record whether the model/adapter can access temporal content |

For every row, record filename/MIME/size, conversation and sent Message, provider/model/settings, stored/downloaded-byte result, actual reply, expected-content comparison and one explicit outcome: analyzed, model/adapter unsupported, or application/transport failure. A model limitation is acceptable evidence of the attempted analysis; a broken ordinary attachment path remains work to fix. An unsupported row must not prevent trying the remaining formats. Preserve the original draft/Message and continue with a new Message without silently dropping the problematic file.

Also send accompanying text with two different supported attachments in one Message and verify both arrive. In another draft, remove one selected file before sending and confirm only the retained files arrive. Check cancelled selection and one oversize rejection without losing the draft. Check refusal/retry behavior does not duplicate a Message. Keep retries deliberate after inspecting the saved state.

Exercise microphone dictation separately from attaching an audio file: **Dictate → speak a known phrase → Stop and transcribe → inspect/edit the unsent text → Send**. Confirm transcription does not send automatically, and **Cancel** leaves no unintended Message. Record the speech provider and device result independently of Codex's audio-attachment capability; the existing dictation feature adds text, not the captured audio file. Missing microphone/provider readiness must be reported rather than counted as a passed audio test.

### F. Further installed acceptance, in order

1. **Runtime readiness:** check the installed executable/wrapper and user-service environment outside a development shell. Select/configure a provider, log in, inspect readiness, send a message, Stop and restart. Confirm credentials and interrupted work behave as shown in the UI.
2. **Protected interactive UI:** ask Fiote to combine supported components with an Action button and a named event. Apply an authorized edit immediately. Place outside areas across its balloon; check both crossing directions are blocked while inner components interact. Save, Close, restart/reopen and make a library copy; verify independent protection and persisted supported edits.
3. **Proposal and Simulation:** request a report with edit/apply/dismiss controls. Preview an unsaved Karma alternative, compare its evidence, apply the chosen revision, then provoke a stale-data refusal. Confirm previews do not start a provider or emit real canvas effects.
4. **Activation and Transfer:** configure a Frequency/Rule to activate Fiote with a nonzero value, then trigger it again while busy. Inspect individual causes, one pending run and unchanged Record quantity; Stop, disable and restart. Through conversation, create/read/revise/discard an unused Transfer draft and inspect its signed evidence; use normal publication/agreement/cancellation controls for a committed Transfer.
5. **Conversation and evidence:** discuss without requesting recording, then request an exact expense and inspect Fact agent/thread/source-Message attribution. Use a real receipt with an image-capable provider; confirm amount/unit/date/meaning, clarification of ambiguity and correction. Retry must not duplicate an entry. Mocked provider tests establish the Action route, not extraction quality.
6. **Inspection and external sessions:** inspect pinned/current instructions and the latest direct request including tool input/results. Check attachment metadata without exposed credentials. Report an existing child thread and open it; external sessions must label their starting-input snapshot rather than imply visibility of unreported agent memory.
7. **Accessibility and media:** use a screen reader, Tab/Shift+Tab and focused button activation for the composer, settings, question answers and Save/Close. Check final-reply and refusal announcements, cancellation and editable unsent microphone dictation. In the media-enabled Communication panel, complete a real call/device workflow; displaying its controls alone does not establish media acceptance.
