// The browser half of the Karma sand's rule form, exercised for real.
//
// The Rust tests can only assert that markup contains a string. What they
// cannot see is the part that actually breaks: a rule authored in the form,
// stored, and then read back into the same form has to arrive as the rule its
// author wrote. Everything here is that round trip.
//
// Run: node crates/web/tests/karma_sand_js.mjs

// `state.js` reads the widget host at module scope, so the globals have to
// exist before the import graph is walked.
globalThis.window = { LinceWidgetHost: null };
globalThis.document = { getElementById: () => null, querySelectorAll: () => [] };

const {
  consequencesFrom,
  fillConsequences,
  fillCadence,
  cadenceLabel,
  consequencesLabel,
  conditionFrom,
  fillCondition,
  conditionLabel,
} = await import("../src/sand/karma/app/recurrence.js");
const { state } = await import("../src/sand/karma/app/state.js");

let failures = 0;
function check(name, actual, expected) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a === e) return;
  failures += 1;
  console.error(`FAIL ${name}\n  expected ${e}\n  actual   ${a}`);
}
function ok(name, condition, detail = "") {
  if (condition) return;
  failures += 1;
  console.error(`FAIL ${name} ${detail}`);
}

/** A stand-in for the form: every control the two functions touch. */
function form(values = {}) {
  const field = (v = "") => ({ value: v });
  const elements = {
    ruleCondition: field(""),
    ruleGate: field("!=0"),
    ruleGateValue: field(""),
    ruleCarry: field("value"),
    ruleCarryValue: field(""),
    ruleNumberAction: field("capture-entry"),
    ruleAmount: field(""),
    ruleConcept: field(""),
    ruleConceptAction: field("none"),
    ruleConceptFrom: field(""),
    ruleConceptTo: field(""),
    rulePreset: field("monthly"),
    ruleInvalidDay: field("clamp"),
    ruleBound: field("unbounded"),
    ruleBoundCount: field("1"),
    ruleBoundUntil: field(""),
    landOnBoxes: [
      { value: "monday", checked: false },
      { value: "friday", checked: false },
    ],
    stepYears: field("0"),
    stepMonths: field("0"),
    stepWeeks: field("0"),
    stepDays: field("0"),
    stepHours: field("0"),
    stepMinutes: field("0"),
    stepSeconds: field("0"),
    stepMilliseconds: field("0"),
  };
  for (const [key, value] of Object.entries(values)) elements[key].value = value;
  return elements;
}

// The form speaks concept *names*; a stored consequence carries the *uid* the
// Action resolved. The round trip only holds if the prefill translates back.
state.conceptNames = new Map([
  ["c_WIP", "wip"],
  ["c_DONE", "done"],
  ["c_RENT", "rent"],
]);

// ---------------------------------------------------------------- building

check(
  "a plain capture",
  consequencesFrom(form({ ruleAmount: "-1200" })).consequences,
  [{ kind: "capture-entry", amount: "-1200", concept: null }],
);

check(
  "a capture filed under a concept",
  consequencesFrom(form({ ruleAmount: "-1200", ruleConcept: "@rent" })).consequences,
  [{ kind: "capture-entry", amount: "-1200", concept: "rent" }],
);

check(
  "setting a level is not a delta, and says so in its field name",
  consequencesFrom(form({ ruleNumberAction: "set-quantity", ruleAmount: "-1" })).consequences,
  [{ kind: "set-quantity", value: "-1" }],
);

check(
  "adding is a movement",
  consequencesFrom(form({ ruleNumberAction: "add-quantity", ruleAmount: "5" })).consequences,
  [{ kind: "add-quantity", delta: "5" }],
);

check(
  "a move is the remove-then-add pair, in that order",
  consequencesFrom(
    form({
      ruleNumberAction: "none",
      ruleConceptAction: "move",
      ruleConceptFrom: "@wip",
      ruleConceptTo: "@done",
    }),
  ).consequences,
  [
    { kind: "remove-concept", concept: "wip" },
    { kind: "add-concept", concept: "done" },
  ],
);

check(
  "both halves, number first",
  consequencesFrom(
    form({
      ruleNumberAction: "add-quantity",
      ruleAmount: "1",
      ruleConceptAction: "add",
      ruleConceptTo: "@done",
    }),
  ).consequences,
  [
    { kind: "add-quantity", delta: "1" },
    { kind: "add-concept", concept: "done" },
  ],
);

// ---------------------------------------------------------------- refusing

ok(
  "a rule that does nothing is refused in the form",
  consequencesFrom(form({ ruleNumberAction: "none" })).error,
);
ok(
  "a number action with no amount is refused",
  consequencesFrom(form({ ruleAmount: "  " })).error,
);
ok(
  "a half-named move is refused",
  consequencesFrom(
    form({ ruleNumberAction: "none", ruleConceptAction: "move", ruleConceptFrom: "@wip" }),
  ).error,
);

// ------------------------------------------------------------- round trips

/** Author a rule, store it, read it back, and re-author it unchanged. */
function roundTrip(name, values, stored) {
  const authored = consequencesFrom(form(values)).consequences;
  check(`${name}: authored`, authored, stored.authored);

  // What the backend hands back: concepts as uids.
  const back = form();
  fillConsequences(back, stored.fromServer);
  for (const [field, expected] of Object.entries(stored.expectForm)) {
    check(`${name}: prefill ${field}`, back[field].value, expected);
  }
  // And re-submitting the untouched form must not change the rule.
  const reauthored = consequencesFrom(back).consequences;
  check(`${name}: re-authored unchanged`, reauthored, stored.authored);
}

roundTrip(
  "capture with concept",
  { ruleAmount: "-1200", ruleConcept: "@rent" },
  {
    authored: [{ kind: "capture-entry", amount: "-1200", concept: "rent" }],
    fromServer: [{ kind: "capture-entry", amount: "-1200", concept: "c_RENT" }],
    expectForm: { ruleNumberAction: "capture-entry", ruleAmount: "-1200", ruleConcept: "@rent" },
  },
);

roundTrip(
  "set quantity",
  { ruleNumberAction: "set-quantity", ruleAmount: "-1" },
  {
    authored: [{ kind: "set-quantity", value: "-1" }],
    fromServer: [{ kind: "set-quantity", value: "-1" }],
    expectForm: { ruleNumberAction: "set-quantity", ruleAmount: "-1", ruleConceptAction: "none" },
  },
);

// The one the prefill is most likely to get wrong: a remove and an add have to
// read back as one *move*, not as two unrelated concept consequences.
roundTrip(
  "column move",
  {
    ruleNumberAction: "none",
    ruleConceptAction: "move",
    ruleConceptFrom: "@wip",
    ruleConceptTo: "@done",
  },
  {
    authored: [
      { kind: "remove-concept", concept: "wip" },
      { kind: "add-concept", concept: "done" },
    ],
    fromServer: [
      { kind: "remove-concept", concept: "c_WIP" },
      { kind: "add-concept", concept: "c_DONE" },
    ],
    expectForm: {
      ruleNumberAction: "none",
      ruleConceptAction: "move",
      ruleConceptFrom: "@wip",
      ruleConceptTo: "@done",
    },
  },
);

// ------------------------------------------------------------ the *if* half

check(
  "no reading means unconditional, with no stray gate",
  conditionFrom(form()),
  { condition: null, gate: null, carry: null },
);

check(
  "a bare reading defaults to the oldest behaviour",
  conditionFrom(form({ ruleCondition: "@apples.stock" })),
  { condition: "@apples.stock", gate: "!=0", carry: "value" },
);

check(
  "a comparison joins its operator and bound",
  conditionFrom(
    form({ ruleCondition: "@apples.stock", ruleGate: "<", ruleGateValue: "3" }),
  ),
  { condition: "@apples.stock", gate: "<3", carry: "value" },
);

check(
  "a fixed carry is spelled const:",
  conditionFrom(
    form({ ruleCondition: "@x", ruleCarry: "const", ruleCarryValue: "-1" }),
  ),
  { condition: "@x", gate: "!=0", carry: "const:-1" },
);

ok(
  "a comparison with no bound is refused",
  conditionFrom(form({ ruleCondition: "@x", ruleGate: ">=" })).error,
);
ok(
  "a fixed carry with no number is refused",
  conditionFrom(form({ ruleCondition: "@x", ruleCarry: "const" })).error,
);

{
  // The round trip, including the parse that is easiest to get wrong: `<=`
  // must not read as `<` with a bound of `=3`.
  for (const gate of ["!=0", "always", "<3", "<=3", ">3", ">=3", "==3"]) {
    const back = form();
    fillCondition(back, { condition: "@apples.stock", gate, carry: "const:-1" });
    const rebuilt = conditionFrom(back);
    check(`condition round trip ${gate}`, rebuilt, {
      condition: "@apples.stock",
      gate,
      carry: "const:-1",
    });
  }
  // And an unconditional rule stays unconditional through the form.
  const back = form();
  fillCondition(back, { condition: null, gate: null, carry: null });
  check("unconditional round trip", conditionFrom(back), {
    condition: null,
    gate: null,
    carry: null,
  });
}

check(
  "a condition is spoken as a sentence",
  conditionLabel({ condition: "@apples.stock", gate: "<3" }),
  "only when @apples.stock is under 3",
);
check("an unconditional rule says nothing", conditionLabel({ condition: null }), "");

// ------------------------------------------------------------------ cadence

{
  const back = form();
  fillCadence(back, {
    every: { months: 1, days: 1 },
    land_on: ["friday"],
    invalid_day: "skip",
    bound: { kind: "count", occurrences: 3 },
  });
  check("cadence prefill: months", back.stepMonths.value, "1");
  check("cadence prefill: days", back.stepDays.value, "1");
  check("cadence prefill: untouched component is zeroed", back.stepHours.value, "0");
  check("cadence prefill: short-month policy", back.ruleInvalidDay.value, "skip");
  check("cadence prefill: bound kind", back.ruleBound.value, "count");
  check("cadence prefill: bound count", back.ruleBoundCount.value, "3");
  check(
    "cadence prefill: weekday landing",
    back.landOnBoxes.map((b) => b.checked),
    [false, true],
  );
  // A stored step is never claimed to be a preset: the components are the rule.
  check("cadence prefill: preset does not guess", back.rulePreset.value, "custom");
}

// --------------------------------------------------------------- speaking back

check(
  "a compound cadence reads in largest-first order",
  cadenceLabel({ every: { months: 1, days: 1 }, bound: { kind: "unbounded" } }),
  "repeats every 1 month and 1 day",
);
check(
  "once is a bound of one, not a separate shape",
  cadenceLabel({ every: {}, bound: { kind: "count", occurrences: 1 } }),
  "happens once",
);
check(
  "a move is spoken as both halves",
  consequencesLabel(
    [
      { kind: "remove-concept", concept: "c_WIP" },
      { kind: "add-concept", concept: "c_DONE" },
    ],
    state.conceptNames,
  ),
  "removes @wip and adds @done",
);

if (failures > 0) {
  console.error(`\n${failures} failure(s)`);
  process.exit(1);
}
console.log("karma sand JS: all checks passed");

// ------------------------------------------------- a figure the rule works out

// The user's sentence, in the form: "-1 * freq(@payday)" into set-quantity,
// with no amount typed. The figure is the condition's job, so the consequence
// carries no number of its own and the backend hands it the computed one.
check(
  "a level with no amount takes what the reading works out",
  consequencesFrom(
    form({
      ruleCondition: "-1 * freq(@payday)",
      ruleNumberAction: "set-quantity",
      ruleAmount: "",
    }),
  ).consequences,
  [{ kind: "set-quantity" }],
);

check(
  "and a movement does the same",
  consequencesFrom(
    form({
      ruleCondition: "-1 * freq(@payday)",
      ruleNumberAction: "add-quantity",
      ruleAmount: "",
    }),
  ).consequences,
  [{ kind: "add-quantity" }],
);

check(
  "a written amount still wins over the reading",
  consequencesFrom(
    form({
      ruleCondition: "@apples.stock",
      ruleNumberAction: "set-quantity",
      ruleAmount: "10",
    }),
  ).consequences,
  [{ kind: "set-quantity", value: "10" }],
);

ok(
  "with no reading and no amount there is no figure at all, and that is refused",
  consequencesFrom(form({ ruleNumberAction: "set-quantity", ruleAmount: "" })).error,
);

check(
  "a carried figure is spoken as one",
  consequencesLabel([{ kind: "set-quantity" }], state.conceptNames),
  "sets the quantity to what the reading works out",
);
