# Fiote automatic compaction

Native Fiote now rolls its working context over within a running tool-using turn. It does not require another human message, a new conversation, or another startup activation. External ACP agents continue to own their own context handling.

## Codex reference and adapter boundary

The reference is OpenAI Codex `rust-v0.166.0`, commit `6ff1e1ca3b30fc8b914720681fcf5fe62e95f856`. Its [turn loop](https://github.com/openai/codex/blob/6ff1e1ca3b30fc8b914720681fcf5fe62e95f856/codex-rs/core/src/session/turn.rs) checks context after model/tool work, compacts mid-turn and continues. Its compaction installation preserves a replacement working history separately from the durable rollout, and its portable implementation retains user goals with summarized earlier work.

Fiote mirrors that lifecycle, rather than importing Codex's entire session runtime or its private remote V2 transport. The native ChatGPT adapter uses the documented [Responses compaction endpoint](https://developers.openai.com/api/docs/guides/compaction). Its canonical replacement window, including encrypted compaction items, is replayed unchanged. Endpoints explicitly reporting unsupported compaction use the portable summary path. Large attachment windows also use that path so a canonical response cannot exceed the adapter’s JSON response bound; current attachment bytes stay intact. The configured OAuth connection was also checked with a synthetic marker: ordinary Responses succeed, while `/responses/compact` returns `401 hardened_oauth_rule_missing`. That exact route rejection selects portable summarization and is cached for this provider instance. Ordinary invalid-token failures still stop. Authentication, entitlement, malformed-output and general network failures remain visible; they do not silently switch accounts or discard context.

## Pi comparison

Pi was checked at `edbdf8d1cf1d57c97047dc4928bde95ea582135e` in its [compaction reference](https://github.com/earendil-works/pi/blob/edbdf8d1cf1d57c97047dc4928bde95ea582135e/packages/coding-agent/docs/compaction.md) and implementation. It performs ordinary model summarization under harness control, retains recent messages, updates structured memory across repeated compactions, tracks file operations, and separates stored transcripts from the model projection. It can cut within one long user span and recover from overflow with a bounded retry.

Fiote adopts the useful parts of that approach: structured handoff sections, preservation of still-relevant earlier memory and file/test references, a bounded recent suffix at completed tool-group boundaries, and the original current user message. Native replay items stay with their matching assistant/tool group. The ordinary Fiote byte/attachment limits and checkpoint identity rules remain in force; Pi's default token numbers are not copied into a differently budgeted runtime.

## Behavior

- Before another model request, estimate context with the existing byte budget and include instructions and tool descriptions. Compact at 80% instead of waiting for the hard limit. Reported context/token usage can also trigger rollover; where the provider does not report a capacity, the existing byte budget supplies a conservative three-bytes-per-token estimate. This is not an exact tokenizer or a claim of identical thresholds to Codex.
- A provider context-window rejection gets one compaction-and-request retry. No tool is executed by compaction or by that request retry. A rejected oversized remote compaction can use chunked local summaries.
- Complete tool results remain in the native journal. Large results receive a UTF-8-safe, bounded preview only in the working model context. The preview says where the complete receipt remains and asks for smaller source reads when more detail is needed.
- The portable path can summarize completed portions of one active turn, chunk unusually large exchanges, and retain the exact current user message. Complete call/result groups stay together when they fit a chunk. Summaries are historical user context, never new system instructions.
- Instructions and tool permissions remain outside the compacted history and are supplied on subsequent requests. Unresolved tool batches cannot be compacted. Completed call identifiers remain protected across rollover, including prior raw history hidden by a restored checkpoint.
- A durable checkpoint identifies the exact original prefix and its replacement. Editing an earlier message invalidates that checkpoint. An opaque checkpoint from another provider is not applied to an incompatible connection; the original transcript remains available for a new summary.
- Native journals append only newly generated frames. Compaction archives completed frames into private, fingerprint-checked segments and retains a bounded active context. A crash before the journal points to a completed archive does not delete the existing journal. Missing or changed archives are reported explicitly.
- The fixed 32-request and 64-tool ceilings are removed. Work continues until completion, owner cancellation, a genuine provider/storage/error condition, or the existing individual input/output limits. Stopped replies contain only the latest bounded tool status lines; complete results stay in the journal.

## Verification

Regression cases live in `crates/fiote/tests/{conversation,runtime,native}.rs` and the native Cell journal/output modules. They cover 100 calls in one turn, repeated rollover, duplicate identifiers after rollover, owner cancellation beyond the old limit, reported usage, context-overflow recovery, failed persistence, unresolved effects, Unicode results, edited-history invalidation, exact encrypted-window replay, unsupported endpoints, authentication rejection, and segmented receipts beyond the old total journal limit.

Normal package test targets remain usable. For this delivery, focused tests execute the production runtime, conversation, journal, output and ChatGPT adapter source through isolated headless harnesses; the HTTP fixtures use dummy credentials on loopback. The rest of the native application and test targets are checked together with Cargo. A separate synthetic probe uses the production account refresh/store and ChatGPT adapter to verify the actual connection and its available compaction path. It does not activate a Fiote or claim a desktop/reboot demonstration. No personal clipboard, screenshot, live agent activation, desktop launch or reboot is needed for these tests.

Live connection result: the synthetic marker survived portable summary compaction and continuation through the configured ChatGPT account. No tools were called. The private receipt is `/home/user/.local/state/lince/park-development/compaction-ai-check.json`.
