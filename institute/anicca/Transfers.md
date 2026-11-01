# Transfers: remaining implementation and validation

Work on the current branch. Play Store publication and Play bundles are outside this work. Use debug APKs only (`signed_release=false`, `play_bundle=false`), preserve the installed release/smoketest apps and their data, and use at most the four disposable participants already prepared.

This file contains unfinished work only. Completed location, audience, background, notification Stop and natural-expiry checks are recorded in `transfer-validation/2026-11-01-acceptance.json`. The complete Android run for `d76535bd` passed; later map/modal changes still need their own final validation. Do not repeat completed measurements unless a relevant change invalidates them.

Follow the groups in order where they depend on each other. Independent CI, native and phone work may run in parallel. Remove each checkbox when its specific result is complete and retain its evidence in the report.

## 1. Known implementation work

### Testing infrastructure

- [ ] Validate and commit the pending `.github/workflows/transfers.yml` step that runs the map storage/navigation contracts. The local map suite already passed 19 tests, including competing local/cache/HTTP source priority.
- [ ] Give `reveal` failures accurate outcomes: distinguish an unchanged viewport, scroll limit or deadline from an actually uncertain ADB input acknowledgement. Current diagnostics conflate these cases.
- [ ] Guard scroll/reveal input against an open IME covering the intended gesture region. Provide an explicit, bounded keyboard-dismiss recovery rather than sending app-scroll gestures into the keyboard.

### Product correctness

No further unimplemented product fix is currently proven. Safe insets, audience scroll extent, Android Back opt-in, viewport clipping, native header contrast and full-map modal scoping have been implemented. Their remaining verification belongs below. If diagnosis reveals another product defect, add one concrete implementation checkbox here before fixing it; do not treat an unverified test failure as an established code defect.

## 2. Diagnosis before another product change

- [ ] Resolve the phone composer scrolling failure with the IME hidden and the actual app viewport identified. The latest capture shows Gboard covering the lower screen; determine whether any remaining failure is product focus/scroll handling or test targeting. Do not assume that the earlier Back press tested the form guard when it may only have dismissed the keyboard.

## 3. Validate the final build

- [ ] Observe the Actions run for the final map/modal revision through completion; repair and repeat only affected failed checks. Run `41330591091` was started for `8783593c`; use the newer revision if product code changes again.
- [ ] Download the final `transfer-regressions`, `android-smoke-evidence` and ARM64 debug APK artifacts, and record their source revision, APK SHA-256, signing certificate and 16 KB alignment.
- [ ] Install the final verified APK on Moto g05 `ZF52555GB7` using `cargo xtask android run --device ZF52555GB7 --apk PATH`. If the debug certificate changes, verify a closed-profile backup before replacing this disposable app and restore its profiles afterward.

The full source check must cover native/mobile checks, migration guard, Transfer/location/map/visibility/editor regressions, backend journeys, both selected Laboratory cases, and Android lint/build/signing/alignment/16 KB smoke. Existing green evidence can satisfy unchanged paths; a host-driver correction can use the standalone smoke workflow with an existing verified APK.

## 4. Finish Transfer/Transport workflow coverage

Use the existing passing native/mobile/backend evidence first. Run an additional case only where it leaves a real gap. Record whether coverage is automated, scripted phone input or physical interaction.

### Composer and persistence

- [ ] Finish explicit Cancel on the phone's full-view ride composer and verify that the local unsaved form is disposed.
- [ ] Verify the application Back guard on an unsaved form after the IME is dismissed; require the guard feedback and preserved draft values.
- [ ] Verify editing an existing Transfer and restoring its saved values after reopening.
- [ ] Identify the passing evidence for preserving a draft's request identity during editing and guarded navigation; add a focused check only if absent.

### Lists, panels and lifecycle

- [ ] Verify that my active transports precede other results.
- [ ] Verify paging in the current Transport list.
- [ ] Verify paging in Transport history.
- [ ] Verify the expanded bottom panel on the phone without losing the retained map.
- [ ] Verify unused unsigned-draft disposal through the appropriate workflow.
- [ ] Verify signed cancellation requires the agreed cancellation workflow rather than unsigned-draft disposal.
- [ ] Identify the passing evidence for camera and list-scroll preservation across panel changes; check any missing path.

### Scope changes

- [ ] Identify the passing evidence for rejecting pending replies after a Person change; check any missing frontend path.
- [ ] Identify the passing evidence for rejecting pending replies after an Organ change; check any missing frontend path.

General Transfers → Transport → Back, real location Stop, and native compact/options captures have already been exercised. Keep their evidence rather than repeating them without cause.

## 5. Finish map validation

Keep this work in a small public area unrelated to the phone's position. Do not use Locate me or publish device coordinates. Map downloads follow the already completed resource measurements.

### Providers and public detail

- [ ] From Transport options, reach the last map/provider action using the final UI; verify that the active controls are accessible and underlying page controls are excluded.
- [ ] Review and save geocoding configuration independently from routing configuration. Verify the correct native editor title before entering values; the earlier old-APK Save attempt opened a folder picker and does not prove provider settings were saved.
- [ ] Center the map on a public test area, such as Reykjavik, without saving that point on an ordinary Record.
- [ ] Verify automatic street-detail loading in an uncached viewport and observe real progress/byte feedback.

### Touch and offline behavior

- [ ] Verify touch panning in the public test viewport.
- [ ] Verify a real or correctly injected two-pointer pinch; button zoom alone is insufficient evidence.
- [ ] Verify cached viewing with external downloads disabled.
- [ ] Verify the honest offline state in a different uncached viewport.
- [ ] Reopen the map and verify persistence of its saved sources and cached data.
- [ ] Save a drawn area for offline use.
- [ ] Cancel an area download without losing previously complete data.
- [ ] Record cancellation/late-result coverage for navigation from the passing backend tests, adding a UI check only if a gap remains.
- [ ] Verify folder-picker cancellation leaves the previous local-pack source unchanged. A real picker was already opened and cancelled; attach unchanged-source evidence or repeat only this small case if needed.

Local-pack priority, competing cache/HTTP precedence, persistent-cache reopening, atomic pack replacement, cancellation and stale navigation results already have passing map tests. Preserve those results instead of recreating the same backend cases on the phone.

## 6. Accessibility and presentation checks

- [ ] Check large text through the Transfer, map and linked audience/location controls, retaining readable controls and access to final actions.
- [ ] Check a long label/query and cancel editing without changing saved data.
- [ ] Check the workflow with Android RTL layout enabled and record any concrete limitation.
- [ ] Verify Android Back on the final map path and its active modal.
- [ ] Verify keyboard editing and focus restoration on the final native-editor path.
- [ ] Change one map theme and verify its controls remain readable.
- [ ] Observe about 30 seconds of settled map idle and record the result.
- [ ] Attach the existing native desktop/430 px compact/options captures and a redacted final phone capture to the durable evidence. Capture again only where a later relevant change invalidates them.

Do not report ADB timings as physical touch latency. Keep coordinates, private addresses, credentials and invitation material out of published evidence.

## 7. Close validation

- [ ] Stop all task-owned sharing, search, download, observer, input-daemon and native-session work.
- [ ] Restore the original global location-enabled state.
- [ ] Verify provider and mock-location state matches the baseline.
- [ ] Restore the disposable debug app's location permissions.
- [ ] Restore the disposable debug app's notification permission.
- [ ] Restore the original text-size setting.
- [ ] Restore the original RTL setting.
- [ ] Restore original brightness and brightness mode.
- [ ] Restore the original screen timeout.
- [ ] Verify charging stay-awake remains at its already restored original value of 15.
- [ ] Restore the intended profile selection and preserve existing apps/data; retain protected debug-profile backups until restoration is verified.
- [ ] Reconcile the redacted report with final source/APK identities, test results, phone outcomes, resource timelines, screenshots and any precise remaining failures.
- [ ] Remove completed checkboxes; if no deliverables remain, delete this plan.
