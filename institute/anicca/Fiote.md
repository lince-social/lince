# Fiote — completion evidence and remaining work

Updated 2026-10-01. This file records implementation evidence and the remaining installed Fiote acceptance workflow. Deferred features stay deferred.

## Agreed behavior already implemented

- Generated UI uses the existing typed components and backend canvas commands. Lince automatically places it inside the known Fiote conversation balloon, with influence immunity in both directions; internal components can still affect one another. Authorized Actions can run immediately. Save keeps a reusable composition, and Close removes its temporary presentation.
- Karma activates a Fiote once per Rule occurrence, carrying the nonzero value and using that Fiote's configured instructions. An active run combines further activations into one pending run, retaining bounded durable cause evidence. Waiting, stop and restart controls are implemented.
- Fiote uses normal Actions for Karma and Transfer CRUD, including unused Transfer draft discard. Optional interaction-to-Fact capture keeps the source user Message and avoids duplicate Facts on retry; a conversation can instead perform other work.
- Context snapshots, linked-child inspection and accessible conversation/status controls are implemented. The earlier feature verification recorded 82 passing tests; the environment-dependent exceptions are listed below.

## Implementation completed in this session

- Every Thread composer, including private conversations, reuses the normal file cards, Attach files, Paste image and dictation controls. Private messages keep literal text and their existing limit; adding files does not enable private Record references or shared mutable forms.
- Normal CreateMessage carries text and files through the existing encrypted private mailbox protocol. Message creation, file chunks/manifest and signed evidence commit together. Retained private metadata stays compact; sending/resending reconstructs bytes and checks the authenticated content hash.
- The existing limit remains 4 MiB decoded content / 16 parts per Message. Private file encoding uses bounded 6 MiB plaintext and 8 MiB encrypted envelopes. Only delivery/inspection/collection have larger frames. Introduction/control limits, provisional consent, operator quotas and the 64 MiB outgoing queue remain enforced; queue accounting uses actual ciphertext size.
- Migration `0331_social_message_attachments.sql` raises the encrypted outbox and trusted mailbox row bounds for content envelopes. It preserves saved queues, destination receipts and foreign keys; introductions, controls and stranger mailbox rows retain the 32 KiB database bound.
- CSV and video retain their correct MIME types. Direct providers receive named UTF-8 text/CSV contents; invalid text and unsupported media refuse the whole request. ACP retains text resources and original PDF/video/image/audio payloads according to advertised capabilities.
- Failed CreateMessage Actions preserve editable drafts/files. Later network refusals preserve the saved Message with its delivery error. Removal/cancel/oversize behavior uses the shared composer, and deliberate resend keeps Message identity and refuses damaged chunks.
- Startup errors now identify their operation/path without weakening private lock permissions or no-follow protection.

## Current verification

| Check | Result |
| --- | --- |
| `cargo check --offline -p lince --features ui` | Passed on final source; includes native media and warning denial |
| Direct-provider text/CSV and refusal tests | 2 passed |
| ACP six-format prompt payload test | 1 passed; protocol fixture, no real model |
| SQLite migration with retained queues, foreign keys and size/consent boundaries | Passed; maximum content accepted, six invalid cases refused |
| Encrypted private Message roundtrip, limits and retry tests | 3 passed; six formats delivered byte-for-byte at the 4 MiB shared limit |
| Native shared composer/MIME/removal/draft tests | 2 passed |
| Private-thread composition, delivery and retained-history regression | 1 passed |
| Ordinary Message attachment order/chunk/missing-data regression | 1 passed |
| Existing encrypted mailbox authentication/refusal/replay/rollback regressions | 6 passed |

All 16 targeted tests passed, plus the SQLite migration checks. Private delivery uses normal CreateMessage Actions, cryptographic peers and in-process mailbox hosts; repeated pickup produces one Message. Synthetic media payloads test transport and integrity, not actual media parsing, playback or model analysis. Real acceptance uses valid files with known contents. Tests used a separate Cargo target after the shared target stayed busy.

## Remaining acceptance, in order

### 1. Run the actual installed application

Normal `lince` launch was retried: exit 1, ReadOnlyFilesystem (OS error 30). The normal home/config directory and `/run/user/1000` are read-only mounts. Connecting independently to both Wayland and the user bus returns Operation not permitted (OS error 1); the bus and tracing also refuse access. This establishes an execution restriction independently of Lince or concurrent coding.

Run in an agent session allowing the normal account's application directories and desktop sockets. Verify the installed executable matches the changes and control its actual UI. If a further application/configuration defect appears, fix its precise path/environment using the existing XDG conventions. Preserve normal data and the Codex account. A substitute home, weaker locks, a headless test or a CLI-only reply does not satisfy this step.

### 2. Codex subscription and Hello

1. Open Lince normally. Click Manage Fiote, select/create a Fiote, then Provider, model and settings / Change provider / sign in.
2. Select an actually available Codex subscription connection. Installed Codex CLI authentication was previously reported as Logged in using ChatGPT; this alone does not prove the selected Lince adapter can use it. Lince's external interface speaks ACP; Codex CLI advertises app-server. Verify the bridge/provider rather than assuming compatibility.
3. Click Check connection · no tokens. Use existing cached authentication through the normal interface. Stop before inference if the existing account cannot authenticate normally; do not copy tokens or substitute an API-key account.
4. Save/load choices. Select GPT-6 Luna / gpt-6-luna, Low reasoning and Fast when actually offered. Record each setting independently; report unavailable controls.
5. Open the conversation, enter Hello and click Send once. Wait for a completed assistant reply, inspect it, then reopen the conversation to verify persistence. A connection check, echo, error or model's claimed identity is not proof of the selected real model.

### 3. Six files through normal Messages

First test a normal human conversation, then Fiote. For each row: Attach files → choose the valid fixture → enter accompanying text → Send → inspect completion/error → reopen Message → Save attachment. Compare downloaded bytes with the original.

| File | Known answer / evidence |
| --- | --- |
| UTF-8 text | Distinctive marker only in the file |
| CSV | Known rows, columns and numeric total |
| PDF | Known fact and its page |
| PNG/JPEG photo | Known visible object/count or receipt amount |
| Audio | Short known spoken phrase; separately Preview / play → Close preview / stop |
| Video | Known first/last events in a short clip |

Record filename, MIME, size, conversation/Message, provider/model/settings, byte comparison, actual reply and outcome: analyzed, model/adapter unsupported, or application/transport failure. Unsupported analysis is acceptable evidence of an attempt; failed ordinary delivery remains an application defect. Preserve each original draft/Message and try the remaining formats. Use a fresh conversation if retained unsupported history prevents later trials; do not rewrite history or quietly omit the file. Do not substitute transcripts, PDF text or video frames and call that original-file analysis.

Also send two different files with text, remove one selected file before sending, cancel selection, reject an oversize file without losing the draft, and check deliberate retries do not duplicate a Message.

### 4. Dictation, playback and wider implemented behavior

Dictate → speak a known phrase → Stop and transcribe → inspect/edit unsent text → Send. Confirm transcription does not send automatically; Cancel creates no Message. Dictation inserts text and does not attach its captured audio. Record speech provider, microphone and playback outcomes independently of Codex file capabilities.

Then verify the previously implemented workflows: protected generated UI with immediate authorized Actions; internal/external influence isolation; Save/Close/reopen/library copies; Karma activation with a carried nonzero value and one pending run; normal Transfer CRUD/discard; optional exact Facts with source-Message evidence and retry deduplication; context/child inspection; screen-reader/keyboard controls and Communication media. Implementation checks do not replace installed/device/provider acceptance.

### 5. Environment-dependent automated cases

Run these previously blocked cases in a session permitting local MCP listeners:

- Cell: `agent_tools_open_without_a_model_provider_and_lock_revokes_them`, `assignment_starts_one_visible_session_and_survives_restart_without_replay`, `mentioning_a_fiote_replies_in_the_record_with_only_recent_context`.
- Desktop: `native_provider_credentials_remain_separate_from_agent_login`.

Run the ignored Cell cases `agent_login_check_and_conversation_workflow` and `agent_question_answers_stay_on_the_question_and_cancel_when_the_turn_stops` with `LINCE_TEST_AGENT_BIN` pointing to `lince-acp-test-agent`; these use a protocol fixture and no model. Run `installed_agent_edits_code_and_record_and_resumes_thread` only with a normally authenticated ACP provider after subscription acceptance. Diagnose any failures that persist with the required access; do not count blocked/ignored tests as passing.
