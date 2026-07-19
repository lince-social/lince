import { agreementMilestoneLabel, formatDate, partyName, statusLabel } from "./model.js";

const localSelection = new Map();

export function renderAgreementControl(row, options) {
  const block = el("div", "agreementControl");
  const parties = Array.isArray(row.parties) ? row.parties : [];
  let selected = selectedParty(row, options, parties);

  if (options.viewer?.local && parties.length) {
    const picker = el("select", "agreementPersonSelect");
    for (const party of parties) picker.append(new Option(partyName(party), party.actor));
    picker.value = selected?.actor || parties[0].actor;
    picker.addEventListener("change", () => {
      localSelection.set(row.uid, picker.value);
      selected = parties.find((party) => party.actor === picker.value) || null;
      controls.replaceChildren(renderLevelControls(row, selected, options));
    });
    block.append(labeled("Acting Person", picker));
  }

  const controls = el("div", "agreementLevelControls");
  controls.append(renderLevelControls(row, selected, options));
  block.append(controls);

  const readiness = renderReadiness(row.readiness, "Transfer readiness");
  if (readiness) block.append(readiness);

  const coalition = row.agreement?.coalition || [];
  if (coalition.length) {
    const members = el("div", "coalitionMembers");
    members.append(el("strong", "", row.agreement.coalition_revision
      ? `Committed coalition · revision ${row.agreement.coalition_revision}`
      : "Committed coalition"));
    for (const person of coalition) {
      const party = parties.find((candidate) => candidate.actor === person || candidate.uid === person);
      members.append(el("span", "coalitionMember", party ? partyName(party) : person));
    }
    block.append(members);
  }

  const events = agreementEvents(row);
  if (events.length) {
    const history = el("ol", "agreementHistory");
    for (const event of events) {
      const party = parties.find((candidate) => candidate.actor === event.person || candidate.uid === event.person);
      const item = el("li", "agreementEvent");
      item.append(
        el("strong", "", `${agreementMilestoneLabel(event.from_level)} → ${agreementMilestoneLabel(event.level)}`),
        el("span", "", [party ? partyName(party) : event.person, `Revision ${event.revision}`, event.at ? formatDate(event.at) : ""].filter(Boolean).join(" · ")),
      );
      history.append(item);
    }
    block.append(history);
  }
  return block;
}

export function renderReadiness(raw, label = "Readiness") {
  const readiness = raw && typeof raw === "object" ? raw : null;
  if (!readiness || readiness.known === false) return null;
  const block = el("div", "readinessBlock");
  block.dataset.ready = String(Boolean(readiness.ready));
  block.append(
    el("strong", "", label),
    el("span", "readinessState", readiness.ready ? "Ready" : "Blocked"),
  );
  const blockers = Array.isArray(readiness.blockers) ? readiness.blockers : [];
  if (blockers.length) {
    const list = el("ul", "readinessBlockers");
    for (const blocker of blockers) list.append(el("li", "", blockerLabel(blocker)));
    block.append(list);
  }
  return block;
}

function renderLevelControls(row, party, options) {
  const wrapper = el("div", "agreementLevelControl");
  if (!party) {
    wrapper.append(el("span", "emptyInline", "No accepted party is available for agreement."));
    return wrapper;
  }
  const level = Math.max(0, Math.min(2, Number(party.level) || 0));
  wrapper.append(
    el("div", "agreementCurrent", agreementMilestoneLabel(level)),
    el("span", "agreementRevision", `Signed against revision ${row.revision}`),
  );
  const actions = el("div", "inlineActions");
  if (level > 0 && canSetLevel(row, party, level - 1)) {
    actions.append(levelButton(row, party, level - 1, `Back to ${agreementMilestoneLabel(level - 1)}`, options));
  }
  if (level < 2 && canSetLevel(row, party, level + 1)) {
    actions.append(levelButton(row, party, level + 1, agreementMilestoneLabel(level + 1), options, true));
  }
  if (actions.childElementCount) wrapper.append(actions);
  else {
    const blockers = blockersForLevel(row, party, level < 2 ? level + 1 : level - 1);
    if (blockers.length) wrapper.append(el("div", "agreementBlockers", blockers.map(blockerLabel).join(" · ")));
  }
  const errors = [level - 1, level + 1]
    .filter((target) => target >= 0 && target <= 2)
    .map((target) => options.actionState?.(`agreement:${row.uid}:${party.actor}:${target}`)?.error)
    .filter(Boolean);
  if (errors.length) {
    const error = el("div", "inlineAlert", errors[0]);
    error.setAttribute("role", "alert");
    wrapper.append(error);
  }
  return wrapper;
}

function levelButton(row, party, target, text, options, primary = false) {
  const key = `agreement:${row.uid}:${party.actor}:${target}`;
  const state = options.actionState?.(key) || null;
  const control = el("button", primary ? "primaryButton" : "secondaryButton");
  control.type = "button";
  control.textContent = state?.waiting ? "Waiting for live agreement" : state?.busy ? "Signing" : text;
  control.disabled = options.mutationsEnabled === false || Boolean(state?.waiting || state?.busy);
  control.addEventListener("click", () => options.onAction?.(key, {
    action: "set-transfer-agreement-level",
    transfer: row.uid,
    expected_revision: row.revision,
    request_id: requestId(),
    person: options.viewer?.local ? party.actor : null,
    level: target,
  }));
  if (state?.error) control.title = state.error;
  return control;
}

function selectedParty(row, options, parties) {
  if (!options.viewer?.local) {
    const actor = row.viewer_party?.actor || options.viewer?.person;
    return parties.find((party) => party.actor === actor || party.uid === row.viewer_party?.uid) || row.viewer_party || null;
  }
  const selected = localSelection.get(row.uid);
  return parties.find((party) => party.actor === selected) || parties[0] || null;
}

function canSetLevel(row, party, target) {
  const current = Number(party.level) || 0;
  if (Math.abs(current - target) !== 1) return false;
  if (target > current && row.agreement?.coalition?.length
    && !row.agreement.coalition.includes(party.uid)
    && !row.agreement.coalition.includes(party.actor)) return false;
  const sources = [party.capabilities, row.viewer_party?.capabilities, row.capabilities];
  const direction = target > current ? "forward" : "back";
  const milestone = target === 2 ? "commit" : target === 1 ? (direction === "forward" ? "review" : "uncommit") : "unreview";
  const names = [
    milestone,
    `agreement_${direction}`,
    `agreement_level_${target}`,
    `set_agreement_level_${target}`,
  ];
  return sources.some((capabilities) => capabilityAllows(capabilities, names, target));
}

function capabilityAllows(capabilities, names, target) {
  if (!capabilities || typeof capabilities !== "object") return false;
  if (names.some((name) => capabilities[name] === true)) return true;
  if (capabilities.set_agreement_level === true) return true;
  const direction = names.some((name) => name.endsWith("_back")) ? "back" : "forward";
  if (capabilities.agreement?.[direction] === true) return true;
  const levels = capabilities.agreement_levels || capabilities.set_agreement_level;
  if (Array.isArray(levels)) return levels.map(Number).includes(target);
  return levels && typeof levels === "object" && levels[String(target)] === true;
}

function blockersForLevel(row, party, target) {
  const sources = [party.blocking_reasons, row.viewer_party?.blocking_reasons, row.blocking_reasons];
  const movingBack = target < (Number(party.level) || 0);
  const names = movingBack
    ? ["agreement_back", "uncommit", "unreview", `agreement_level_${target}`, `set_agreement_level_${target}`]
    : [`agreement_level_${target}`, `set_agreement_level_${target}`, target === 2 ? "commit" : "review"];
  for (const source of sources) {
    if (!source || typeof source !== "object") continue;
    for (const name of names) {
      const value = source[name];
      if (Array.isArray(value) && value.length) return value;
      if (value) return [value];
    }
  }
  return [];
}

function agreementEvents(row) {
  const direct = row.agreement?.events || [];
  const candidates = direct.length ? direct : row.parties.flatMap((party) => party.agreement_events || []);
  const seen = new Set();
  return candidates.filter((event) => {
    const key = event.uid || `${event.person}:${event.revision}:${event.level}:${event.at}`;
    if (seen.has(key)) return false;
    seen.add(key);
    return true;
  });
}

function blockerLabel(blocker) {
  if (blocker && typeof blocker === "object") {
    const label = blocker.message || blocker.code || blocker.kind || "blocked";
    const details = [
      blocker.person && `Person ${blocker.person}`,
      blocker.party && `party ${blocker.party}`,
      blocker.promise && `promise ${blocker.promise}`,
      blocker.dependency && `dependency ${blocker.dependency}`,
      blocker.upstream && `${blocker.upstream_kind || "upstream"} ${blocker.upstream}`,
      blocker.required_state && `requires ${statusLabel(blocker.required_state)}`,
      blocker.level != null && `level ${blocker.level}`,
      blocker.required != null && `required ${blocker.required}`,
    ].filter(Boolean);
    return [statusLabel(label), ...details].join(" · ");
  }
  return statusLabel(String(blocker || "blocked"));
}

function labeled(text, control) {
  const label = el("label", "inlineField agreementPersonField");
  label.append(el("span", "", text), control);
  return label;
}

function requestId() {
  if (globalThis.crypto?.randomUUID) return `transfer-agreement:${globalThis.crypto.randomUUID()}`;
  return `transfer-agreement:${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

function el(tag, className = "", text = null) {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text != null) node.textContent = String(text);
  return node;
}
