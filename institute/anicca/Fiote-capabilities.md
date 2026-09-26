# Fiote: remaining capabilities and verification

Reviewed 2026-09-26 against the implementation. Completed tasks have been removed. This list separates unfinished verification, possible additions, and limitations of the tested tools. It does not authorize new implementation.

## Finish verification and installation

- [ ] Install the updated Lince executable and Nix wrapper, restart the user service, and exercise login, connection checks, cancellation, and session resume outside the development shell. The private configuration now shares Lince's pinned prebuilt Goose between the terminal and service, but changing the configuration does not activate it. Its Lince binary input was still pinned to the older executable when last checked; updating the source input alone does not update that binary.
- [ ] Exercise the complete visible workflow in human-only and Fiote threads: mixed attachments, questions, progress, commands, and conversation settings. Automated fixture tests and cargo checks passed; these do not replace a complete installed-app check.
- [ ] Verify real microphone capture, device selection, and final-block dictation with a configured transcription provider. Confirm that the result stays editable and unsent. Incremental transcription is not required.
- [ ] Verify real provider/model combinations for images, audio, and file references, recording which combinations accept them. Connection checks and local fixtures do not prove model acceptance. These tests may use paid APIs; ordinary readiness checks must remain free of generation.
- [ ] Verify login and advertised session features against other installed agents and supported packaging targets. The existing runtime probe covered Goose 1.52.0 on x86_64 Linux.

## Possible additions, in a suggested order

These require further implementation and, for agent features, confirmation that the connected tool exposes the necessary API. Existing permission prompts, resume, terminal login, and message attachments are already implemented; the tasks below extend them.

1. [ ] Add session browsing, deletion, import/export, and branching where supported. Add steering of a running turn only where the agent supports it.
2. [ ] Add management of tool permission policies, extensions, skills, recipes, and schedules where exposed by the agent.
3. [ ] Add general agent-requested terminals and richer file/diff review beyond login terminals and message attachments.
4. [ ] Add direct-provider tuning and presets, configurable timeouts, output/tool limits, and enforced spending budgets. Enforcement needs measured usage and control over dispatch; displaying reported cost is already implemented.
5. [ ] Add explicit compaction controls and agent plan editing/approval where supported. Shared human step lists and display of reported agent plans already exist.
6. [ ] Add a local speech-to-text backend when available, independent of Goose's configured cloud transcription providers. Keep it usable in any thread.
7. [ ] Explore local chat-model management, remote agent connections, live voice, and generated apps as separate features. These need their own backend and interface work; they are not guaranteed by the current adapters.
8. [ ] Define stronger filesystem isolation if wanted. Working and additional directories describe context; they do not enforce a sandbox.

Shared message features must remain usable without Fiote. Credentials and private account state stay outside messages. Dictation produces one final block in an editable, unsent draft.

## Limits of the current tools and adapters

| Capability | Current limit | What would make it possible |
| --- | --- | --- |
| Goose audio prompt attachments | Tested Goose 1.52.0 does not advertise audio prompt input. Shared audio messages and speech-to-text are separate supported paths. | An agent advertising audio input, or a compatible direct-provider adapter/model. |
| Goose additional directories | The tested agent does not advertise this capability. Lince's controls and protocol integration are implemented but gated. | An agent/version advertising support. |
| Goose logout | The tested agent does not advertise protocol logout. Disconnecting is not credential deletion or provider revocation. | Supported agent logout or a separately implemented provider account flow. |
| Local dictation through tested Goose | Its probe advertised no local transcription provider; its advertised providers were unconfigured. | Configure a supported transcription provider, or implement another backend. |
| Universal live settings | Choices depend on what each agent advertises. Direct-provider settings currently live in Fiote setup, not ACP session controls. | Relevant agent support or additional direct-provider conversation controls. |
| Universal rich model input | genai image/PDF support depends on model; the current audio adapter is Gemini-only. modelbridge is text-only. | Compatible models and additional adapter support. |
| Exact cost and quota | Some providers omit cost, currency, token details, or subscription quota. Missing values cannot be treated as zero or exact estimates. | Provider reporting, or clearly labelled estimates with maintained pricing. |
| Complete validation without tokens | Executable, protocol, configuration, and supported readiness checks can run without generation. They cannot prove a future prompt will succeed, quota is sufficient, or a model accepts an attachment. | An actual request, or a provider's authoritative diagnostic endpoint where available. |
| Arbitrary question schemas | Current shared forms support a limited field schema, not every possible nested schema. | More form controls and validation for additional schema types. |
| General audio playback | The implemented attachment playback covers WAV, not every audio codec. | Additional decoding support. |
| File references across machines | A local path does not transfer a file or grant another participant access. | Send a managed snapshot or an accessible shared resource. |
| Automatic Goose availability from raw archives | Raw Lince release archives do not contain Goose. The Nix wrappers provide the pinned executable through PATH. | Install Goose in the service environment or use an explicit executable path. |

These are limits of the tested configuration, not claims that every future version or other agent has the same limits. Goose-specific experimental endpoints must be checked at runtime.

Implementation references: [ACP integration](../../crates/fiote/src/acp.rs), [speech adapter](../../crates/fiote/src/speech.rs), [live controls](../../crates/interface/src/fiote/session/live.rs), [Nix packaging](../../flake.nix).
