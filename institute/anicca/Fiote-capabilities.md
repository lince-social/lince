# Fiote capabilities: tasks for discussion

Revised 2026-09-26. This is the first review round, not approval to implement. Only this document is being changed. Selected work: live settings, richer messages, additional directories, usage and cost, plans and progress, command discovery, structured questions, authentication, and dictation.

## Where the capabilities belong

Lince owns messages, drafts, their contents, and the controls people use to compose and read them. Fiote connects those capabilities to an agent or model. A person-to-person thread must benefit from the same message features without requiring a Fiote.

| Capability | Shared Lince capability | Fiote's part |
| --- | --- | --- |
| Images, audio, files | Message contents, storage, sharing, previews, playback | Translate supported contents into agent/model requests |
| Dictation | Speech-to-text service that writes into an editable, unsent draft | Use the resulting text like any other message |
| Questions | Message forms, answers, recipients, and response state | Translate agent requests and return answers |
| Plans and progress | Message-linked steps, activity, and status displays | Apply agent updates |
| Commands | Composer discovery, completion, and explicit invocation | Supply commands from the current agent session |
| Usage and cost | Optional operation metadata and reusable displays | Collect reported model/session usage and cost |
| Authentication | Account setup, secure credentials, and login status | Connect agent/provider login methods |
| Live settings and directories | Reusable controls and context references where useful | Own agent session settings and filesystem context |

Reusable does not mean putting every state into message text. Credentials stay outside messages. Usage and progress can be attached metadata. Unsent drafts stay private. Session controls belong to the relevant Fiote/session. Only intended message contents enter model context.

## Current baseline

- Provider setup, supported model/thinking/speed choices, working directory, saved options, threads, resume, stop, tool activity, and permission prompts already exist.
- The previous fix added connection checks without sending a model prompt, clearer startup errors, and login cancellation. Those checks do not prove generation, quota, or every provider's credential validity.
- Both Fiote input paths currently accept user text: ACP sends a text block, and the direct provider path uses a string for user messages. Rich content needs backend and interface changes.
- Lince has Message and MessageDraft Records and native microphone capture for calls. Reuse suitable behavior; a composer draft must not accidentally become a queued message.
- Pinned tooling: Goose 1.52.0, ACP client 2.2.0, ACP schema 1.9.1, genai 0.6.5, modelbridge 0.5.2. The previous tooling review retained stable genai instead of its beta.
- The tested Goose Linux binary advertises images and embedded context, but not audio prompt attachments. Speech transcription is separate. A schema alone does not prove the installed binary, selected model, or configured provider supports a feature.

## Tasks, in implementation order

Start with the small shared foundation, then follow the requested feature order. Every feature includes its backend, Rust interface, and relevant tests. Recommendations in the review section remain open for discussion.

### 0. Shared message foundation and capability reporting

- [ ] Define the smallest extensions for ordered contents, attachments, editable drafts, questions, and message-linked activity. Reuse Records, ownership, and sharing rules.
- [ ] Distinguish contents, execution metadata, local drafts, and private account state. Define what is saved, shared, and included in model input.
- [ ] Show support as available, unavailable, or unverified for the connected agent and selected model. Explain unavailable actions and keep ordinary thread capabilities usable.
- [ ] Receive session updates while idle as well as during replies. Route by connection and session; discard stale updates after switching or reconnecting.

Done when later features can reuse these contracts without a second Fiote-only message system. Avoid building a general extension framework before these concrete needs require one.

### 1. Live settings

- [ ] Put supported provider, model, thinking, speed, mode, and other advertised choices beside the active conversation.
- [ ] Separate Fiote defaults from choices for this conversation. Show the scope and effective value.
- [ ] Apply changes to the actual conversation session and reflect the full returned settings, including dependent options. Restore the previous value on failure.
- [ ] Reflect agent-initiated changes and restored-session settings.
- [ ] Define changes during a running turn. Proposed default: next turn unless the agent explicitly supports changing during execution. Show pending changes.
- [ ] Handle provider changes separately when they require login or a new session. Do not imply every provider supports switching within a conversation.

Done when a change affects the intended session, another thread stays unchanged, and reconnecting shows accepted settings. [ACP settings](https://agentclientprotocol.com/protocol/v1/session-config-options).

### 2. Images, audio, and file references in messages

- [ ] Let any thread compose ordered text, images, audio, file attachments, and resource references. Add attach/paste/drop, preview, remove, and playback controls where appropriate.
- [ ] Define attachment storage and transfer using existing Lince storage where suitable. Cover type, size, filename, content identity, access rules, transfer state, retention, and cleanup.
- [ ] Distinguish copied file snapshots, Lince Record/resource references, and local filesystem references. Show which is being sent. A local path does not transfer a file to another device.
- [ ] Preserve contents through draft editing, sending, reopening, synchronization, and session resume. Show missing or inaccessible content.
- [ ] Translate the same contents into ACP blocks and supported direct-provider inputs. Display rich agent replies through the shared message components.
- [ ] Check support before Fiote dispatch. Offer explicit conversion or removal when necessary; never silently discard attachments. Human recipients can still receive content Lince supports.
- [ ] Keep audio attachments and dictated text separate. Never transcribe or send a recording implicitly.
- [ ] Bound file size, decoding, and memory use. Test malformed content, unauthorized reads, interrupted transfers, and mixed content order.

Done when the same message works in a human thread and reaches a capable Fiote intact. Test images, audio, and references separately. Unsupported Goose audio prompts must show a limitation, not false success. [ACP content](https://agentclientprotocol.com/protocol/v1/content).

### 3. Additional directories

- [ ] Add a directory list alongside the primary working directory, with browse/add/remove and validation.
- [ ] Associate paths with the agent's machine. Distinguish them from files attached by a remote user.
- [ ] Save Fiote defaults and conversation overrides. Send the full intended list on every supported new/load/resume request.
- [ ] Enable the integration only when advertised by the agent. Explain when changing directories requires a session restart.
- [ ] Explain that directory context does not itself enforce a sandbox, grant OS access, upload files, or read all contents.

Done when resumed sessions receive the selected directories and unsupported agents cannot appear to accept them. [ACP workspace roots](https://agentclientprotocol.com/protocol/v1/session-setup).

### 4. Usage and cost

- [ ] Show reported context occupancy/capacity, session cost, currency, and last update.
- [ ] Preserve each value's source and scope: context size, operation, turn, or cumulative session. Missing data means unavailable, not zero.
- [ ] Collect direct-provider token counts and supported agent reports. Do not present context occupancy as total billed tokens.
- [ ] Handle cumulative updates, retries, resume, and model changes without double counting. Keep currencies separate.
- [ ] Store optional operation metadata, including transcription usage when reported. Define shared-thread visibility; account spending is private by default.
- [ ] Label reported and estimated costs. Keep enforced spending limits as separate work.

Done when reopening or replaying a session does not increase recorded cost. Standard ACP session usage covers context and optional cumulative cost; detailed end-turn token reporting is a separate experimental surface in our installed schema. [ACP usage](https://agentclientprotocol.com/rfds/session-usage).

### 5. Plans and progress

- [ ] Add shared message-linked step lists and operation status displays that people and other Lince services can also use.
- [ ] Map reported plan steps, priority, and state. Replace the current agent plan when a full new plan arrives.
- [ ] Show running, waiting for input, completed, failed, interrupted, and disconnected operations separately from agent step statuses.
- [ ] Save the latest state and enough history to explain changes without creating a chat message for every update. Restore without duplicates.
- [ ] Keep agent plan editing/approval outside the initial integration. Editing a person's own step list must not imply an agent's plan changed.

Done when a restart preserves progress and interrupted work never appears completed. Show reported steps, not invented completion percentages. [ACP plans](https://agentclientprotocol.com/protocol/v1/agent-plan).

### 6. Command discovery

- [ ] Add a shared composer command picker with search, descriptions, argument hints, and keyboard completion.
- [ ] Populate Fiote commands from session updates; refresh when the agent changes the list.
- [ ] Distinguish Lince commands from agent commands and show the target in a thread with several participants. Define name collisions and literal slash-prefixed text.
- [ ] Insert selected commands into the draft for review. Execute only after explicit action; discovery and selection must not issue a model prompt.
- [ ] Preserve command syntax and show that execution can invoke tools or use tokens.

Done when completion executes nothing and invocation reaches only the chosen target. [ACP commands](https://agentclientprotocol.com/protocol/v1/slash-commands).

### 7. Structured questions

- [ ] Add shared question messages with fields, choices, validation, intended responder, and pending/answered/declined/cancelled state. People must be able to use them without Fiote.
- [ ] Let responders review/edit answers before submission. Distinguish decline from cancel.
- [ ] Translate supported ACP forms and route answers to the exact waiting request. Do not submit an answer twice as both a protocol response and another prompt.
- [ ] Handle multiple requests, timeouts, disconnects, and repeated responses. A saved question does not make a dead protocol request answerable after restart.
- [ ] Show URL requests as private interaction controls with requesting agent and destination host. Opening a URL does not mean authorization finished.
- [ ] Keep credentials and sensitive authentication URLs outside shared messages and model context. Return only the permitted result.

Done when human questions and agent forms share interaction components while answers reach the correct recipient. Advertise form/URL support only after their complete flows exist. [ACP questions](https://agentclientprotocol.com/protocol/v1/elicitation).

### 8. Authentication and installed-service reliability

- [ ] Extend setup with reusable account status: executable, provider configuration, login pending, expiry where detectable, and last connection check.
- [ ] Add interactive terminal login only when Lince can reproduce the advertised invocation with the same executable, environment, and account as its agent. Keep the terminal outside message history.
- [ ] Add supported logout. Distinguish disconnecting Lince, clearing local credentials, and revoking provider access.
- [ ] Keep cancellation, retry, timeouts, account switching, and stale-result rejection consistent across browser, device-code, key, and terminal flows.
- [ ] Keep credentials in the appropriate store. Never expose them in logs, messages, or synchronized Records.
- [ ] Preserve checks without generation and report exactly what remains unverified.
- [ ] Verify Nix source and prebuilt packages as a service with minimal PATH. Define runtime dependencies for raw GitHub archives and other supported platforms.
- [ ] Diagnose resolved executable/version and runtime account without dumping environment secrets. Report missing provider/tool executables separately.

Done when login works after service restart and missing executables have clear remedies. This extends the previous bug fix. [ACP authentication](https://agentclientprotocol.com/protocol/v1/authentication).

### 9. Dictation as speech-to-text for any message draft

- [ ] Add a reusable speech-to-text service with provider/model configuration and readiness. A chat model suffices only if it supports the required transcription API.
- [ ] Activate the microphone from the ordinary composer, with visible recording and stop/cancel controls. Settle button and keyboard behavior during review.
- [ ] Form the unsent message while the user speaks. Stream where available; otherwise transcribe short audio segments and show pending text.
- [ ] Allow editing of the transcript and existing draft. Apply late results only to their intended draft/segment, without overwriting edits or writing into another thread.
- [ ] Stop capture and finalize the draft. Never send automatically on silence, stop, or transcription completion. Sending remains the normal message action.
- [ ] Keep audio temporary by default. Retaining/sending a voice attachment is separate. Explain when cloud transcription receives audio even though the message is unsent.
- [ ] Reuse suitable native capture, including device selection, permission failures, and active-call handling. Bound recording length, buffers, and queued transcription.
- [ ] Connect Goose dictation as one possible backend, gated by actual availability. Its request accepts audio and returns text; continuous draft updates need segmenting or another streaming backend.
- [ ] Test interruption, silence, duplicates, manual edits, focus changes, network errors, and unavailable providers.

Done when someone can dictate, edit, and send in a human-only thread without Fiote or chat generation. Transcription can still have its own provider cost. [Goose dictation schema](https://raw.githubusercontent.com/aaif-goose/goose/v1.52.0/crates/goose/acp-schema.json).

### 10. End-to-end acceptance

- [ ] Exercise human-only and Fiote threads using the same drafts, attachments, questions, and progress components.
- [ ] Test capability differences with fixtures, including unknown and unsupported features. Use local audio/image fixtures; do not spend tokens just to check wiring.
- [ ] Verify reconnect, replay, cancellation, cross-thread routing, access control, and bounded storage/memory.
- [ ] Verify the installed service outside the development shell. Record real provider/model combinations tested and those still unverified.
- [ ] Run focused tests and cargo check for changed crates when implementation is authorized. A schema or transport test alone cannot prove model support.

## First criticism round: decisions to refine

| Question | Proposed direction | Why it matters |
| --- | --- | --- |
| How much becomes a Message feature? | Share content, forms, drafts, and activity; keep credentials and session controls in their proper scope. | Reuse must not expose private state or turn every update into model input. |
| File snapshot or live reference? | Default attachments to managed snapshots; make references explicit. | Later edits should not silently change a sent message's meaning. |
| What does live dictation mean initially? | Short phrase updates for batch transcription; streaming when supported. | Goose's transcription request alone cannot promise continuous partial text. |
| Which transcription backend first? | Prefer local when available; make cloud use explicit. Verify packaged support before choosing. | Availability, latency, languages, downloads, and cost need a concrete check. |
| What happens to recorded audio? | Delete temporary audio after transcription/cancellation; retain only by attachment choice. | Dictation and voice messages have different storage/sharing needs. |
| Who answers shared-thread questions? | Name the intended responder; accept one response for agent requests. Decide human multi-response behavior separately. | The first click must not accidentally answer for someone else. |
| When do settings changes apply? | This conversation, next turn by default; save Fiote defaults separately. | Threads should not change one another or silently interrupt work. |
| What cost detail is guaranteed? | Show reported values with scope/currency and mark missing values unavailable. | Context size, billed tokens, subscription quotas, and money differ. |

Next round: resolve these choices, revise tasks and acceptance conditions, then choose the first implementation slice. This document alone does not approve new features.

## Why Goose worked in development but failed as a service

Fiote launches an external agent process. Goose is one such agent: it manages its configured provider/model and agent tools, while Lince communicates through ACP and exposes Lince tools through MCP. Goose is a runtime executable dependency, not embedded in Lince's Rust binary. The direct genai/modelbridge path is separate.

Development shells include Goose in PATH. The plain GitHub packaging step copies Lince, its license, and revision; it does not bundle Goose. Before the previous fix, Nix wrappers added library paths but no Goose executable path. Having Goose installed somewhere on the machine was insufficient for the service.

Read-only inspection on 2026-09-26 confirmed the active user service runs the older package lince-bin-4c32203c7faa. Its process PATH cannot resolve goose, and its installed wrapper contains no Goose path. The system service is inactive. The active user service has ProtectHome=no, so home-directory isolation is not the explanation for this instance.

The previous change adds Goose to runtime PATH for the x86_64 Linux Nix source and prebuilt packages. It takes effect after installing the updated package and restarting; editing this repository does not change an installed Nix store package. Raw archives still require Goose reachable through the service PATH or an explicit executable path. The earlier statement that packaged Lince includes Goose applies to those Nix packages, not every download.

Local references: [service configuration and package wrappers](../../flake.nix), [GitHub archive packaging](../../.github/workflows/build.yml), [executable resolution](../../crates/fiote/src/acp/launch.rs).

## Retained for later discussion

The earlier inventory remains useful; these items are outside this selected round:

- Tool permissions, extensions, skills, recipes, and schedules.
- Session browsing/deletion, steering, import/export, and branching.
- General agent-requested terminals and richer file/diff tool interactions beyond the shared content foundation.
- Local chat-model management, live voice, generated apps, and remote agent transports.
- Direct-model tuning, presets, timeouts, output/tool limits, enforced budgets, sandboxing, compaction, and agent plan editing/approval.

Goose-specific additions use experimental endpoints and need availability checks. [Pinned Goose API schema](https://raw.githubusercontent.com/aaif-goose/goose/v1.52.0/crates/goose/acp-schema.json).
