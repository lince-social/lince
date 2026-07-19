import {
  comparableDraft,
  draftFromTransfer,
  emptyDependency,
  emptyDraft,
  emptyPromise,
  renewDraftRequest,
  toAdoptTransferAction,
  toClaimOpenPromiseAction,
  toCounterofferTransferAction,
  toCreateTransferAction,
  toReviseTransferAction,
  validateDraftStep,
} from "./create-model.js";

const STEPS = ["Terms", "People", "Promises", "Sharing", "Review"];
const CLAIM_STEPS = ["Claimant", "Refine promise", "Review"];

export function createTransferComposer(root, { host, onCreated, onSubmitted, onClosed }) {
  let step = 0;
  let open = false;
  let busy = false;
  let awaiting = false;
  let mutationsEnabled = true;
  let error = "";
  let people = [];
  let records = [];
  let units = [];
  let transfers = [];
  let transferRows = [];
  let viewer = {};
  let draft = emptyDraft(viewer);
  let mode = "create";
  let transferUid = "";
  let expectedRevision = 0;
  let baseDraft = "";
  let remoteRevision = null;
  let remoteSummary = "";
  let remoteRow = null;
  let editableInvitees = new Set();
  let contextualPeople = [];
  let contextualRecords = [];
  let contextualUnits = [];
  let actingPerson = "";
  let sourcePromiseUid = "";
  let sourceReusePolicy = "duplicate";
  let sourceProposer = "";
  let sourceDirection = "";

  function setOptions(next = {}) {
    viewer = normalizeViewer(next.viewer);
    people = normalizeOptions([...(next.people || []), ...contextualPeople]);
    if (viewer.person && !people.some((person) => person.uid === viewer.person)) {
      people.push({ uid: viewer.person, head: viewer.personHead || viewer.personSlug || viewer.person, slug: viewer.personSlug });
      people.sort((left, right) => left.head.localeCompare(right.head));
    }
    records = normalizeOptions([...(next.records || []), ...contextualRecords]);
    units = normalizeOptions([...(next.units || []), ...contextualUnits]);
    transferRows = Array.isArray(next.transfers) ? next.transfers : [];
    transfers = normalizeOptions(transferRows);
    if (viewer.person && !viewer.local && mode === "create") {
      draft.creator = viewer.person;
      draft.invitees = draft.invitees.filter((uid) => uid !== viewer.person);
    }
    if (open) render();
  }

  function setMutationsEnabled(value) {
    const next = Boolean(value);
    if (mutationsEnabled === next) return;
    mutationsEnabled = next;
    if (open) render();
  }

  function show(prefill = {}) {
    draft = emptyDraft(viewer);
    step = 0;
    error = "";
    busy = false;
    awaiting = false;
    mode = "create";
    transferUid = "";
    expectedRevision = 0;
    remoteRevision = null;
    remoteSummary = "";
    remoteRow = null;
    actingPerson = viewer.local ? "" : viewer.person;
    baseDraft = comparableForMode();
    sourcePromiseUid = "";
    sourceReusePolicy = "duplicate";
    sourceProposer = "";
    sourceDirection = "";
    editableInvitees = new Set();
    contextualPeople = [];
    contextualRecords = [];
    contextualUnits = [];
    applyCreationPrefill(prefill);
    open = true;
    root.hidden = false;
    render();
    requestAnimationFrame(() => root.querySelector("input, select, button")?.focus());
  }

  function showEdit(row) {
    if (!row?.uid) return;
    contextualPeople = projectedPeople(row);
    contextualRecords = projectedRecords(row);
    contextualUnits = projectedUnits(row);
    people = normalizeOptions([...people, ...contextualPeople]);
    records = normalizeOptions([...records, ...contextualRecords]);
    units = normalizeOptions([...units, ...contextualUnits]);
    draft = draftFromTransfer(row, viewer);
    step = 0;
    error = "";
    busy = false;
    awaiting = false;
    mode = Number(row.revision) === 0 ? "adopt" : "edit";
    transferUid = String(row.uid);
    expectedRevision = Number(row.revision || 0);
    actingPerson = viewer.local ? draft.creator : viewer.person;
    baseDraft = comparableForMode();
    remoteRevision = null;
    remoteSummary = "";
    remoteRow = null;
    editableInvitees = new Set(draft.invitees);
    sourcePromiseUid = "";
    sourceReusePolicy = "duplicate";
    sourceProposer = "";
    sourceDirection = "";
    open = true;
    root.hidden = false;
    render();
    requestAnimationFrame(() => root.querySelector("input, select, button")?.focus());
  }

  function showCounteroffer(row) {
    if (!row?.uid) return;
    openProjectedDraft(row, "counteroffer");
  }

  function showClaim(row, sourcePromise) {
    if (!row?.uid || !sourcePromise?.uid) return;
    openProjectedDraft(row, "claim");
    const copied = draft.promises.find((promise) => promise.uid === String(sourcePromise.uid));
    if (!copied) {
      error = "This OPEN promise is no longer part of the current signed revision.";
      render();
      return;
    }
    sourcePromiseUid = String(sourcePromise.uid);
    sourceReusePolicy = copied.reusePolicy;
    sourceProposer = copied.proposer || sourcePromise.proposer || sourcePromise.party || "";
    sourceDirection = copied.direction;
    copied.uid = "";
    copied.open = false;
    copied.party = actingPerson;
    copied.proposer = sourceProposer;
    copied.direction = oppositeDirection(sourceDirection);
    copied.reusePolicy = sourceReusePolicy;
    draft.promises = [copied];
    baseDraft = comparableForMode();
    render();
  }

  function openProjectedDraft(row, nextMode) {
    contextualPeople = projectedPeople(row);
    contextualRecords = projectedRecords(row);
    contextualUnits = projectedUnits(row);
    people = normalizeOptions([...people, ...contextualPeople]);
    records = normalizeOptions([...records, ...contextualRecords]);
    units = normalizeOptions([...units, ...contextualUnits]);
    draft = draftFromTransfer(row, viewer);
    step = 0;
    error = "";
    busy = false;
    awaiting = false;
    mode = nextMode;
    transferUid = String(row.uid);
    expectedRevision = Number(row.revision || 0);
    actingPerson = viewer.local ? "" : viewer.person;
    sourcePromiseUid = "";
    sourceReusePolicy = "duplicate";
    sourceProposer = "";
    sourceDirection = "";
    remoteRevision = null;
    remoteSummary = "";
    remoteRow = null;
    editableInvitees = new Set(draft.invitees);
    baseDraft = comparableForMode();
    open = true;
    root.hidden = false;
    render();
    requestAnimationFrame(() => root.querySelector("input, select, button")?.focus());
  }

  function updateProjection(row) {
    if (!open || mode === "create" || String(row?.uid || "") !== transferUid) return;
    const revision = Number(row?.revision || 0);
    if (revision <= expectedRevision) return;
    if (awaiting) return;
    if (comparableForMode() === baseDraft) {
      if (mode === "claim") refreshClaimFromProjection(row);
      else draft = draftFromTransfer(row, viewer);
      expectedRevision = revision;
      baseDraft = comparableForMode();
      remoteRevision = null;
      remoteSummary = "";
      remoteRow = null;
    } else {
      remoteRevision = revision;
      remoteSummary = revisionSummary(row);
      remoteRow = row;
    }
    render();
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
    identity.append(
      el("div", "eyebrow", mode === "create" ? "Manual transfer" : `Revision ${expectedRevision}`),
      el("h2", "", composerTitle()),
    );
    const dismiss = button("×", "iconButton", close);
    dismiss.setAttribute("aria-label", "Close transfer creator");
    dismiss.disabled = busy || awaiting;
    header.append(identity, dismiss);

    const progress = el("ol", "stepper");
    for (const [index, label] of activeSteps().entries()) {
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
    renderCurrentStep(content);

    if (error) {
      const alert = el("div", "formAlert", error);
      alert.setAttribute("role", "alert");
      content.prepend(alert);
    }
    if (remoteRevision != null) content.prepend(staleRevisionNotice());

    const footer = el("footer", "creatorFooter");
    const back = button("Back", "secondaryButton", () => {
      error = "";
      step -= 1;
      render();
    });
    back.disabled = step === 0 || busy || awaiting;
    const commitLabel = mode === "create" ? "Create transfer"
      : mode === "adopt" ? "Adopt reviewed draft"
        : mode === "counteroffer" ? "Sign counteroffer"
          : mode === "claim" ? `Sign ${sourceReusePolicy === "consume" ? "consumed" : "duplicated"} claim`
            : "Save revision";
    const nextLabel = step === activeSteps().length - 1 ? commitLabel : "Continue";
    const forward = button(awaiting ? "Waiting for live revision" : busy ? "Signing revision" : nextLabel, "primaryButton", next);
    forward.disabled = busy || awaiting || !mutationsEnabled;
    if (!mutationsEnabled) forward.title = "Reconnect and wait for a fresh Transfer snapshot before signing";
    footer.append(back, forward);
    shell.append(header, progress, content, footer);
    root.replaceChildren(shell);
  }

  function renderTerms(form) {
    const description = mode === "create"
      ? "Name the commitment and choose how agreement is reached."
      : mode === "counteroffer"
        ? "Your signed complete terms become the one current proposal and reset revision-bound agreement."
        : "Every public-result change creates a signed revision and resets affected agreement levels.";
    form.append(sectionHeading("Terms", description));
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
        ["inherit", "Cell default"],
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
    if (mode === "claim") {
      renderClaimant(form);
      return;
    }
    if (mode === "counteroffer") {
      form.append(sectionHeading("People", "Invitation lifecycle stays unchanged by this counteroffer."));
      const facts = el("dl", "reviewFacts");
      facts.append(
        reviewFact("Creator", optionName(people, draft.creator) || draft.creator),
        reviewFact("Acting participant", optionName(people, actingPerson) || actingPerson || "Select below"),
        reviewFact("Pending invitees", draft.invitees.map((uid) => optionName(people, uid) || uid).join(", ") || "None"),
      );
      form.append(facts);
      if (viewer.local) {
        form.append(field("Acting participant", optionSelect(participantOptions(), actingPerson, "Select accepted participant", (value) => {
          actingPerson = value;
        }), { required: true }));
      }
      return;
    }
    form.append(sectionHeading("People", "The creator proposes the transfer. Selected others remain pending invitees until each accepts."));
    if (viewer.person && !viewer.local) {
      const creator = el("dl", "reviewFacts");
      creator.append(reviewFact("Creator", optionName(people, viewer.person) || viewer.person));
      form.append(creator);
    } else {
      const creatorSelect = optionSelect(people, draft.creator, "Select creator Person", (value) => {
        draft.creator = value;
        draft.invitees = draft.invitees.filter((uid) => uid !== value);
        render();
      });
      form.append(field("Creator", creatorSelect, { required: true, hint: "Trusted local mode requires an explicit acting Person." }));
    }
    form.append(el("h3", "peopleSubheading", "Pending invitees"));
    const list = el("div", "composerList");
    for (const [index, uid] of draft.invitees.entries()) {
      const row = el("div", "composerRow partyComposerRow");
      row.append(optionSelect(inviteeOptions(), uid, "Select invitee", (value) => { draft.invitees[index] = value; }));
      row.append(removeButton(() => { draft.invitees.splice(index, 1); render(); }, mode === "create" ? "Remove invitee" : "Withdraw invitation"));
      list.append(row);
    }
    if (!draft.invitees.length) list.append(emptyComposer("No pending invitees"));
    form.append(list);
    const available = inviteeOptions();
    if (mode === "create") {
      const add = button("+ Add invitee", "secondaryButton", () => {
        draft.invitees.push(firstUnused(available, draft.invitees));
        render();
      });
      add.disabled = !available.length || draft.invitees.length >= available.length;
      form.append(add);
    }
    if (!people.length) form.append(note("Create Person records before composing a transfer."));
    else form.append(note("Invitees are addressed by this proposal. They become transfer parties only after accepting it."));
  }

  function renderPromises(form) {
    form.append(sectionHeading(
      mode === "claim" ? "Refine promise" : "Promises",
      mode === "claim"
        ? `${sourceReusePolicy === "consume" ? "Consume" : "Duplicate"} source into a concrete counter-promise signed by ${optionName(people, actingPerson) || actingPerson || "the claimant"}.`
        : "Describe signed terms. OPEN keeps the acting proposer as owner while leaving only the counterparty unspecified.",
    ));
    const list = el("div", "promiseComposerList");
    for (const [index, promise] of draft.promises.entries()) {
      const card = el("fieldset", "promiseComposer");
      const legend = el("legend", "", mode === "claim" ? "Claimed terms" : `Promise ${index + 1}`);
      const remove = mode === "claim" ? null : removeButton(() => { removePromise(index); }, `${mode === "create" ? "Remove" : "Withdraw"} promise ${index + 1}`);
      remove?.classList.add("promiseRemove");
      const grid = el("div", "formGrid");
      const reuse = select([["duplicate", "Duplicate (default)"], ["consume", "Consume"]], promise.reusePolicy, (value) => { promise.reusePolicy = value; });
      reuse.disabled = !promise.open;
      grid.append(
        field(mode === "claim" ? "Claimant" : "Counterparty", mode === "claim" ? lockedValue(optionName(people, actingPerson) || actingPerson) : promisePersonSelect(promise), {
          hint: mode === "claim"
            ? `Distinct from proposer ${personLabel(sourceProposer)}; both sides must agree before activation.`
            : promise.open
              ? `OPEN proposal owned by ${personLabel(promise.proposer || proposalPerson())}; another Person may sign a refinement.`
              : "The selected Person owns these concrete terms.",
        }),
        field("Direction", select([["gives", "Gives"], ["receives", "Receives"]], promise.direction, (value) => { promise.direction = value; })),
        field("Record", optionSelect(records, promise.record, "Select record", (value) => {
          promise.record = value;
          const record = records.find((candidate) => candidate.uid === value);
          if (!promise.unit && record?.unit) promise.unit = record.unit;
        }), { required: true }),
        field("Quantity", numberInput(promise.quantity, 0.001, null, (value) => { promise.quantity = value; }), { required: true }),
        field("Signed unit", optionSelect(units, promise.unit, "Intentionally unitless", (value) => { promise.unit = value; }), { hint: "The selected canonical concept is copied into this signed revision" }),
        ...(mode === "claim" ? [] : [field("When used", reuse, { hint: promise.open ? "Signed behavior when another person uses this suggestion" : "Only OPEN promises are reusable" })]),
        field("Window starts", input("datetime-local", promise.windowStart, (value) => { promise.windowStart = value; })),
        field("Deadline", input("datetime-local", promise.windowEnd, (value) => { promise.windowEnd = value; })),
        field("Reserve from", select([
          ["", "Transfer default"], ["none", "Never"], ["proposed", "Proposal"], ["agreed", "Agreement"], ["active", "Activation"],
        ], promise.reserveFrom, (value) => { promise.reserveFrom = value; })),
        field("Condition", input("text", promise.condition, (value) => { promise.condition = value; }), { hint: "Optional formula or prerequisite", wide: true }),
      );
      grid.append(...placeFields("Place", promise.place));
      card.append(legend);
      if (remove) card.append(remove);
      card.append(grid);
      list.append(card);
    }
    if (!draft.promises.length) list.append(emptyComposer("No promises added"));
    form.append(list);
    if (mode !== "claim") {
      const add = button("+ Add promise", "secondaryButton", () => {
        draft.promises.push(emptyPromise(draft.creator || draft.invitees[0] || ""));
        render();
      });
      add.disabled = !draft.creator || !records.length;
      form.append(add);
    }
    if (!draft.promises.length && mode !== "create") {
      form.append(note("After pending invitations are withdrawn, saving this reviewed empty promise set withdraws the complete draft. Existing promises remain in signed revision history."));
    }
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
    grid.append(...placeFields("Default place", draft.defaultPlace));
    form.append(grid);
    renderDependencies(form);
  }

  function renderReview(form) {
    const description = mode === "create" ? "These terms are submitted together as one transfer draft."
      : mode === "claim" ? `This ${sourceReusePolicy} claim refines the OPEN source against revision ${expectedRevision}.`
        : `These complete terms replace revision ${expectedRevision} in one signed mutation.`;
    form.append(sectionHeading("Review", description));
    const facts = el("dl", "reviewFacts");
    facts.append(reviewFact("Title", draft.head));
    if (mode === "claim") {
      facts.append(
        reviewFact("OPEN proposer", personLabel(sourceProposer)),
        reviewFact("Claimant", personLabel(actingPerson)),
        reviewFact("Pair agreement", "Both proposer and claimant must agree"),
      );
    } else {
      facts.append(reviewFact("Creator", optionName(people, draft.creator) || draft.creator));
    }
    facts.append(
      reviewFact("Pending invitees", mode === "claim" ? "Unchanged" : draft.invitees.map((uid) => optionName(people, uid) || uid).join(", ") || "None"),
      reviewFact("Agreement", draft.agreement === "percentage" ? `${draft.agreementPct}%` : labelValue(draft.agreement)),
      reviewFact("Visibility", draft.visibility === "proximity" ? `Proximity ${draft.maxProximity}` : labelValue(draft.visibility)),
      reviewFact("Confirmations", draft.requireConfirmation ? "Delivery and receipt" : "Not required"),
      reviewFact("Hierarchy", optionName(transfers, draft.parent) || "Top level"),
      reviewFact("Source", optionName(records, draft.source) || "None"),
      reviewFact("Default place", placeLabel(draft.defaultPlace)),
    );
    form.append(facts);

    const terms = el("div", "reviewTerms");
    for (const promise of draft.promises) {
      const quantity = Number(promise.quantity);
      const publicDelta = promise.direction === "gives" ? -quantity : quantity;
      const privateDelta = publicDelta;
      const row = el("article", "reviewTerm");
      const publicTerm = el("div", "reviewTermMain");
      const promiseOwner = promise.open
        ? `OPEN proposed by ${personLabel(promise.proposer || proposalPerson())}`
        : personLabel(promise.party);
      publicTerm.append(
        el("strong", "", `${promiseOwner} ${promise.direction}`),
        el("span", "", `${formatNumber(quantity)}${promise.unit ? ` ${promise.unit}` : ""} · ${optionName(records, promise.record)}`),
      );
      const deltas = el("dl", "reviewDeltas");
      deltas.append(
        reviewFact("Public occurrence", signed(publicDelta)),
        reviewFact("Private quantity effect", signed(privateDelta)),
      );
      row.append(publicTerm, deltas);
      const window = promise.windowStart || promise.windowEnd
        ? `${promise.windowStart ? formatDateTime(promise.windowStart) : "Now"} to ${promise.windowEnd ? formatDateTime(promise.windowEnd) : "open-ended"}` : "";
      const metadata = [window, placeLabel(promise.place, ""), promise.condition, labelValue(promise.reusePolicy)].filter(Boolean);
      if (metadata.length) {
        row.append(el("div", "reviewMeta", metadata.join(" · ")));
      }
      terms.append(row);
    }
    form.append(terms);
    if (mode === "claim") {
      form.append(note("Submitting signs one concrete proposer-and-claimant pair. The new pair cannot activate until both people reach the required agreement level."));
    }
    if (mode !== "claim" && draft.dependencies.length) {
      const dependencies = el("div", "reviewDependencies");
      dependencies.append(el("h3", "", "Dependencies"));
      for (const dependency of draft.dependencies) {
        dependencies.append(el("div", "reviewDependency", dependencyLabel(dependency)));
      }
      form.append(dependencies);
    }
    if (awaiting) form.append(note("The Actions were accepted. This view will open the transfer after its live projection arrives.", "success"));
  }

  function next() {
    if (busy || awaiting) return;
    error = validateCurrentStep();
    if (error) { render(); return; }
    if (step < activeSteps().length - 1) {
      step += 1;
      render();
      requestAnimationFrame(() => root.querySelector("input, select, button")?.focus());
      return;
    }
    submit();
  }

  async function submit() {
    if (!mutationsEnabled) {
      error = "Reconnect and wait for a fresh Transfer snapshot before signing.";
      render();
      return;
    }
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
      const action = mode === "create" ? toCreateTransferAction(draft)
        : mode === "adopt" ? toAdoptTransferAction(transferUid, draft)
          : mode === "counteroffer" ? toCounterofferTransferAction(transferUid, expectedRevision, draft, actingPerson)
            : mode === "claim" ? toClaimOpenPromiseAction(transferUid, sourcePromiseUid, expectedRevision, draft, actingPerson)
              : toReviseTransferAction(transferUid, expectedRevision, draft);
      const result = await host.act(action);
      const uid = mode === "create" ? String(result?.created || "") : transferUid;
      if (!uid) throw new Error("The transfer mutation completed without an identifier.");
      busy = false;
      awaiting = true;
      render();
      if (mode === "create") onCreated?.(uid, result);
      else onSubmitted?.({ uid, expectedRevision, requestId: action.request_id, result });
    } catch (cause) {
      busy = false;
      error = cause instanceof Error ? cause.message : "Transfer creation failed.";
      if (cause?.code === "transfer_revision_stale") {
        error = "This draft was not saved because a newer signed revision exists. Review the intervening revision before reapplying your changes.";
      }
      render();
    }
  }

  function validateAll() {
    if (mode === "claim") {
      if (!actingPerson) return "Select the Person signing this claim.";
      if (!sourceProposer) return "The OPEN proposal has no traceable proposer identity.";
      if (actingPerson === sourceProposer) return "The OPEN proposer cannot claim their own proposal.";
      if (!claimantOptions().some((person) => person.uid === actingPerson)) return "Choose an available claimant distinct from the proposer.";
      if (draft.promises.length !== 1) return "The OPEN claim must refine exactly one promise.";
      draft.promises[0].party = actingPerson;
      draft.promises[0].open = false;
      draft.promises[0].reusePolicy = sourceReusePolicy;
      if (draft.promises[0].direction === sourceDirection) return "The claimant direction must oppose the OPEN proposal direction.";
      return validateDraftStep({ ...draft, agreement: "full", allowEmptyPromises: false, participants: [actingPerson] }, 2);
    }
    if (mode === "counteroffer" && !actingPerson) return "Select the accepted participant signing this counteroffer.";
    if (mode === "counteroffer" && !participantOptions().some((person) => person.uid === actingPerson)) return "Choose an accepted participant to sign this counteroffer.";
    for (let index = 0; index < STEPS.length - 1; index += 1) {
      const message = validateDraftStep(draft, index);
      if (message) return message;
    }
    return "";
  }

  function validateCurrentStep() {
    if (mode !== "claim") return validateDraftStep(draft, step);
    if (step === 0) {
      if (!actingPerson) return "Select the Person signing this claim.";
      if (actingPerson === sourceProposer) return "The OPEN proposer cannot claim their own proposal.";
      return claimantOptions().some((person) => person.uid === actingPerson)
        ? "" : "Choose an available claimant distinct from the proposer.";
    }
    if (step === 1) return validateAll();
    return "";
  }

  function selectedPeople() {
    const ids = new Set([draft.creator, ...draft.invitees, ...(draft.participants || [])].filter(Boolean));
    return people.filter((person) => ids.has(person.uid));
  }

  function participantOptions(includeAllForClaim = false) {
    if (mode === "claim" && (includeAllForClaim || viewer.local)) return people;
    const participantIds = new Set(draft.participants || []);
    return people.filter((person) => participantIds.has(person.uid));
  }

  function claimantOptions() {
    return people.filter((person) => person.uid !== sourceProposer);
  }

  function inviteeOptions() {
    return people.filter((person) => person.uid !== draft.creator && (mode === "create" || editableInvitees.has(person.uid)));
  }

  function staleRevisionNotice() {
    const notice = el("section", "revisionNotice");
    notice.append(
      el("strong", "", `Revision ${remoteRevision} arrived while this draft had unsaved changes.`),
      el("p", "", remoteSummary || "The signed public terms changed in another open view. Review them before deciding whether your complete draft should replace them."),
    );
    const actions = el("div", "revisionNoticeActions");
    const review = button(`Review revision ${remoteRevision}`, "secondaryButton", () => {
      if (mode === "claim" && !refreshClaimFromProjection(remoteRow)) {
        render();
        return;
      }
      if (mode !== "claim") draft = draftFromTransfer(remoteRow, viewer);
      expectedRevision = remoteRevision;
      baseDraft = comparableForMode();
      editableInvitees = new Set(draft.invitees);
      remoteRevision = null;
      remoteSummary = "";
      remoteRow = null;
      error = "";
      render();
    });
    const currentEditablePromiseUids = new Set((remoteRow?.promises || [])
      .filter((promise) => ["open", "proposed", "agreed"].includes(String(promise?.state || "")))
      .map((promise) => String(promise.uid || "")));
    const hasNonEditableLocalPromise = draft.promises.some((promise) => promise.uid && !currentEditablePromiseUids.has(promise.uid));
    const replace = button(mode === "claim" ? `Apply my claim to revision ${remoteRevision}` : `Replace revision ${remoteRevision} with my complete draft`, "secondaryButton", () => {
      const stillPending = new Set((remoteRow?.invitations || [])
        .filter((invitation) => invitation?.status === "pending")
        .map((invitation) => String(invitation.addressed_person || invitation.addressed_person_uid || "")));
      if (mode !== "claim") {
        draft.invitees = draft.invitees.filter((uid) => stillPending.has(uid));
        editableInvitees = stillPending;
      }
      expectedRevision = remoteRevision;
      renewDraftRequest(draft);
      remoteRevision = null;
      remoteSummary = "";
      remoteRow = null;
      error = "";
      render();
    });
    const sourceStillOpen = mode !== "claim" || currentOpenSource(remoteRow) != null;
    replace.disabled = hasNonEditableLocalPromise || !sourceStillOpen;
    replace.title = !sourceStillOpen
      ? "The source promise is no longer OPEN. Review the signed revision."
      : hasNonEditableLocalPromise
      ? "A promise in your draft is no longer editable. Review the signed revision instead."
      : "This explicitly replaces all editable public terms; omitted remote additions are withdrawn with signed history.";
    actions.append(review, replace);
    notice.append(actions);
    return notice;
  }

  function promisePersonSelect(promise) {
    const node = optionSelect(selectedPeople(), promise.open ? "" : promise.party, "OPEN", (value) => {
      promise.open = !value;
      promise.party = value;
      if (value) {
        promise.proposer = "";
        promise.reusePolicy = "duplicate";
      } else {
        promise.proposer ||= proposalPerson();
      }
      render();
    });
    return node;
  }

  function renderClaimant(form) {
    form.append(sectionHeading("Claimant", `${sourceReusePolicy === "consume" ? "Consume" : "Duplicate"} the OPEN source into a concrete proposer-and-claimant pair.`));
    const identities = el("dl", "reviewFacts");
    identities.append(
      reviewFact("OPEN proposer", personLabel(sourceProposer)),
      reviewFact("Required relationship", "Claimant must be a different Person"),
    );
    form.append(identities);
    if (viewer.person && !viewer.local) {
      actingPerson = viewer.person;
      draft.promises[0].party = actingPerson;
      form.append(field("Signing Person", lockedValue(optionName(people, actingPerson) || actingPerson), { required: true }));
    } else {
      form.append(field("Signing Person", optionSelect(claimantOptions(), actingPerson, "Select claimant Person", (value) => {
        actingPerson = value;
        draft.promises[0].party = value;
      }), { required: true }));
    }
    form.append(note("The signed claim creates both sides of one concrete exchange. The proposer and claimant must each agree before later stages unlock."));
  }

  function refreshClaimFromProjection(row) {
    const latest = draftFromTransfer(row, viewer);
    const source = latest.promises.find((promise) => promise.uid === sourcePromiseUid && promise.open);
    if (!source) {
      error = "The source promise is no longer OPEN in this signed revision.";
      return false;
    }
    sourceReusePolicy = source.reusePolicy;
    sourceProposer = source.proposer;
    sourceDirection = source.direction;
    source.uid = "";
    source.open = false;
    source.party = actingPerson;
    source.direction = oppositeDirection(sourceDirection);
    source.reusePolicy = sourceReusePolicy;
    latest.promises = [source];
    draft = latest;
    return true;
  }

  function currentOpenSource(row) {
    return (Array.isArray(row?.promises) ? row.promises : []).find((promise) => String(promise?.uid || "") === sourcePromiseUid
      && Boolean(promise?.open || promise?.state === "open"));
  }

  function proposalPerson() {
    return mode === "counteroffer" ? actingPerson : draft.creator;
  }

  function personLabel(uid) {
    return optionName(people, uid) || uid || "Unavailable";
  }

  function comparableForMode() {
    const terms = comparableDraft(draft);
    return mode === "claim" ? JSON.stringify([actingPerson, sourcePromiseUid, terms]) : JSON.stringify([actingPerson, terms]);
  }

  function activeSteps() { return mode === "claim" ? CLAIM_STEPS : STEPS; }

  function composerTitle() {
    if (mode === "create") return "New transfer";
    if (mode === "adopt") return "Review legacy transfer";
    if (mode === "counteroffer") return "Counteroffer";
    if (mode === "claim") return "Claim OPEN promise";
    return "Edit transfer";
  }

  function renderCurrentStep(form) {
    if (mode === "claim") {
      if (step === 0) renderPeople(form);
      if (step === 1) renderPromises(form);
      if (step === 2) renderReview(form);
      return;
    }
    if (step === 0) renderTerms(form);
    if (step === 1) renderPeople(form);
    if (step === 2) renderPromises(form);
    if (step === 3) renderSharing(form);
    if (step === 4) renderReview(form);
  }

  function placeFields(label, place) {
    return [
      field(`${label} latitude`, coordinateInput(place.lat, -90, 90, (value) => { place.lat = value; }), { hint: "-90 to 90" }),
      field(`${label} longitude`, coordinateInput(place.lon, -180, 180, (value) => { place.lon = value; }), { hint: "-180 to 180" }),
      field(`${label} address`, input("text", place.address, (value) => { place.address = value; }), { hint: "Optional signed label", wide: true }),
    ];
  }

  function renderDependencies(form) {
    if (draft.dependencies.length) ensurePromiseUids();
    const section = el("section", "dependencyComposerSection");
    section.append(sectionHeading("Dependencies", "Each signed gate blocks this transfer or one selected promise until its upstream item reaches the required state."));
    const list = el("div", "dependencyComposerList");
    for (const [index, dependency] of draft.dependencies.entries()) {
      const row = el("fieldset", "dependencyComposer");
      row.append(el("legend", "", `Dependency ${index + 1}`));
      const remove = removeButton(() => {
        draft.dependencies.splice(index, 1);
        render();
      }, `Remove dependency ${index + 1}`);
      remove.classList.add("dependencyRemove");
      const grid = el("div", "formGrid");
      const scope = select([["transfer", "Whole transfer"], ["promise", "One promise"]], dependency.scope, (value) => {
        dependency.scope = value;
        if (value === "promise") {
          ensurePromiseUids();
          if (!draft.promises.some((promise) => promise.uid === dependency.promise)) dependency.promise = draft.promises[0]?.uid || "";
        } else dependency.promise = "";
        render();
      });
      grid.append(field("Scope", scope));
      if (dependency.scope === "promise") {
        ensurePromiseUids();
        grid.append(field("Blocked promise", optionSelect(draftPromiseOptions(), dependency.promise, "Select promise", (value) => { dependency.promise = value; }), { required: true }));
      }
      grid.append(field("Upstream kind", select([["transfer", "Transfer"], ["promise", "Promise"]], dependency.upstreamKind, (value) => {
        dependency.upstreamKind = value;
        dependency.upstream = "";
        render();
      })));
      const upstreamOptions = dependency.upstreamKind === "promise" ? upstreamPromiseOptions() : transfers.filter((transfer) => transfer.uid !== transferUid);
      grid.append(
        field("Upstream", optionSelect(upstreamOptions, dependency.upstream, `Select upstream ${dependency.upstreamKind}`, (value) => { dependency.upstream = value; }), { required: true }),
        field("Required state", select([
          ["kept", "Kept"], ["active", "Active"], ["agreed", "Agreed"], ["proposed", "Proposed"],
          ["open", "Open"], ["broken", "Broken"], ["withdrawn", "Withdrawn"],
        ], dependency.requiredState, (value) => { dependency.requiredState = value; })),
      );
      row.append(remove, grid);
      list.append(row);
    }
    if (!draft.dependencies.length) list.append(emptyComposer("No dependency gates"));
    section.append(list);
    const add = button("+ Add dependency", "secondaryButton", () => {
      draft.dependencies.push(emptyDependency());
      render();
    });
    section.append(add);
    form.append(section);
  }

  function removePromise(index) {
    const uid = draft.promises[index]?.uid;
    draft.promises.splice(index, 1);
    if (uid) draft.dependencies = draft.dependencies.filter((dependency) => dependency.promise !== uid && !(dependency.upstreamKind === "promise" && dependency.upstream === uid));
    render();
  }

  function ensurePromiseUids() {
    for (const promise of draft.promises) {
      if (!promise.uid) promise.uid = clientUid("p");
    }
  }

  function draftPromiseOptions() {
    return draft.promises.map((promise, index) => ({
      uid: promise.uid,
      head: `Promise ${index + 1} · ${optionName(records, promise.record) || promise.record || "Unbound"}`,
      slug: "",
    }));
  }

  function upstreamPromiseOptions() {
    const options = [...draftPromiseOptions()];
    for (const transfer of transferRows) {
      for (const [index, promise] of (Array.isArray(transfer?.promises) ? transfer.promises : []).entries()) {
        const uid = String(promise?.uid || "");
        if (!uid || options.some((option) => option.uid === uid)) continue;
        options.push({
          uid,
          head: `${transfer.head || transfer.slug || transfer.uid} · ${promise.record_head || promise.record_slug || `Promise ${index + 1}`}`,
          slug: "",
        });
      }
    }
    return options;
  }

  function dependencyLabel(dependency) {
    const scope = dependency.scope === "promise" ? optionName(draftPromiseOptions(), dependency.promise) || dependency.promise : "Whole transfer";
    const upstreamOptions = dependency.upstreamKind === "promise" ? upstreamPromiseOptions() : transfers.filter((transfer) => transfer.uid !== transferUid);
    const upstream = optionName(upstreamOptions, dependency.upstream) || dependency.upstream;
    return `${scope} waits for ${labelValue(dependency.upstreamKind)} ${upstream} · ${labelValue(dependency.requiredState)}`;
  }

  function applyCreationPrefill(raw) {
    const prefill = raw && typeof raw === "object" ? raw : {};
    const supplied = Array.isArray(prefill.records)
      ? prefill.records
      : prefill.record != null
        ? [prefill.record]
        : [];
    contextualRecords = normalizeOptions(supplied.filter((value) => value && typeof value === "object"));
    records = normalizeOptions([...records, ...contextualRecords]);
    const recordUids = [...new Set(supplied
      .map((value) => String(value && typeof value === "object" ? value.uid || "" : value || ""))
      .filter(Boolean))];
    for (const recordUid of recordUids) {
      const promise = emptyPromise(prefill.open ? "" : draft.creator);
      promise.record = recordUid;
      const record = records.find((candidate) => candidate.uid === recordUid);
      if (record?.unit) promise.unit = record.unit;
      draft.promises.push(promise);
    }
    if (typeof prefill.head === "string") draft.head = prefill.head;
  }

  return {
    show,
    showEdit,
    showCounteroffer,
    showClaim,
    close,
    complete,
    updateProjection,
    setAwaitingLive,
    setMutationsEnabled,
    setOptions,
  };
}

function normalizeViewer(raw) {
  const viewer = raw && typeof raw === "object" ? raw : {};
  return {
    local: Boolean(viewer.local),
    person: String(viewer.person || ""),
    personHead: String(viewer.person_head || ""),
    personSlug: String(viewer.person_slug || ""),
  };
}

function normalizeOptions(options) {
  return (Array.isArray(options) ? options : [])
    .map((option) => ({
      uid: String(option?.uid || ""),
      head: String(option?.head || option?.name || option?.slug || option?.uid || "Unnamed"),
      slug: String(option?.slug || ""),
      unit: String(option?.unit || option?.unit_uid || option?.unit_name || ""),
    }))
    .filter((option) => option.uid)
    .sort((left, right) => left.head.localeCompare(right.head));
}

function projectedPeople(row) {
  const values = [];
  for (const party of Array.isArray(row?.parties) ? row.parties : []) {
    values.push({ uid: party?.actor, head: party?.actor_head, slug: party?.actor_slug });
  }
  for (const invitation of Array.isArray(row?.invitations) ? row.invitations : []) {
    values.push({
      uid: invitation?.addressed_person || invitation?.addressed_person_uid,
      head: invitation?.addressed_person_head,
      slug: invitation?.addressed_person_slug,
    });
  }
  return values.filter((value) => value.uid);
}

function projectedRecords(row) {
  return (Array.isArray(row?.promises) ? row.promises : []).map((promise) => ({
    uid: promise?.record || promise?.record_uid,
    head: promise?.record_head,
    slug: promise?.record_slug,
    unit: promise?.unit || promise?.unit_uid || promise?.unit_name,
  })).filter((value) => value.uid);
}

function projectedUnits(row) {
  return (Array.isArray(row?.promises) ? row.promises : []).map((promise) => ({
    uid: promise?.unit || promise?.unit_uid,
    head: promise?.unit_name || promise?.unit || promise?.unit_uid,
  })).filter((value) => value.uid);
}

function optionSelect(options, selected, placeholder, onChange) {
  const node = el("select", "");
  node.append(new Option(placeholder, ""));
  for (const option of options) node.append(new Option(option.slug ? `${option.head} · ${option.slug}` : option.head, option.uid));
  if (selected && !options.some((option) => option.uid === selected)) node.append(new Option(selected, selected));
  node.value = selected;
  node.addEventListener("change", () => onChange(node.value));
  return node;
}

function lockedValue(value) {
  const node = el("span", "lockedValue", value || "Not selected");
  node.setAttribute("aria-live", "polite");
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

function coordinateInput(value, min, max, onInput) {
  const node = input("number", value, onInput);
  node.step = "any";
  node.min = String(min);
  node.max = String(max);
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
function oppositeDirection(direction) { return direction === "gives" ? "receives" : "gives"; }
function formatNumber(value) { return new Intl.NumberFormat(undefined, { maximumFractionDigits: 3 }).format(Number(value)); }
function signed(value) { return `${value > 0 ? "+" : ""}${formatNumber(value)}`; }
function formatDateTime(value) { return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(new Date(value)); }
function firstUnused(options, selected) { return options.find((option) => !selected.includes(option.uid))?.uid || ""; }

function clientUid(prefix) {
  const alphabet = "0123456789ABCDEFGHJKMNPQRSTVWXYZ";
  let time = Date.now();
  const timeChars = new Array(10);
  for (let index = 9; index >= 0; index -= 1) {
    timeChars[index] = alphabet[time % 32];
    time = Math.floor(time / 32);
  }
  let entropy = "";
  if (globalThis.crypto?.getRandomValues) {
    const bytes = new Uint8Array(16);
    globalThis.crypto.getRandomValues(bytes);
    for (const byte of bytes) entropy += alphabet[byte % alphabet.length];
  } else {
    for (let index = 0; index < 16; index += 1) entropy += alphabet[Math.floor(Math.random() * alphabet.length)];
  }
  return `${prefix}_${timeChars.join("")}${entropy}`;
}

function placeLabel(place, fallback = "None") {
  const lat = String(place?.lat ?? "").trim();
  const lon = String(place?.lon ?? "").trim();
  const address = String(place?.address || "").trim();
  if (!lat && !lon && !address) return fallback;
  return [address, lat && lon ? `${lat}, ${lon}` : ""].filter(Boolean).join(" · ");
}

function revisionSummary(row) {
  const projected = row?.prior_revision_summary || row?.revision_summary || row?.change_summary;
  if (typeof projected === "string" && projected.trim()) return projected.trim();
  if (Array.isArray(projected)) return projected.map(String).filter(Boolean).join(" · ");
  const evidenceProjection = row?.revision_evidence;
  const evidence = Array.isArray(evidenceProjection)
    ? evidenceProjection.at(-1)
    : evidenceProjection?.current || evidenceProjection;
  if (evidence && typeof evidence === "object") {
    const actor = evidence.actor_head || evidence.actor_slug || evidence.actor;
    const at = evidence.at ? formatDateTime(evidence.at) : "";
    const changedFields = evidenceProjection?.changed_fields || evidence.changed;
    const changed = Array.isArray(changedFields) ? changedFields.map(labelValue).join(", ") : "";
    const details = [actor && `Signed by ${actor}`, at, changed && `Changed: ${changed}`].filter(Boolean);
    if (details.length) return details.join(" · ");
  }
  return `The server now projects revision ${Number(row?.revision || 0)}. Compare its complete terms before reapplying this draft.`;
}

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
