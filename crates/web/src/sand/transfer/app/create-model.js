export function emptyDraft() {
  return {
    head: "", slug: "", agreement: "full", agreementPct: 100,
    satiation: "none", reserveDefault: "none", requireConfirmation: true,
    parties: [], promises: [], visibility: "hidden", maxProximity: 1,
    parent: "", source: "",
  };
}

export function emptyPromise(party) {
  return { party, record: "", direction: "gives", quantity: 1, windowEnd: "", condition: "", reserveFrom: "" };
}

export function validateDraftStep(draft, index) {
  if (index === 0) {
    if (!draft.head.trim()) return "Enter a transfer title.";
    if (draft.slug && !/^[a-z0-9][a-z0-9-]*(?:\.[a-z0-9][a-z0-9-]*)*$/.test(draft.slug)) return "Use lowercase dot-separated words containing letters, numbers, or dashes.";
    if (draft.agreement === "percentage" && !wholeBetween(draft.agreementPct, 1, 100)) return "Agreement percentage must be a whole number from 1 to 100.";
  }
  if (index === 1) {
    const selected = draft.parties.filter(Boolean);
    if (!selected.length) return "Add at least one person.";
    if (new Set(selected).size !== selected.length) return "Each person can appear only once.";
  }
  if (index === 2) {
    if (!draft.promises.length) return "Add at least one promise.";
    for (const [promiseIndex, promise] of draft.promises.entries()) {
      if (!promise.party || !draft.parties.includes(promise.party)) return `Choose a party for promise ${promiseIndex + 1}.`;
      if (!promise.record) return `Choose a record for promise ${promiseIndex + 1}.`;
      if (!(Number(promise.quantity) > 0) || !Number.isFinite(Number(promise.quantity))) return `Enter a positive quantity for promise ${promiseIndex + 1}.`;
      if (promise.windowEnd && Number.isNaN(new Date(promise.windowEnd).getTime())) return `Enter a valid deadline for promise ${promiseIndex + 1}.`;
      if (promise.windowEnd && new Date(promise.windowEnd).getTime() <= Date.now()) return `Choose a future deadline for promise ${promiseIndex + 1}.`;
    }
    if (draft.agreement === "dependency" && !draft.promises.some((promise) => promise.condition.trim())) return "Dependency agreement requires at least one promise condition.";
  }
  if (index === 3 && draft.visibility === "proximity" && !wholeBetween(draft.maxProximity, 1, Number.MAX_SAFE_INTEGER)) {
    return "Maximum proximity must be a positive whole number.";
  }
  if (index === 3 && draft.satiation === "first_completes" && !draft.source) return "First-sibling completion requires a shared source record.";
  return "";
}

export function toCreateTransferAction(draft) {
  return {
    action: "create-transfer-draft",
    slug: nullable(draft.slug),
    head: draft.head.trim(),
    agreement: draft.agreement,
    agreement_pct: draft.agreement === "percentage" ? Number(draft.agreementPct) : null,
    satiation: draft.satiation,
    parent: nullable(draft.parent),
    source: nullable(draft.source),
    visibility: draft.visibility,
    max_proximity: draft.visibility === "proximity" ? Number(draft.maxProximity) : null,
    reserve_default: draft.reserveDefault,
    require_confirmation: draft.requireConfirmation,
    parties: draft.parties.filter(Boolean),
    promises: draft.promises.map((promise) => ({
      record: promise.record,
      party: promise.party,
      delta: promise.direction === "gives" ? -Number(promise.quantity) : Number(promise.quantity),
      window_end: promise.windowEnd ? new Date(promise.windowEnd).toISOString() : null,
      condition: nullable(promise.condition),
      reserve_from: nullable(promise.reserveFrom),
    })),
  };
}

function nullable(value) { return String(value || "").trim() || null; }
function wholeBetween(value, min, max) { const number = Number(value); return Number.isInteger(number) && number >= min && number <= max; }
