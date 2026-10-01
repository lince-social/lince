# Remaining Karma work

## Native automatic-call validation

- [ ] Finish warnings-denied native checks and run the call/component tests once concurrent social compilation is repaired. `crates/engine/src/social/profile.rs:956` calls `social_store_asset`, which is private in `crates/engine/src/social/media.rs:282` (E0624). This prevents the native test executable from being produced. Do not alter that agent's social work to unblock Karma.

Run `cargo check -p lince-desktop --tests` both with and without `--features native-media`. Run the native-media tests under `communication::calls::tests` and `component_push::tests`; also run the media-disabled component test. Cover admission refusal, repeated pushes, later firings after a call ends, restored components and unavailable media. Restoration alone must not start calls; an explicit later presentation may request the call again.

Backend, state parsing and editor-model checks passed (8 + 3 + 2 tests). Logs are in `.cache-lince-karma/automatic-call-backend.log`, `automatic-call-nucleus.log` and `automatic-call-models.log`. Native diagnostics are in `automatic-call-native-compile.log` and `automatic-call-media-check.log`.

## Deferred ideas

- Manipulating existing board components and broader device/workspace routing.
- More owner-authored Karma Trails and eventual pattern-based suggestions.
