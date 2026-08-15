// Which rules run on THIS Cell — C7's second axis.
//
// The first axis, whether a rule is synced, is ordinary Record sync and has no
// control here. This one is per-machine and never travels, so every word on
// this panel is about "here" rather than about the rule: a rule turned off on
// the laptop is still running on the always-on Cell, and calling that "paused"
// or "disabled" would make correct behaviour elsewhere read as a fault.
//
// The list is built from every Program this Cell holds, not from the ones that
// have been configured, because the question the panel answers is "what runs
// here" and a list of exceptions cannot answer it.

import { el, replaceChildren, setHidden } from "./format.js";
import { act, state, setNotice } from "./state.js";

/**
 * This Cell's own uid, as the Program rows report it.
 *
 * Taken from the row rather than held in state: the host answers the query, so
 * the row is the only place the answer is known to be current, and a cached
 * copy would survive a Cell being re-identified.
 */
function programCell(programUid) {
  const row = (state.programs || []).find((program) => program.uid === programUid);
  return row?.this_cell || null;
}

/** Render the per-Cell execution list. */
export function renderExecution(elements) {
  const programs = state.programs || [];
  replaceChildren(elements.executionList, programs.map((program) => row(program)));
  setHidden(elements.executionEmpty, programs.length > 0);
}

function row(program) {
  const runs = program.executes_here !== false;
  const item = el("li", {
    class: runs ? "ruleRow" : "ruleRow ruleRow--dormant",
    "data-program": program.uid,
    "data-runs": runs ? "true" : "false",
  });
  item.append(el("span", { class: "ruleName" }, program.slug || program.uid));
  // The state is words, not only a styled row: "not here" has to survive being
  // read by someone who cannot see the styling, and it is the whole content of
  // this panel.
  item.append(
    el(
      "span",
      { class: "ruleState" },
      runs ? "runs on this Cell" : "held here, runs elsewhere",
    ),
  );
  // The warning belongs on the row that is currently running an outward rule,
  // which is where the choice is made. Discovering in the Ledger that three
  // Cells each scheduled the same Transfer is exactly the failure this line
  // exists to pre-empt — and it says "three Transfers", not "may cause
  // duplicates", because the vague phrasing is what gets ignored.
  // The shared half. Read before the warning, because a rule designated to
  // exactly one Cell cannot act twice and warning about it anyway would be the
  // kind of noise that teaches people to ignore the warning that matters.
  const designated = program.designated_cell || null;
  const designatedHere = designated && designated === program.this_cell;
  if (designated) {
    item.append(
      el(
        "span",
        { class: "ruleState" },
        designatedHere
          ? "this Cell is the designated one"
          : "designated to another Cell",
      ),
    );
  }
  item.append(
    el(
      "button",
      {
        type: "button",
        class: "ghostButton",
        "data-designate": program.uid,
        "data-clear": designated ? "true" : "false",
      },
      designated ? "Let any Cell run it" : "Only this Cell",
    ),
  );
  if (program.externally_observable && runs && !designated) {
    item.append(
      el(
        "span",
        { class: "ruleWarning" },
        "acts outside this Cell — if another Cell also runs it, it acts twice",
      ),
    );
  }
  if (program.execution_note) {
    item.append(el("span", { class: "ruleNote" }, program.execution_note));
  }
  // Offered only when there is something to explain — switching a rule back ON
  // is the default state and needs no reason. It sits BESIDE the button rather
  // than behind a confirm step, so leaving it blank costs nothing: demanding a
  // reason just teaches people to type a space.
  if (runs) {
    item.append(
      el("input", {
        type: "text",
        class: "ruleNoteInput",
        "data-execution-note": program.uid,
        placeholder: "why not here? (optional)",
        autocomplete: "off",
      }),
    );
  }
  const button = el(
    "button",
    {
      type: "button",
      class: "ghostButton",
      "data-execution-toggle": program.uid,
      "data-next": runs ? "off" : "on",
    },
    runs ? "Don't run here" : "Run here",
  );
  item.append(button);
  return item;
}

export function wireExecution(elements) {
  elements.executionList.addEventListener("click", async (event) => {
    const designate = event.target.closest("[data-designate]");
    if (designate) {
      const clearing = designate.getAttribute("data-clear") === "true";
      const programUid = designate.getAttribute("data-designate");
      // Without a uid for this Cell, "Only this Cell" would send a null and
      // CLEAR the designation — a button doing the opposite of its label. Say
      // so instead of acting.
      if (!clearing && !programCell(programUid)) {
        setNotice("This Cell cannot identify itself, so it cannot be designated.");
        return;
      }
      // Designating names THIS Cell: it is the only Cell whose uid this page
      // can be sure of, and picking a Cell you are not sitting at is how a
      // rule ends up designated to a machine that is no longer running.
      // Moving it means going to the Cell you want and pressing it there,
      // which is the manual takeover the design chose over a heartbeat.
      await act({
        action: "designate-karma-executor",
        program_uid: programUid,
        cell_uid: clearing ? null : programCell(programUid),
      });
      return;
    }
    const button = event.target.closest("[data-execution-toggle]");
    if (!button) return;
    const programUid = button.getAttribute("data-execution-toggle");
    const executes = button.getAttribute("data-next") === "on";
    // The note is asked for only when turning a rule OFF, and never demanded.
    // Three Cells with a rule dormant on two of them is a configuration nobody
    // remembers the reason for in a month, but refusing the change without a
    // reason would just teach people to type a space.
    const note = executes
      ? null
      : (elements.executionList
          .querySelector(`[data-execution-note="${CSS.escape(programUid)}"]`)
          ?.value || null);
    const result = await act({
      action: "set-karma-execution",
      program_uid: programUid,
      executes,
      note,
    });
    if (result) setNotice(null);
  });
}
