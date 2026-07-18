import { emptyDraft, emptyPromise, toCreateTransferAction, validateDraftStep } from "./create-model.js";

const STEPS = ["Terms", "People", "Promises", "Sharing", "Review"];

export function createTransferComposer(root, { host, onCreated, onClosed }) {
  let step = 0;
  let open = false;
  let busy = false;
  let awaiting = false;
  let error = "";
  let people = [];
  let records = [];
  let transfers = [];
  let draft = emptyDraft();

  function setOptions(next = {}) {
    people = normalizeOptions(next.people);
    records = normalizeOptions(next.records);
    transfers = normalizeOptions(next.transfers);
    if (open) render();
  }

  function show() {
    draft = emptyDraft();
    step = 0;
    error = "";
    busy = false;
    awaiting = false;
    open = true;
    root.hidden = false;
    render();
    requestAnimationFrame(() => root.querySelector("input, select, button")?.focus());
  }

  function close() {
    if (busy || awaiting) return;
    open = false;
    root.hidden = true;
    root.replaceChildren();
    onClosed?.();
  }

  function setAwaitingLive(value) {
    awaiting = Boolean(value);
    if (open) render();
  }

  function complete() {
    awaiting = false;
    busy = false;
    open = false;
    root.hidden = true;
    root.replaceChildren();
  }

  function render() {
    const shell = el("div", "creatorShell");
    const header = el("header", "creatorHeader");
    const identity = el("div", "creatorIdentity");
    identity.append(el("div", "eyebrow", "Manual transfer"), el("h2", "", "New transfer"));
    const dismiss = button("×", "iconButton", close);
    dismiss.setAttribute("aria-label", "Close transfer creator");
    dismiss.disabled = busy || awaiting;
    header.append(identity, dismiss);

    const progress = el("ol", "stepper");
    for (const [index, label] of STEPS.entries()) {
      const item = el("li", "stepItem", label);
      item.dataset.current = String(index === step);
      item.dataset.complete = String(index < step);
      progress.append(item);
    }

    const content = el("form", "creatorForm");
    content.addEventListener("submit", (event) => {
      event.preventDefault();
      next();
    });
    if (step === 0) renderTerms(content);
    if (step === 1) renderPeople(content);
    if (step === 2) renderPromises(content);
    if (step === 3) renderSharing(content);
    if (step === 4) renderReview(content);

    if (error) {
      const alert = el("div", "formAlert", error);
      alert.setAttribute("role", "alert");
      content.prepend(alert);
    }

    const footer = el("footer", "creatorFooter");
    const back = button("Back", "secondaryButton", () => {
      error = "";
      step -= 1;
      render();
    });
    back.disabled = step === 0 || busy || awaiting;
    const nextLabel = step === STEPS.length - 1 ? "Create transfer" : "Continue";
    const forward = button(awaiting ? "Waiting for live transfer" : busy ? "Creating transfer" : nextLabel, "primaryButton", next);
    forward.disabled = busy || awaiting;
    footer.append(back, forward);
    shell.append(header, progress, content, footer);
    root.replaceChildren(shell);
  }

  function renderTerms(form) {
    form.append(sectionHeading("Terms", "Name the commitment and choose how agreement is reached."));
    const grid = el("div", "formGrid");
    grid.append(
      field("Title", input("text", draft.head, (value) => { draft.head = value; }), { required: true, wide: true }),
      field("Slug", input("text", draft.slug, (value) => { draft.slug = value; }), { hint: "Optional stable reference" }),
      field("Agreement", select([
        ["full", "Everyone"],
        ["individual", "Individual"],
        ["percentage", "Percentage"],
        ["dependency", "Dependency order"],
      ], draft.agreement, (value) => { draft.agreement = value; render(); })),
    );
    if (draft.agreement === "percentage") {
      grid.append(field("Required percentage", numberInput(draft.agreementPct, 1, 100, (value) => { draft.agreementPct = value; }), { suffix: "%" }));
    }
    grid.append(
      field("Sibling completion", select([
        ["none", "Independent"],
        ["first_completes", "First sibling completes"],
      ], draft.satiation, (value) => { draft.satiation = value; })),
      field("Reserve quantities from", select([
        ["none", "Never"],
        ["proposed", "Proposal"],
        ["agreed", "Agreement"],
        ["active", "Activation"],
      ], draft.reserveDefault, (value) => { draft.reserveDefault = value; })),
    );
    form.append(grid);
    form.append(checkField("Require delivery and receipt confirmations", draft.requireConfirmation, (checked) => { draft.requireConfirmation = checked; }));
  }

  function renderPeople(form) {
    form.append(sectionHeading("People", "Every party is a Person record and can agree independently."));
    const list = el("div", "composerList");
    for (const [index, uid] of draft.parties.entries()) {
      const row = el("div", "composerRow partyComposerRow");
      row.append(optionSelect(people, uid, "Select person", (value) => { draft.parties[index] = value; }));
      row.append(removeButton(() => { draft.parties.splice(index, 1); render(); }, "Remove person"));
      list.append(row);
    }
    if (!draft.parties.length) list.append(emptyComposer("No parties added"));
    form.append(list);
    const add = button("+ Add person", "secondaryButton", () => {
      draft.parties.push(firstUnused(people, draft.parties));
      render();
    });
    add.disabled = !people.length || draft.parties.length >= people.length;
    form.append(add);
    if (!people.length) form.append(note("Create Person records before composing a transfer."));
  }

  function renderPromises(form) {
    form.append(sectionHeading("Promises", "Describe what each person gives or receives and when."));
    const list = el("div", "promiseComposerList");
    for (const [index, promise] of draft.promises.entries()) {
      const card = el("fieldset", "promiseComposer");
      const legend = el("legend", "", `Promise ${index + 1}`);
      const remove = removeButton(() => { draft.promises.splice(index, 1); render(); }, `Remove promise ${index + 1}`);
      remove.classList.add("promiseRemove");
      const grid = el("div", "formGrid");
      grid.append(
        field("Person", optionSelect(selectedPeople(), promise.party, "Select party", (value) => { promise.party = value; }), { required: true }),
        field("Direction", select([["gives", "Gives"], ["receives", "Receives"]], promise.direction, (value) => { promise.direction = value; })),
        field("Record", optionSelect(records, promise.record, "Select record", (value) => { promise.record = value; }), { required: true }),
        field("Quantity", numberInput(promise.quantity, 0.001, null, (value) => { promise.quantity = value; }), { required: true }),
        field("Deadline", input("datetime-local", promise.windowEnd, (value) => { promise.windowEnd = value; })),
        field("Reserve from", select([
          ["", "Transfer default"], ["none", "Never"], ["proposed", "Proposal"], ["agreed", "Agreement"], ["active", "Activation"],
        ], promise.reserveFrom, (value) => { promise.reserveFrom = value; })),
        field("Condition", input("text", promise.condition, (value) => { promise.condition = value; }), { hint: "Optional formula or prerequisite", wide: true }),
      );
      card.append(legend, remove, grid);
      list.append(card);
    }
    if (!draft.promises.length) list.append(emptyComposer("No promises added"));
    form.append(list);
    const add = button("+ Add promise", "secondaryButton", () => {
      draft.promises.push(emptyPromise(draft.parties[0] || ""));
      render();
    });
    add.disabled = !draft.parties.some(Boolean) || !records.length;
    form.append(add);
  }

  function renderSharing(form) {
    form.append(sectionHeading("Sharing and hierarchy", "Transfers start hidden unless you explicitly publish them."));
    const grid = el("div", "formGrid");
    grid.append(field("Visibility", select([
      ["hidden", "Hidden"], ["public", "Public"], ["proximity", "Proximity"],
    ], draft.visibility, (value) => { draft.visibility = value; render(); })));
    if (draft.visibility === "proximity") {
      grid.append(field("Maximum proximity", numberInput(draft.maxProximity, 1, null, (value) => { draft.maxProximity = value; }), { hint: "Whole-number organ distance" }));
    }
    grid.append(
      field("Parent transfer", optionSelect(transfers, draft.parent, "No parent", (value) => { draft.parent = value; }), { hint: "Place this transfer in a larger tree" }),
      field("Source record", optionSelect(records, draft.source, "No source", (value) => { draft.source = value; }), { hint: "Record that originated this transfer" }),
    );
    form.append(grid);
  }

  function renderReview(form) {
    form.append(sectionHeading("Review", "These terms are submitted together as one transfer draft."));
    const facts = el("dl", "reviewFacts");
    facts.append(
      reviewFact("Title", draft.head),
      reviewFact("Agreement", draft.agreement === "percentage" ? `${draft.agreementPct}%` : labelValue(draft.agreement)),
      reviewFact("Visibility", draft.visibility === "proximity" ? `Proximity ${draft.maxProximity}` : labelValue(draft.visibility)),
      reviewFact("Confirmations", draft.requireConfirmation ? "Delivery and receipt" : "Not required"),
      reviewFact("Hierarchy", optionName(transfers, draft.parent) || "Top level"),
      reviewFact("Source", optionName(records, draft.source) || "None"),
    );
    form.append(facts);

    const terms = el("div", "reviewTerms");
    for (const promise of draft.promises) {
      const quantity = Number(promise.quantity);
      const publicDelta = promise.direction === "gives" ? -quantity : quantity;
      const privateDelta = publicDelta;
      const row = el("article", "reviewTerm");
      const publicTerm = el("div", "reviewTermMain");
      publicTerm.append(
        el("strong", "", `${optionName(people, promise.party)} ${promise.direction}`),
        el("span", "", `${formatNumber(quantity)} ${optionName(records, promise.record)}`),
      );
      const deltas = el("dl", "reviewDeltas");
      deltas.append(
        reviewFact("Public occurrence", signed(publicDelta)),
        reviewFact("Private quantity effect", signed(privateDelta)),
      );
      row.append(publicTerm, deltas);
      if (promise.windowEnd || promise.condition) {
        row.append(el("div", "reviewMeta", [promise.windowEnd && formatDateTime(promise.windowEnd), promise.condition].filter(Boolean).join(" · ")));
      }
      terms.append(row);
    }
    form.append(terms);
    form.append(note("The current private Record effect matches the signed transfer delta. No private application formula is applied in this phase."));
    if (awaiting) form.append(note("The Actions were accepted. This view will open the transfer after its live projection arrives.", "success"));
  }

  function next() {
    if (busy || awaiting) return;
    error = validateDraftStep(draft, step);
    if (error) { render(); return; }
    if (step < STEPS.length - 1) {
      step += 1;
      render();
      requestAnimationFrame(() => root.querySelector("input, select, button")?.focus());
      return;
    }
    submit();
  }

  async function submit() {
    error = validateAll();
    if (error) { render(); return; }
    if (typeof host?.act !== "function") {
      error = "The Action bridge is unavailable.";
      render();
      return;
    }
    busy = true;
    render();
    try {
      const result = await host.act(toCreateTransferAction(draft));
      const uid = String(result?.created || "");
      if (!uid) throw new Error("The transfer was created without an identifier.");
      busy = false;
      awaiting = true;
      render();
      onCreated?.(uid);
    } catch (cause) {
      busy = false;
      error = cause instanceof Error ? cause.message : "Transfer creation failed.";
      render();
    }
  }

  function validateAll() {
    for (let index = 0; index < STEPS.length - 1; index += 1) {
      const message = validateDraftStep(draft, index);
      if (message) return message;
    }
    return "";
  }

  function selectedPeople() {
    const ids = new Set(draft.parties.filter(Boolean));
    return people.filter((person) => ids.has(person.uid));
  }

  return { show, close, complete, setAwaitingLive, setOptions };
}

function normalizeOptions(options) {
  return (Array.isArray(options) ? options : [])
    .map((option) => ({ uid: String(option?.uid || ""), head: String(option?.head || option?.slug || option?.uid || "Unnamed"), slug: String(option?.slug || "") }))
    .filter((option) => option.uid)
    .sort((left, right) => left.head.localeCompare(right.head));
}

function optionSelect(options, selected, placeholder, onChange) {
  const node = el("select", "");
  node.append(new Option(placeholder, ""));
  for (const option of options) node.append(new Option(option.slug ? `${option.head} · ${option.slug}` : option.head, option.uid));
  node.value = selected;
  node.addEventListener("change", () => onChange(node.value));
  return node;
}

function select(options, selected, onChange) {
  const node = el("select", "");
  for (const [value, label] of options) node.append(new Option(label, value));
  node.value = selected;
  node.addEventListener("change", () => onChange(node.value));
  return node;
}

function input(type, value, onInput) {
  const node = el("input", "");
  node.type = type;
  node.value = value ?? "";
  node.addEventListener("input", () => onInput(node.value));
  return node;
}

function numberInput(value, min, max, onInput) {
  const node = input("number", value, onInput);
  node.step = min && min < 1 ? "any" : "1";
  if (min != null) node.min = String(min);
  if (max != null) node.max = String(max);
  return node;
}

function field(label, control, options = {}) {
  const wrapper = el("label", "formField");
  if (options.wide) wrapper.classList.add("wide");
  wrapper.append(el("span", "fieldLabel", `${label}${options.required ? " *" : ""}`));
  const controlRow = el("span", "controlRow");
  controlRow.append(control);
  if (options.suffix) controlRow.append(el("span", "controlSuffix", options.suffix));
  wrapper.append(controlRow);
  if (options.hint) wrapper.append(el("span", "fieldHint", options.hint));
  return wrapper;
}

function checkField(label, checked, onChange) {
  const wrapper = el("label", "checkField");
  const control = el("input", "");
  control.type = "checkbox";
  control.checked = checked;
  control.addEventListener("change", () => onChange(control.checked));
  wrapper.append(control, el("span", "", label));
  return wrapper;
}

function sectionHeading(title, description) {
  const heading = el("div", "formHeading");
  heading.append(el("h3", "", title), el("p", "", description));
  return heading;
}

function removeButton(onClick, label) {
  const node = button("×", "removeButton", onClick);
  node.setAttribute("aria-label", label);
  node.title = label;
  return node;
}

function reviewFact(label, value) {
  const node = el("div", "reviewFact");
  node.append(el("dt", "", label), el("dd", "", value || "None"));
  return node;
}

function note(text, tone = "") {
  const node = el("p", "formNote", text);
  if (tone) node.dataset.tone = tone;
  return node;
}

function emptyComposer(text) { return el("div", "emptyComposer", text); }
function optionName(options, uid) { return options.find((option) => option.uid === uid)?.head || ""; }
function labelValue(value) { return String(value || "").replaceAll("_", " ").replace(/\b\w/g, (letter) => letter.toUpperCase()); }
function formatNumber(value) { return new Intl.NumberFormat(undefined, { maximumFractionDigits: 3 }).format(Number(value)); }
function signed(value) { return `${value > 0 ? "+" : ""}${formatNumber(value)}`; }
function formatDateTime(value) { return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(new Date(value)); }
function firstUnused(options, selected) { return options.find((option) => !selected.includes(option.uid))?.uid || ""; }

function button(text, className, onClick) {
  const node = el("button", className, text);
  node.type = "button";
  node.addEventListener("click", onClick);
  return node;
}

function el(tag, className = "", text = null) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text != null) node.textContent = String(text);
  return node;
}
