The work left of mobile can be seen by taking the last state before i wrote this line, in a previous commit, my description of the tasks goes as follows:
- [ ] Make karma have a configuration per device that is in sync in an organ so we can sync the karma but only run in one device, so rules don't fire twice, by default only one device of organ/roster can run the karmas and they are synced for editing.
   - AI written wording the human maintained for consultation regarding this task:

   9. **Karma waits for explicit approval after the other agent finishes.** Do not change execution permissions or run Karma tests during the tasks above. Audit the other agent's final implementation first. Finish or verify per-Cell execution permission, default off, managed from the signed Organ roster; display permission separately from locally running/stopped state. Preserve choices across restart and enrolment. Cover scheduled, reactive, manual and queued execution while keeping definition editing and sync available. Test removal, expiry and restart only in this approved phase.

      Use existing rule executor designation and local execution settings. Designate the laptop for each test rule, leave the phone disabled, and verify old work stops when designation changes. A local database claim alone does not coordinate devices; do not add automatic takeover yet.

      Run these checks in order after approval:

      - Cycle one disposable Record through `-1, -2, 0, 1` every five seconds for twenty changes. Observe each value on the unfiltered phone page and verify operations and final state.
      - Set a task to `-1` every minute on the laptop. Set it to zero on the phone and confirm zero on the laptop before the next tick. A later tick may legitimately make it negative again.
      - Test a phone edit at the same time as a tick. Define the interaction of quantity assignment and accumulated Facts first; Loro text merging does not define quantity semantics.
      - Repeat with disconnects, restarts, replay, permission removal and executor changes. Check that the phone never executes a rule while disabled.
