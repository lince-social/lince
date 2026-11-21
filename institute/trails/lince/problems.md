# Clippy problems

Findings are from Sensei’s Clippy, rust-analyzer, and rustfmt cleanup of first-party host targets with default features. They include diagnostics that need review and edits the cleanup skipped. Vendored and protected sources were excluded.

Cargo’s dependency graph identifies packages blocked by Engine’s existing build error. The baseline compiler reported undefined `namespace` and `fds` references in [part_1.rs](../../../crates/engine/src/actions/dispatch/part_1.rs#L22). Those packages have incomplete coverage.
## `crates/ailuros/Cargo.toml`

- `crates/ailuros/src/project.rs:404:16` · `clippy::manual_is_multiple_of` · manual implementation of `.is_multiple_of()`
- `crates/ailuros/src/project.rs:472:24` · `clippy::chunks_exact_to_as_chunks` · using `chunks_exact` with a constant chunk size

## `crates/anicca/Cargo.toml`

- `crates/anicca/src/grammar.rs:31:1` · `clippy::module_inception` · module has the same name as its containing module
- `crates/anicca/src/grammar.rs:174:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:335:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:344:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:454:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:489:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:545:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:709:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:715:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:800:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:817:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:823:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:830:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:852:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:858:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:864:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:871:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:878:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:884:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:891:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:989:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:995:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:1001:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:1007:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:1013:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:1024:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:1030:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:1036:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern
- `crates/anicca/src/grammar.rs:1042:5` · `clippy::manual_non_exhaustive` · this seems like a manual implementation of the non-exhaustive pattern


## `crates/cell/Cargo.toml`

Coverage incomplete. The host build requires Engine and is blocked by its baseline compiler errors.
## `crates/document/Cargo.toml`

- `crates/document/src/lib.rs:129:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/document/src/tests.rs:148:14` · `clippy::chunks_exact_to_as_chunks` · using `chunks_exact` with a constant chunk size
- `crates/document/src/tests.rs:176:14` · `clippy::chunks_exact_to_as_chunks` · using `chunks_exact` with a constant chunk size
- `crates/document/src/tests.rs:184:14` · `clippy::chunks_exact_to_as_chunks` · using `chunks_exact` with a constant chunk size
- `crates/document/src/tests.rs:194:18` · `clippy::chunks_exact_to_as_chunks` · using `chunks_exact` with a constant chunk size
- `crates/document/src/tests.rs:331:14` · `clippy::chunks_exact_to_as_chunks` · using `chunks_exact` with a constant chunk size
- `crates/document/src/tests.rs:383:14` · `clippy::chunks_exact_to_as_chunks` · using `chunks_exact` with a constant chunk size

## `crates/editor/Cargo.toml`

- `crates/editor/src/watch.rs:41:1` · `clippy::items_after_test_module` · items after a test module


## `crates/engine/Cargo.toml`

Coverage incomplete. The host build requires Engine and is blocked by its baseline compiler errors.

## `crates/facade/Cargo.toml`

Coverage incomplete. The host build requires Engine and is blocked by its baseline compiler errors.
## `crates/fiote/Cargo.toml`

- `crates/fiote/examples/speech_fixtures.rs:212:16` · `clippy::unwrap_used` · used `unwrap()` on an `Option` value
- `crates/fiote/examples/speech_fixtures.rs:284:9` · `clippy::unwrap_used` · used `unwrap()` on an `Option` value
- `crates/fiote/examples/speech_fixtures.rs:425:46` · `clippy::unwrap_used` · used `unwrap()` on an `Option` value
- `crates/fiote/examples/speech_fixtures.rs:477:29` · `clippy::chunks_exact_to_as_chunks` · using `chunks_exact` with a constant chunk size
- `crates/fiote/examples/voice_request_fixture.rs:88:20` · `clippy::unwrap_used` · used `unwrap()` on an `Option` value
- `crates/fiote/examples/voice_request_fixture.rs:101:13` · `clippy::unwrap_used` · used `unwrap()` on a `Result` value
- `crates/fiote/examples/voice_request_fixture.rs:130:9` · `clippy::unwrap_used` · used `unwrap()` on a `Result` value
- `crates/fiote/src/communication/acp/launch.rs:21:1` · `clippy::items_after_test_module` · items after a test module
- `crates/fiote/src/communication/acp/options.rs:170:25` · `clippy::unwrap_used` · used `unwrap()` on an `Option` value
- `crates/fiote/src/communication/acp/questions.rs:172:24` · `clippy::unwrap_used` · used `unwrap()` on an `Option` value
- `crates/fiote/src/communication/acp/state.rs:40:17` · `clippy::collapsible_match` · this `if` can be collapsed into the outer `match`
- `crates/fiote/src/communication/acp/terminal.rs:139:45` · `clippy::unwrap_used` · used `unwrap()` on a `Result` value
- `crates/fiote/src/communication/acp/terminal.rs:157:25` · `clippy::unwrap_used` · used `unwrap()` on a `Result` value
- `crates/fiote/src/communication/acp/terminal.rs:166:29` · `clippy::unwrap_used` · used `unwrap()` on a `Result` value
- `crates/fiote/src/communication/acp/terminal.rs:167:31` · `clippy::unwrap_used` · used `unwrap()` on a `Result` value
- `crates/fiote/src/communication/acp/terminal.rs:189:13` · `clippy::unwrap_used` · used `unwrap()` on a `Result` value
- `crates/fiote/src/communication/acp/terminal.rs:205:21` · `clippy::unwrap_used` · used `unwrap()` on a `Result` value
- `crates/fiote/src/communication/acp/terminal.rs:225:21` · `clippy::unwrap_used` · used `unwrap()` on a `Result` value
- `crates/fiote/src/communication/acp/terminal.rs:238:17` · `clippy::unwrap_used` · used `unwrap()` on a `Result` value
- `crates/fiote/src/communication/acp.rs:361:42` · `clippy::unwrap_used` · used `unwrap()` on a `Result` value
- `crates/fiote/src/communication/acp.rs:368:34` · `clippy::unwrap_used` · used `unwrap()` on a `Result` value
- `crates/fiote/src/communication/chatgpt.rs:203:61` · `clippy::manual_range_patterns` · this OR pattern can be rewritten using a range
- `crates/fiote/src/communication/connection.rs:184:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/fiote/src/communication/speech/moonshine.rs:121:16` · `clippy::manual_is_multiple_of` · manual implementation of `.is_multiple_of()`
- `crates/fiote/src/communication/speech/moonshine.rs:501:29` · `clippy::chunks_exact_to_as_chunks` · using `chunks_exact_mut` with a constant chunk size
- `crates/fiote/src/communication/speech/segment.rs:132:35` · `clippy::unwrap_used` · used `unwrap()` on an `Option` value
- `crates/fiote/src/communication/speech/voice.rs:159:26` · `clippy::unwrap_used` · used `unwrap()` on an `Option` value
- `crates/fiote/src/communication/speech/voice.rs:362:17` · `clippy::unwrap_used` · used `unwrap()` on an `Option` value
- `crates/fiote/src/communication/speech/wake.rs:86:26` · `clippy::unwrap_used` · used `unwrap()` on an `Option` value
- `crates/fiote/src/communication/speech/wake.rs:199:23` · `clippy::unwrap_used` · used `unwrap()` on an `Option` value
- `crates/fiote/src/config.rs:75:35` · `clippy::manual_range_contains` · manual `!RangeInclusive::contains` implementation
- `crates/fiote/tests/conversation.rs:205:9` · `clippy::await_holding_lock` · this `MutexGuard` is held across an await point

## `crates/hand-input/Cargo.toml`

- `crates/hand-input/src/lib.rs:212:69` · `clippy::chunks_exact_to_as_chunks` · using `chunks_exact` with a constant chunk size
- `crates/hand-input/src/lib.rs:242:75` · `clippy::chunks_exact_to_as_chunks` · using `chunks_exact` with a constant chunk size

## `crates/interface/Cargo.toml`

Coverage incomplete. Baseline failed. cargo check exited with exit status: 101. The runtime flavor `multi_thread` requires the `rt-multi-thread` feature.


## `crates/lince/Cargo.toml`

Coverage incomplete. The host build requires Engine and is blocked by its baseline compiler errors.
## `crates/map/Cargo.toml`

- `crates/map/src/source.rs:280:16` · `clippy::type_complexity` · very complex type used. Consider factoring parts into `type` definitions


## `crates/mobile/Cargo.toml`

Coverage incomplete. The host build requires Engine and is blocked by its baseline compiler errors.
## `crates/nucleus/Cargo.toml`

- `crates/nucleus/src/canvas/room/tests.rs:641:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/nucleus/src/canvas.rs:666:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/nucleus/src/canvas.rs:744:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/nucleus/src/canvas.rs:963:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/nucleus/src/demeter/meal.rs:496:31` · `clippy::filter_map_bool_then` · usage of `bool::then` in `filter_map`
- `crates/nucleus/src/demeter/production.rs:195:1` · `clippy::too_many_arguments` · this function has too many arguments (9/7)
- `crates/nucleus/src/drawing.rs:108:37` · `clippy::useless_vec` · useless use of `vec!`
- `crates/nucleus/src/execution/causal.rs:143:5` · `clippy::result_unit_err` · this returns a `Result<_, ()>`
- `crates/nucleus/src/execution/causal.rs:162:5` · `clippy::result_unit_err` · this returns a `Result<_, ()>`
- `crates/nucleus/src/execution/causal.rs:216:5` · `clippy::result_unit_err` · this returns a `Result<_, ()>`
- `crates/nucleus/src/graph.rs:90:14` · `clippy::needless_range_loop` · the loop variable `i` is used to index `done`
- `crates/nucleus/src/karma/cadence.rs:321:9` · `clippy::explicit_counter_loop` · the variable `index` is used as a loop counter
- `crates/nucleus/src/karma/dispatcher.rs:1494:9` · `clippy::while_let_loop` · this loop could be written as a `while let` loop
- `crates/nucleus/src/karma/dsl.rs:868:21` · `clippy::needless_bool_assign` · this if-then-else expression assigns a bool literal
- `crates/nucleus/src/karma/dsl.rs:2535:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/nucleus/src/karma/execution.rs:376:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/nucleus/src/karma/frequency.rs:218:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/nucleus/src/karma/replay.rs:95:6` · `clippy::result_large_err` · the `Err`-variant returned from this function is very large
- `crates/nucleus/src/karma/replay.rs:129:26` · `clippy::result_large_err` · the `Err`-variant returned from this function is very large
- `crates/nucleus/src/karma/replay.rs:143:40` · `clippy::result_large_err` · the `Err`-variant returned from this function is very large
- `crates/nucleus/src/karma/replay.rs:214:63` · `clippy::result_large_err` · the `Err`-variant returned from this function is very large
- `crates/nucleus/src/karma/simple_frequency.rs:84:28` · `clippy::clone_on_copy` · using `clone` on type `CadenceBound` which implements the `Copy` trait
- `crates/nucleus/src/plugins.rs:265:21` · `clippy::collapsible_match` · this `if` can be collapsed into the outer `match`
- `crates/nucleus/src/sand_package.rs:224:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/nucleus/src/shader_effect/bindings.rs:265:9` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/nucleus/src/shader_effect/spatial.rs:433:9` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/nucleus/src/social.rs:350:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/nucleus/src/social.rs:699:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/nucleus/src/transfer/receipt.rs:1224:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/nucleus/src/transfer/receipt_delivery.rs:72:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/nucleus/src/trust_analysis/claims/commands.rs:71:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/nucleus/src/trust_analysis/claims/recipes/commands.rs:59:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/nucleus/src/trust_analysis/commands.rs:79:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/nucleus/src/trust_analysis/remote.rs:211:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/nucleus/src/trust_analysis/serving.rs:70:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/nucleus/src/trust_analysis/serving.rs:332:16` · `clippy::manual_is_multiple_of` · manual implementation of `.is_multiple_of()`
- `crates/nucleus/src/visibility/plan.rs:152:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/nucleus/tests/demeter_production.rs:213:9` · `clippy::cloned_ref_to_slice_refs` · unnecessary use of `clone` to create a slice from a reference
- `crates/nucleus/tests/demeter_production.rs:237:13` · `clippy::cloned_ref_to_slice_refs` · unnecessary use of `clone` to create a slice from a reference
- `crates/nucleus/tests/demeter_production.rs:250:9` · `clippy::cloned_ref_to_slice_refs` · unnecessary use of `clone` to create a slice from a reference
- `crates/nucleus/tests/karma_k0.rs:61:27` · `clippy::useless_format` · useless use of `format!`

## `crates/park/Cargo.toml`

- `crates/park/src/launch.rs:218:29` · `clippy::unnecessary_get_then_check` · unnecessary use of `get(OsStr::new("LD_LIBRARY_PATH")).is_none()`

## `crates/plugin/Cargo.toml`

- `crates/plugin/src/lib.rs:455:12` · `clippy::comparison_to_empty` · comparison to empty slice


## `crates/plugins/plugin-activitypub/Cargo.toml`

Coverage incomplete. The host build requires Engine and is blocked by its baseline compiler errors.

## `crates/plugins/plugin-files/Cargo.toml`

Coverage incomplete. The host build requires Engine and is blocked by its baseline compiler errors.

## `crates/plugins/plugin-irc/Cargo.toml`

Coverage incomplete. The host build requires Engine and is blocked by its baseline compiler errors.
## `crates/plugins/plugin-nomad/Cargo.toml`

Coverage incomplete. Baseline failed. cargo check exited with exit status: 101. cannot update the lock file /home/user/git/lince-social/lince/crates/plugins/plugin-nomad/Cargo.lock because --locked was passed to prevent this


## `crates/plugins/plugin-nostr/Cargo.toml`

Coverage incomplete. The host build requires Engine and is blocked by its baseline compiler errors.
## `crates/protein-authority/Cargo.toml`

- `crates/protein-authority/tests/wire.rs:9:20` · `clippy::unwrap_used` · used `unwrap()` on a `Result` value
- `crates/protein-authority/tests/wire.rs:10:19` · `clippy::unwrap_used` · used `unwrap()` on a `Result` value
- `crates/protein-authority/tests/wire.rs:11:5` · `clippy::unwrap_used` · used `unwrap()` on a `Result` value


## `crates/protein/Cargo.toml`

Coverage incomplete. The host build requires Engine and is blocked by its baseline compiler errors.

## `crates/sandbox/Cargo.toml`

Coverage incomplete. The host build requires Engine and is blocked by its baseline compiler errors.

## `crates/simulation/Cargo.toml`

Coverage incomplete. The host build requires Engine and is blocked by its baseline compiler errors.
## `crates/store/Cargo.toml`

- `crates/store/src/action_intents.rs:243:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/assertions.rs:32:1` · `clippy::too_many_arguments` · this function has too many arguments (8/7)
- `crates/store/src/concepts.rs:331:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/concepts.rs:360:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/config.rs:171:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/config.rs:180:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/config.rs:204:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/door.rs:94:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/evidence_custody.rs:64:1` · `clippy::too_many_arguments` · this function has too many arguments (8/7)
- `crates/store/src/evidence_forwarding.rs:56:13` · `clippy::type_complexity` · very complex type used. Consider factoring parts into `type` definitions
- `crates/store/src/karma/frequencies.rs:1075:5` · `clippy::collapsible_if` · this `if` statement can be collapsed
- `crates/store/src/karma/frequencies.rs:1367:1` · `clippy::too_many_arguments` · this function has too many arguments (8/7)
- `crates/store/src/karma/programs.rs:484:1` · `clippy::too_many_arguments` · this function has too many arguments (9/7)
- `crates/store/src/karma/programs.rs:779:1` · `clippy::too_many_arguments` · this function has too many arguments (8/7)
- `crates/store/src/karma/schedules.rs:149:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/store/src/karma/schedules.rs:274:1` · `clippy::large_enum_variant` · large size difference between variants
- `crates/store/src/karma/schedules.rs:310:1` · `clippy::too_many_arguments` · this function has too many arguments (9/7)
- `crates/store/src/karma/schedules.rs:336:1` · `clippy::too_many_arguments` · this function has too many arguments (10/7)
- `crates/store/src/karma_stages.rs:38:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/ledger.rs:113:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/profile_claims.rs:44:132` · `clippy::type_complexity` · very complex type used. Consider factoring parts into `type` definitions
- `crates/store/src/projection.rs:384:1` · `clippy::too_many_arguments` · this function has too many arguments (8/7)
- `crates/store/src/receipt_events.rs:52:14` · `clippy::type_complexity` · very complex type used. Consider factoring parts into `type` definitions
- `crates/store/src/receipt_references.rs:47:14` · `clippy::type_complexity` · very complex type used. Consider factoring parts into `type` definitions
- `crates/store/src/record_docs.rs:552:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/records.rs:445:11` · `clippy::needless_borrows_for_generic_args` · the borrowed expression implements the required traits
- `crates/store/src/records.rs:456:11` · `clippy::needless_borrows_for_generic_args` · the borrowed expression implements the required traits
- `crates/store/src/records.rs:477:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/records.rs:640:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/replica.rs:98:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/replica.rs:115:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/roster.rs:146:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/social.rs:85:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/sync_ops.rs:110:1` · `clippy::too_many_arguments` · this function has too many arguments (11/7)
- `crates/store/src/sync_ops.rs:399:24` · `clippy::explicit_auto_deref` · deref which would be done by auto-deref
- `crates/store/src/transfer_balances.rs:656:1` · `clippy::too_many_arguments` · this function has too many arguments (9/7)
- `crates/store/src/transfer_delivery.rs:689:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/transfer_delivery.rs:978:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/transfer_delivery.rs:1067:1` · `clippy::too_many_arguments` · this function has too many arguments (9/7)
- `crates/store/src/transfer_delivery.rs:1100:16` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/transfer_delivery.rs:1142:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/transfer_delivery.rs:1795:1` · `clippy::too_many_arguments` · this function has too many arguments (8/7)
- `crates/store/src/transfer_delivery.rs:2049:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/transfer_delivery.rs:2207:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/transfer_delivery.rs:2841:1` · `clippy::too_many_arguments` · this function has too many arguments (11/7)
- `crates/store/src/transfer_effects.rs:246:1` · `clippy::too_many_arguments` · this function has too many arguments (8/7)
- `crates/store/src/transfers.rs:356:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/transfers.rs:559:1` · `clippy::too_many_arguments` · this function has too many arguments (9/7)
- `crates/store/src/transfers.rs:567:19` · `clippy::type_complexity` · very complex type used. Consider factoring parts into `type` definitions
- `crates/store/src/transfers.rs:3461:1` · `clippy::too_many_arguments` · this function has too many arguments (9/7)
- `crates/store/src/transfers.rs:5363:44` · `clippy::explicit_auto_deref` · deref which would be done by auto-deref
- `crates/store/src/vocabulary/features.rs:113:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/src/vocabulary/preferences.rs:105:5` · `clippy::needless_question_mark` · enclosing `Ok` and `?` operator are unneeded
- `crates/store/tests/access_snapshot.rs:225:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/store/tests/access_snapshot.rs:707:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/store/tests/access_snapshot.rs:710:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/store/tests/access_snapshot.rs:713:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/store/tests/access_snapshot.rs:716:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/store/tests/access_snapshot.rs:719:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/store/tests/access_snapshot.rs:722:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/store/tests/access_snapshot.rs:725:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/store/tests/access_snapshot.rs:728:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/store/tests/access_snapshot.rs:731:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/store/tests/access_snapshot.rs:734:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/store/tests/access_snapshot.rs:737:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/store/tests/access_snapshot.rs:740:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/store/tests/access_snapshot.rs:753:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/store/tests/access_snapshot.rs:762:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/store/tests/access_snapshot.rs:771:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/store/tests/access_snapshot.rs:780:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/store/tests/communication.rs:201:15` · `clippy::needless_borrows_for_generic_args` · the borrowed expression implements the required traits
- `crates/store/tests/communication.rs:234:5` · `clippy::field_reassign_with_default` · field assignment outside of initializer for an instance created with Default::default()
- `crates/store/tests/concepts.rs:147:70` · `clippy::cloned_ref_to_slice_refs` · unnecessary use of `clone` to create a slice from a reference
- `crates/store/tests/revision_migration_edges.rs:36:5` · `clippy::needless_late_init` · unneeded late initialization
- `crates/store/tests/seed.rs:25:5` · `clippy::bool_assert_comparison` · used `assert_eq!` with a literal bool
- `crates/store/tests/trust_queries.rs:109:9` · `clippy::explicit_auto_deref` · deref which would be done by auto-deref
- `crates/store/tests/trust_queries.rs:203:25` · `clippy::explicit_auto_deref` · deref which would be done by auto-deref


## `crates/transport/Cargo.toml`

Coverage incomplete. The host build requires Engine and is blocked by its baseline compiler errors.
## `crates/utils/Cargo.toml`

- `crates/utils/src/self_update.rs:184:28` · `clippy::manual_is_multiple_of` · manual implementation of `.is_multiple_of()`
- `crates/utils/src/self_update.rs:324:1` · `clippy::items_after_test_module` · items after a test module

## `crates/xtask/Cargo.toml`

- `crates/xtask/src/android_smoke.rs:303:21` · `clippy::collapsible_if` · this `if` statement can be collapsed
- `crates/xtask/src/content.rs:62:5` · `clippy::single_element_loop` · for loop over a single element
- `crates/xtask/src/main.rs:667:18` · `clippy::ptr_arg` · writing `&PathBuf` instead of `&Path` involves a new object where a slice will do
- `crates/xtask/src/maintenance/fiote.rs:265:16` · `clippy::manual_is_multiple_of` · manual implementation of `.is_multiple_of()`
- `crates/xtask/src/maintenance/fiote.rs:571:33` · `clippy::chunks_exact_to_as_chunks` · using `chunks_exact` with a constant chunk size
- `crates/xtask/src/park/validation/attribution.rs:588:32` · `clippy::chunks_exact_to_as_chunks` · using `chunks_exact` with a constant chunk size
- `crates/xtask/src/park/validation/automation.rs:74:18` · `clippy::double_ended_iterator_last` · called `Iterator::last` on a `DoubleEndedIterator`; this will needlessly iterate the entire iterator
- `crates/xtask/src/park/validation/final_suite/scenarios.rs:201:5` · `clippy::double_ended_iterator_last` · called `Iterator::last` on a `DoubleEndedIterator`; this will needlessly iterate the entire iterator
- `crates/xtask/src/park/validation/final_suite/scenarios.rs:614:18` · `clippy::double_ended_iterator_last` · called `Iterator::last` on a `DoubleEndedIterator`; this will needlessly iterate the entire iterator
- `crates/xtask/src/park/validation/observe.rs:192:32` · `clippy::chunks_exact_to_as_chunks` · using `chunks_exact` with a constant chunk size
- `crates/xtask/src/park/validation.rs:610:18` · `clippy::unwrap_or_default` · use of `or_insert_with` to construct default value
- `crates/xtask/src/park/validation.rs:1196:18` · `clippy::double_ended_iterator_last` · called `Iterator::last` on a `DoubleEndedIterator`; this will needlessly iterate the entire iterator
- `crates/xtask/src/park/validation.rs:3240:22` · `clippy::double_ended_iterator_last` · called `Iterator::last` on a `DoubleEndedIterator`; this will needlessly iterate the entire iterator
- `crates/xtask/src/park/validation.rs:3274:9` · `clippy::double_ended_iterator_last` · called `Iterator::last` on a `DoubleEndedIterator`; this will needlessly iterate the entire iterator
- `crates/xtask/src/workspace/fixtures.rs:644:1` · `clippy::too_many_arguments` · this function has too many arguments (8/7)


214 findings across 23 verified packages. 2 additional package passes failed, and 13 were blocked by Engine.
