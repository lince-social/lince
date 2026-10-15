# Fiote remaining work

Planning update: 2026-10-15. This file contains remaining tasks and deferred follow-ups. Review and refine the new voice/routing scope before implementation. Remove each task only after its backend, interface, and acceptance checks are complete.

## Constraints and reuse

- New speech and routing inference must run through Rust libraries with no C++ dependency, including transitive features. No required external agent, Python, Node, model daemon, or ACP launcher. CPU execution is the baseline; acceleration must also satisfy this restriction.
- Reuse the native Fiote harness, provider abstraction, normal Messages/Actions, working-directory inheritance, credential vault, and canvas broker. Speech and routing are separate capabilities; a chat connection does not automatically provide transcription or synthesis.
- The current interface is crates/interface/src/native, replacing crates/desktop. Implement new UI there. The existing dictation code is native/speech.rs; capture is native/sound/device.rs. The service is crates/cell/src/speech.rs, with its endpoint adapter in crates/fiote/src/communication/speech/endpoint.rs.
- Simple endpoint dictation already records audio, returns editable text, supports cancellation and microphone selection, and preserves an unsent draft. Reuse it; the missing feature is embedded transcription without setting up an endpoint.
- Fiote creation and ancestor instructions live in crates/cell/src/fiote/behavior.rs and crates/fiote/src/prompt.rs. The existing directory() management selection chooses Development Fiote; it is not an assistant default. Select the intake Fiote from the actual hierarchy, without a builtin name or seeded UID.
- Parent relationships currently govern instructions and working directories. They do not by themselves delegate requests. The existing @mention path can select an enabled Fiote for a person's message; it does not establish agent-to-agent handoff.
- lince_message writes an attributed reply in its own thread. In crates/transport/src/session.rs, automatic Fiote dispatch currently intercepts a finished CreateMessage with author=None, parent=None, and no references. An attributed or linked forwarded Message therefore needs explicit supported dispatch; changing a prompt alone cannot deliver this feature.

## Sequential voice and routing tasks

### 1. Automatic default Fiote and destination selection

- [ ] Select the default intake Fiote by child count, then age, and use it as the reliable fallback for assistant requests.

Among the nondeleted Fiote Records authorized for the person's assistant use, choose the one with the most direct Fiote children. "Children" means immediate configured Fiotes whose current parent is that candidate; do not count grandchildren, Threads, task sessions, ordinary Records, duplicate assertions, or deleted children. Count only children visible in the authorized scope. Read parent relationships through the existing Engine/vocabulary contract, not a hardcoded ancestry predicate UID.

For equal child counts, choose the oldest Record by its persisted created_at timestamp, comparing parsed instants rather than display strings. For equal timestamps, use ascending Record UID as the final deterministic tie-break. A candidate with zero children is valid when no candidate has more. This interpretation uses direct children, not total descendants.

Compute the ranking across the authorized hierarchy with a consistent read and bounded batched queries. The current fiote_choices() helper returns at most 1,000 choices ordered by title; ranking only that truncated list could pick the wrong default. Reuse or extend discovery to count the complete relevant set, or report an explicit supported-size limit. Avoid one database round trip per child and test that ordering/renaming cannot change the winner.

Connection readiness is checked after choosing and must not silently change this ranking. If the winning Fiote is disabled, disconnected, or unauthenticated, preserve the request and show its connection setup. Do not borrow another Fiote's account or silently select a runner-up. If no authorized Fiote exists, show normal Fiote creation/setup rather than inventing an identity.

Resolve the winner for each new untargeted assistant conversation/request and pin that recipient once dispatch starts. A hierarchy change affects subsequent untargeted requests, not a running turn or an existing conversation. Show the chosen Fiote, direct-child count, and reason for selection. Do not store a fixed builtin default or rewrite parent relationships to make the selected Fiote a root; "default parent" means the intake/fallback role here.

Ordinary dictation into an existing conversation preserves that conversation and its intended recipient. Explicitly selecting a Fiote also preserves that choice. Automatic routing applies to the assistant entry point when no recipient was selected.

Once a request is explicitly submitted or admitted by the listening-mode decision in step 5, System 1 destination routing being off, missing, loading, unavailable, inconclusive, or failed sends it once to the automatically selected default Fiote. That Fiote may answer directly, ask for clarification, or delegate. Admission of ambient speech is a separate decision; a failed admission check must not send every overheard phrase to the default Fiote.

Acceptance: test different child counts, equal counts with different ages, equal ages, zero-child candidates, deleted/hidden children, reparenting, no candidates, and a disconnected winning Fiote. Disabling/removing destination routing still allows the selected configured Fiote to answer; a missing connection preserves the request; dictation in a child conversation stays with that child; existing conversations and active turns do not switch recipients when ranking changes.

### 2. Real Fiote-to-Fiote dispatch

- [ ] Provide a native handoff operation that the parent Fiote can actually invoke.

First test whether existing Actions plus service dispatch can safely express handoff. Reuse those paths where possible; otherwise add one narrow operation to the native tool catalog and Fiote service. Do not create a second agent runtime. Its proposed inputs are a stable request ID, target Fiote UID, source Message/thread, and the delegated request. Include attachments through ordinary Message references/content rather than dropping or copying their bytes into a model prompt.

Resolve targets through authorized Fiote discovery. Validate readable/writable scope, an enabled connection, and the person's configured eligibility for assistant routing. The recipient uses its own instructions, tools, account, and effective working directory. Handoff must not alter ConfigureFiote, grant permissions, edit ancestor prompts, or impersonate the person.

Persist the source/destination relationship and return the destination thread, Message, and dispatch receipt. Distinguish accepted, queued, running, completed, failed, and uncertain outcomes. The conversation UI must show who received the request and link to its conversation. A dispatch receipt is not proof that the requested work completed.

Use the existing concurrency and activation facilities where their semantics fit. Queue or schedule handoff outside locks held by the current tool invocation; avoid reentrant calls waiting on the running-thread mutex. Reject self-handoff and delegation cycles, bound the number of hops, and deduplicate retries using the source request and target. An interrupted or uncertain dispatch must be inspected before retrying.

Acceptance: a scripted parent can invoke the actual tool, a child responds or performs a disposable fixture change, and the initiating conversation can inspect the result. Also test a parent answering without delegation, attachments, unavailable targets, authority removal, duplicate requests, loops, cancellation, busy recipients, and reopen/recovery.

### 3. Seed and session instructions for optional delegation

- [ ] Explain routing in the default parent's seed description and expose the needed context/tools.

Extend crates/fiote/src/prompt.rs DEFAULT and the relevant Fiote seed/session instruction paths. The intended instruction is: "You may handle the person's request yourself or delegate it to an eligible Fiote when that Fiote's description makes it a better fit. Discover the available Fiotes and read their descriptions before choosing. Use the supported handoff tool, preserve the original request and its references, and report the recipient. Ask for clarification when the target is unclear. Delegation is optional and does not grant additional authority."

Keep this behavior configurable through each Fiote's description. Supply the currently selected intake identity, the current recipient's role, eligible Fiote descriptions, and supported handoff schema through bounded discovery. Any Fiote selected by the ranking must be able to discover and invoke handoff, even if it does not inherit the builtin root description. The common session wrapper supplies role/tool guidance; user descriptions decide whether delegation is appropriate. Selection must not rewrite descriptions or force a new ancestor. Do not append every descendant's complete instructions to every turn. Preserve user-authored descriptions rather than resetting them whenever Lince starts.

Account for pinned session instructions: refresh them through the existing instruction flow when descriptions change, and preserve the routing instructions across compaction. Descendants inheriting the common description must receive context identifying their actual role and the delegation chain, so inheritance does not make every child behave as the global intake router. A ranking change does not repurpose instructions already pinned to an existing conversation.

Acceptance: fresh seeding includes the instructions; a differently named/nonbuiltin Fiote winning the ranking discovers and invokes handoff; customized descriptions can choose to answer directly; instruction refresh and compaction preserve the behavior without changing ancestry or user descriptions.

### 4. Embedded simple dictation

- [ ] Add local Rust speech-to-text to the existing Dictate / Stop and transcribe / Cancel workflow.

Start with multilingual Whisper through Candle, subject to the dependency and performance checks below. Reuse microphone capture, resampling, WAV validation, duration limits, job cancellation, and draft insertion. Run inference on a bounded worker away from the UI and async executor. Load weights once and share the model service across message composers.

Add a speech-engine choice between embedded local transcription and the existing compatible endpoint. Offer a small multilingual default, a language preference including Portuguese and English, and visible model download/loading/error state. Model files need pinned revisions, integrity checks, licenses, known storage requirements, and an import/offline path. No download or credential requirement should be hidden behind the Dictate button.

Simple dictation produces editable text and never sends automatically. Preserve text typed during transcription. Silence, cancellation, an unavailable microphone, a closed composer, or a model error must leave the draft intact and release the device. Continue to respect the microphone coordination already used by calls.

Acceptance: real Portuguese/English speech fixtures transcribe through the embedded engine without HTTP or installed programs; existing native interface tests activate the controls and verify insertion/cancellation. Use actual microphone acceptance for device capture, not as a replacement for reproducible fixtures.

### 5. Optional local System 1 admission and routing

- [ ] Add a replaceable Rust decision-model interface and evaluate an open model such as Laya.

Define a small interface accepting the request, bounded relevant conversation/workspace context, and eligible Fiote UID/description choices. Return a typed choice, uncertainty/abstention, and model identity. Support two separate judgments: whether an utterance is an assistant request, and which Fiote should receive an admitted request. They may share one loaded model, but admission must succeed before a destination choice can dispatch. Keep decision-model implementation code separate from the Fiote orchestration layer. An ordinary LLM parent remains fully usable without this dependency.

Evaluate the Rust libraries first: [laya-rust](https://github.com/aovestdipaperino/laya-rust) exposes in-process Candle inference; [sys1](https://github.com/alvarobartt/sys1) is a Rust server implementation that can inform model support. Prefer embedding a library/model implementation over requiring its executable or opening a localhost server inside Lince. Verify the chosen checkpoint architecture, multilingual support, inference parity, license, and actual dependency features before choosing either implementation.

"No C++" requires more than a Rust top-level crate. In particular, [tokenizers 0.21 defaults](https://github.com/huggingface/tokenizers/blob/v0.21.4/tokenizers/Cargo.toml) include esaxx_fast, which enables esaxx-rs/cpp; the examined laya-rust manifest uses tokenizers defaults. Disable these features in every dependency enabling them, or reuse the permitted model implementation with a narrowly maintained dependency adaptation. Do not substitute ONNX Runtime, libtorch, sherpa-onnx, whisper.cpp, CUDA C++ kernels, or Flash Attention dependencies. Inspect the resolved Cargo feature graph and verify the CPU inference build.

Make the model optional and disabled until a suitable checkpoint is available and passes the applicable evaluations. Model import/download should reuse the local model management from dictation. Route an admitted request directly only for a validated, eligible, sufficiently clear destination decision; otherwise use the ranked default Fiote. A stale decision from a cancelled request must never dispatch.

- [ ] Let System 1 decide whether speech should activate any Fiote when general listening mode is explicitly enabled.

Reuse local microphone capture, a Rust utterance segmenter, and the embedded transcription engine. Convert completed utterances into bounded local text and give the admission model choices such as assistant-request, ordinary-conversation, and unclear. Include only the short, relevant context needed to determine whether the request addresses the assistant; the model does not need broad Record access. Normal dictation, an explicit Send, a selected-recipient request, and a recognized wake phrase do not need this ambient-speech admission judgment.

Ordinary conversation, background speech, and unclear admission produce no Message, Fiote activation, tool call, or cloud request. Briefly show that speech was ignored or leave the local transcript available for explicit submission, according to the person's selected UI option. Discard the temporary audio/transcript after the bounded decision or cancellation unless the person explicitly saves/submits it. Do not build a permanent record of all nearby conversation.

An admitted request goes to destination routing, or the ranked default if destination routing is unavailable/uncertain. If admission itself is unavailable or errors, general listening falls back to explicit activation through the microphone/Send controls, and later the wake phrase; it does not ask a cloud LLM about every utterance. Show this degraded mode clearly. The ordinary default-Fiote fallback remains available for every explicitly submitted request.

Fast decision inference is useful, but the end-to-end path also includes microphone segmentation and speech-to-text. Measure throughput and latency for all stages before claiming it can keep up with people speaking. Use a bounded worker queue, shared loaded models, cancellation/generation checks, and no duplicate activation from overlapping transcription windows. When a queue is full, show busy and stop accepting more ambient segments rather than silently dispatching stale requests later. Process a single utterance at a time initially and do not claim reliable separation of simultaneous speakers. An ambiguous transcript should not auto-activate Fiote; processing speed does not solve overlapping voices.

Measure false activations on ordinary Portuguese/English conversations and missed assistant requests, including quoted commands, television/playback, pauses, similar Fiote names, and ambiguous pronouns. Destination accuracy and admission accuracy need separate tests and thresholds. A confidence value alone does not establish that speech was addressed to Lince. Ambient-mode thresholds should favor no activation when uncertain; explicit activation remains the reliable route.

The [Laya project's reported limits](https://github.com/NandhaKishorM/laya#honest-limits) show that base checkpoints can perform poorly on typed decisions, and results from a checkpoint trained on one benchmark do not establish Fiote admission/routing accuracy. Use a held-out Portuguese/English suite with overlapping descriptions, explicit names, unknown requests, changed label order, and ambiguous multi-purpose requests. Measure errors and abstention, not just confidence. Keep candidate descriptions short enough that neither options nor request are silently truncated. If a model passes routing but fails admission, enable only the tested routing capability. If the model is inadequate, retain explicit activation and the parent fallback and report the limitation; mandatory training is not a prerequisite for usable dictation/Fiote.

Acceptance: run real embedded-model admission and routing checks, measure end-to-end latency, throughput, and peak memory on CPU, verify expected outputs against reference fixtures, and demonstrate unavailable/low-confidence/cancelled destination routing falling back without duplicate messages. Ordinary/unclear speech and admission failures must trigger no Fiote or cloud request. Test opt-in/off state, silence, model-loading failure, full queues, cancelling late results, and separation of admission from destination uncertainty.

### 6. Native interface and end-to-end acceptance

- [ ] Complete assistant controls and test the entire request path.

Keep simple dictation inside ordinary message composers. Add an assistant entry point for an untargeted request, explain the automatically ranked default Fiote, show its connection status, and allow an explicit recipient for a request/conversation. Add optional destination routing and a separate general-listening toggle, off initially. General listening uses local admission; simple dictation remains editable and unsent. Show Waiting / Listening / Transcribing / Deciding / Working, ignored/busy/degraded status, and visible mute/Stop controls. Share the existing message composer and Fiote connection controls instead of requiring provider-specific voice screens.

Use scripted native interface actions and an isolated Cell/Engine to exercise hierarchy ranking, dictation, transcript edits, parent answers, parent-to-child delegation, System 1 admission/destination choice, and parent fallback. Assert that ignored/uncertain ambient speech creates no activation or Message and calls no cloud provider. Verify stored Messages, source links, receipts, resulting fixture files/data, and logs; a label saying "sent" is insufficient.

Include real ASR and decision-model fixtures as explicit model-backed acceptance. Deterministic fake models test orchestration; they do not prove recognition or classification. Local inference tests require no subscription. A real LLM parent/child test can use the available native subscription when explicitly authorized and should record the selected model and actual outcome.

Run appropriate cargo check and targeted tests with warnings denied, through cargo xtask or the supported Nix environment. Check new dependency licenses/credits in the affected Sand. Test CPU operation outside the development shell and offline after models are installed. Do not claim wake-word activation or spoken replies from dictation/routing acceptance.

## Architecture alternatives and reporting

Keep one native harness, one supported handoff path, the existing speech service, and a replaceable local decision-model boundary. Discovery, permission checks, receipt handling, and UI state must use those shared paths. A model or library failure is not permission to introduce an external agent, duplicate runtime, Node/Python process, or C++ implementation.

- If existing Actions cannot safely dispatch an attributed child request, add the narrow native handoff/service operation from step 2 and prove it through the same Session authority checks.
- If Laya's library, checkpoint architecture, dependency features, or measured results fail the restrictions, evaluate another open Rust decision-model implementation behind the same interface. A small, licensed adaptation of the existing Rust implementation is preferable to a parallel provider/harness stack. Until a compliant model passes, destination routing uses the ranked default and ambient listening uses explicit activation.
- If Candle Whisper is too slow or cannot satisfy a supported target, evaluate another compliant Rust transcription implementation/checkpoint behind the existing speech interface. The compatible endpoint remains an explicit user-configured alternative; do not silently upload audio or require a local daemon. If embedded transcription cannot be delivered, leave its task unchecked and report the concrete blocker.
- If continuous listening cannot keep up or has unacceptable false activations, keep ordinary dictation and explicit activation functional, leave admission disabled, and report the failing measurements. Do not advertise a reliable always-listening assistant based solely on a fast model forward pass.

For each deviation, report what failed, the relevant test/log or dependency evidence, the chosen alternative, whether it meets the Rust/no-C++ and self-contained requirements, and any resulting limitation. Proceed with an equivalent alternative inside the agreed architecture; seek refinement before relaxing a hard restriction or changing observable behavior. Keep incomplete acceptance tasks in this file.

## Later voice stages

These preserve the original voice-assistant direction after simple dictation and routing. Refine the implementation/model choice before starting them.

- [ ] Add opt-in "Hey Fiote" activation through a Rust wake-word detector as another explicit activation route alongside general listening. Evaluate Rustpotter with recorded Fiote pronunciations, and audit its resolved dependency features against the no-C++ restriction. Reuse the same capture, transcription, routing, and dispatch services. Show Off / Waiting / Listening / Working states, retain a short local audio buffer so the start of the request is not lost, and bound recording/end-of-speech detection. Send through the same routing path only when voice-assistant automatic sending is enabled; normal dictation remains editable and unsent. Test false activations, missed words, silence, timeouts, cancellation, microphone conflicts, and English/Portuguese pronunciation. Keep a microphone-button fallback.

- [ ] Add optional spoken Fiote replies through a Rust synthesis implementation and existing audio playback. Evaluate Portuguese/English voices and CPU latency before picking the model; model weights and any phonemizer must also satisfy no C++. Provide voice selection, mute, and Stop. Initially suspend wake-word activation during playback to prevent self-triggering. Interrupting playback and stopping a running Fiote are separate controls. Full-duplex conversation and reliable interruption while speaking need separate refinement and measured echo handling.

## Deferred native-component follow-ups

The base canvas inspection, atomic authoring, token/link/group/behavior editing, library save/reuse, guarded undo, durable receipts, and native file/command tools already have implementation and targeted coverage. Their old implementation tasks and historical acceptance reports have been removed. The following are additional capabilities retained from the previous plan, not prerequisites for voice/routing.

- [ ] Parameterize reusable component Record bindings and add explicit definition revision/update policies. Current library reuse keeps explicit bindings and supports Rebind. Introduce named binding slots and declared dependencies; instances must not silently receive a changed definition. Test save/recreation with new Records, nested definitions, missing assets, and an explicit update retaining instance overrides and editor state.

- [ ] Add canonical structural differences and durable authoring provenance where existing receipts are insufficient. Relate originating task/thread, target instance/part, inspected/exported revision and saved definition. Compare authored constraints, tokens, bindings, links and behaviors separately from resolved geometry and transient Entity IDs. Reuse the existing revisions/receipts rather than adding a parallel canvas history.

- [ ] Refine promotion of a reviewed native component into Lince defaults. Pin the exported authored revision/hash and parameterized bindings, make the Rust factory consume the shared definition, and use the existing repository tools. Source edits do not update compiled behavior until the application is rebuilt and restarted. Preserve unrelated work; source promotion does not automatically authorize commit, publication, or replacement of the running application.

- [ ] Refine the personal Fiote workflow for launching another Lince and comparing the result. Reuse cargo xtask/Nix and Laboratory with disposable data. Match viewport, theme, fonts, scale, content, disclosure and bindings; compare composition/interaction evidence as well as screenshots. Independent-process launch, screenshot orchestration and source promotion remain separate from the completed canvas CRUD.
