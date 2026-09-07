export function emptyDraft(viewer = {}) {
  return {
    requestId: requestId("create"),
    creator: viewer.local ? "" : String(viewer.person || ""),
    head: "", slug: "", agreement: "full", agreementPct: 100,
    satiation: "none", reserveDefault: "inherit", requireConfirmation: true,
    defaultPlace: emptyPlace(), invitees: [], participants: [], promises: [], dependencies: [], visibility: "hidden", maxProximity: 1,
    parent: "", source: "", allowEmptyPromises: false,
  };
}

export function draftFromTransfer(row, viewer = {}) {
  const snapshot = row?.revision_evidence?.current?.terms;
  const terms = snapshot?.transfer || row || {};
  const parties = Array.isArray(snapshot?.parties) ? snapshot.parties : Array.isArray(row?.parties) ? row.parties : [];
  const projectedInvitations = Array.isArray(row?.invitations) ? row.invitations : [];
  const invitations = Array.isArray(snapshot?.invitations)
    ? snapshot.invitations.map((signed) => ({
      ...signed,
      ...(projectedInvitations.find((projected) => projected?.uid === signed?.uid) || {}),
    }))
    : projectedInvitations;
  const promises = Array.isArray(snapshot?.promises) ? snapshot.promises : Array.isArray(row?.promises) ? row.promises : [];
  const creator = String(
    terms?.creator_person || terms?.creator_person_uid || row?.creator?.person
      || parties.find((party) => party?.kind === "creator" || party?.role === "creator")?.person_uid
      || parties.find((party) => party?.role === "creator")?.actor
      || (Array.isArray(row?.viewer_roles) && row.viewer_roles.includes("creator") ? viewer.person : "")
      || parties[0]?.person_uid || parties[0]?.actor || "",
  );
  return {
    requestId: requestId("revise"),
    creator,
    head: String(terms?.head || ""),
    slug: String(terms?.slug || ""),
    agreement: String(terms?.agreement_type || "full"),
    agreementPct: Number(terms?.agreement_pct ?? 100),
    satiation: String(terms?.satiation || "none"),
    reserveDefault: String(terms?.reserve_default ?? "inherit"),
    requireConfirmation: Boolean(terms?.require_confirmation),
    defaultPlace: normalizePlace(terms?.default_place),
    invitees: invitations
      .filter((invitation) => invitation?.status === "pending")
      .map((invitation) => String(invitation.addressed_person_uid || invitation.addressed_person || ""))
      .filter(Boolean),
    participants: parties.map((party) => String(party?.person_uid || party?.actor || "")).filter(Boolean),
    promises: promises
      .filter((promise) => promise?.state !== "withdrawn")
      .map(promiseFromProjection),
    dependencies: normalizeDependencies(snapshot?.dependencies || terms?.dependencies || row?.dependencies),
    visibility: String(terms?.visibility || "hidden"),
    maxProximity: Number(terms?.max_proximity ?? 1),
    parent: String(terms?.parent_uid || terms?.parent || ""),
    source: String(terms?.source_uid || terms?.source || ""),
    allowEmptyPromises: true,
  };
}

export function emptyPromise(party = "") {
  return {
    uid: clientUid("p"), party, proposer: "", open: !party, record: "", direction: "gives", quantity: 1,
    unit: "", place: emptyPlace(), windowStart: "", windowEnd: "", condition: "", reserveFrom: "",
    reusePolicy: "duplicate", originalWindowEnd: "",
  };
}

export function emptyDependency() {
  return { uid: "", scope: "transfer", promise: "", upstreamKind: "transfer", upstream: "", requiredState: "kept" };
}

export function validateDraftStep(draft, index) {
  if (index === 0) {
    if (!draft.head.trim()) return "Enter a transfer title.";
    if (draft.slug && !/^[a-z0-9][a-z0-9-]*(?:\.[a-z0-9][a-z0-9-]*)*$/.test(draft.slug)) return "Use lowercase dot-separated words containing letters, numbers, or dashes.";
    if (draft.agreement === "percentage" && !wholeBetween(draft.agreementPct, 1, 100)) return "Agreement percentage must be a whole number from 1 to 100.";
  }
  if (index === 1) {
    if (!draft.creator) return "Select the Person creating this transfer.";
    const invitees = draft.invitees.filter(Boolean);
    if (new Set(invitees).size !== invitees.length) return "Each invitee can appear only once.";
    if (invitees.includes(draft.creator)) return "The creator cannot also be an invitee.";
  }
  if (index === 2) {
    if (!draft.promises.length && !draft.allowEmptyPromises) return "Add at least one promise.";
    if (!draft.promises.length && draft.invitees.length) return "Withdraw every pending invitation before withdrawing the complete draft.";
    for (const [promiseIndex, promise] of draft.promises.entries()) {
      if (!promise.open && (!promise.party || ![draft.creator, ...draft.invitees, ...(draft.participants || [])].includes(promise.party))) return `Choose an accepted participant, an invitee, or OPEN for promise ${promiseIndex + 1}.`;
      if (!promise.record) return `Choose a record for promise ${promiseIndex + 1}.`;
      if (!(Number(promise.quantity) > 0) || !Number.isFinite(Number(promise.quantity))) return `Enter a positive quantity for promise ${promiseIndex + 1}.`;
      if (!validPlace(promise.place)) return `Enter both latitude and longitude for promise ${promiseIndex + 1}, or leave its place empty.`;
      if (!['duplicate', 'consume'].includes(promise.reusePolicy)) return `Choose how promise ${promiseIndex + 1} is reused.`;
      if (promise.windowEnd && Number.isNaN(new Date(promise.windowEnd).getTime())) return `Enter a valid deadline for promise ${promiseIndex + 1}.`;
      if (promise.windowEnd && new Date(promise.windowEnd).getTime() <= Date.now()) {
        const original = promise.originalWindowEnd ? new Date(promise.originalWindowEnd).getTime() : NaN;
        if (new Date(promise.windowEnd).getTime() !== original) return `Choose a future deadline for promise ${promiseIndex + 1}.`;
      }
      if (promise.windowStart && Number.isNaN(new Date(promise.windowStart).getTime())) return `Enter a valid start time for promise ${promiseIndex + 1}.`;
      if (promise.windowStart && promise.windowEnd && new Date(promise.windowStart) >= new Date(promise.windowEnd)) return `Promise ${promiseIndex + 1} must start before its deadline.`;
    }
    if (draft.agreement === "dependency" && !draft.dependencies.length) return "Dependency agreement requires at least one structured dependency.";
  }
  if (index === 3 && draft.visibility === "proximity" && !wholeBetween(draft.maxProximity, 1, Number.MAX_SAFE_INTEGER)) {
    return "Maximum proximity must be a positive whole number.";
  }
  if (index === 3 && draft.satiation === "first_completes" && !draft.source) return "First-sibling completion requires a shared source record.";
  if (index === 3 && !validPlace(draft.defaultPlace)) return "Enter both default-place coordinates, or leave the default place empty.";
  if (index === 3) {
    const promiseUids = new Set(draft.promises.map((promise) => promise.uid).filter(Boolean));
    const dependencyTerms = new Set();
    for (const [dependencyIndex, dependency] of draft.dependencies.entries()) {
      if (!["transfer", "promise"].includes(dependency.scope)) return `Choose a scope for dependency ${dependencyIndex + 1}.`;
      if (dependency.scope === "promise" && !promiseUids.has(dependency.promise)) return `Choose one of this draft's promises for dependency ${dependencyIndex + 1}.`;
      if (!["transfer", "promise"].includes(dependency.upstreamKind)) return `Choose an upstream kind for dependency ${dependencyIndex + 1}.`;
      if (!dependency.upstream) return `Choose an upstream item for dependency ${dependencyIndex + 1}.`;
      if (!dependency.requiredState) return `Choose a required state for dependency ${dependencyIndex + 1}.`;
      const key = [dependency.scope, dependency.promise, dependency.upstreamKind, dependency.upstream, dependency.requiredState].join(":");
      if (dependencyTerms.has(key)) return `Dependency ${dependencyIndex + 1} repeats an existing gate.`;
      dependencyTerms.add(key);
    }
  }
  return "";
}

export function toCreateTransferAction(draft) {
  return { action: "create-transfer-draft", request_id: draft.requestId, creator: draft.creator, ...wireDraft(draft) };
}

export function toReviseTransferAction(transfer, expectedRevision, draft) {
  return {
    action: "revise-transfer-draft",
    transfer,
    expected_revision: Number(expectedRevision),
    request_id: draft.requestId,
    draft: wireDraft(draft),
  };
}

export function toAdoptTransferAction(transfer, draft) {
  return {
    action: "adopt-transfer-draft",
    transfer,
    request_id: draft.requestId,
    draft: wireDraft(draft),
  };
}

export function toCounterofferTransferAction(transfer, expectedRevision, draft, person = "") {
  return {
    action: "counteroffer-transfer",
    transfer,
    expected_revision: Number(expectedRevision),
    request_id: draft.requestId,
    person: nullable(person),
    draft: wireDraft(draft),
  };
}

export function toClaimOpenPromiseAction(transfer, sourcePromise, expectedRevision, draft, person) {
  return {
    action: "claim-open-transfer-promise",
    transfer,
    promise: sourcePromise,
    expected_revision: Number(expectedRevision),
    request_id: draft.requestId,
    person: nullable(person),
    terms: wirePromise(draft.promises[0]),
  };
}

export function comparableDraft(draft) {
  return JSON.stringify(wireDraft(draft));
}

export function renewDraftRequest(draft, kind = "revise") {
  draft.requestId = requestId(kind);
}

function wireDraft(draft) {
  return {
    creator: draft.creator,
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
    default_place: wirePlace(draft.defaultPlace),
    invitees: draft.invitees.filter(Boolean),
    promises: draft.promises.map(wirePromise),
    dependencies: draft.dependencies.map((dependency) => ({
      uid: nullable(dependency.uid),
      scope: dependency.scope,
      promise: dependency.scope === "promise" ? nullable(dependency.promise) : null,
      upstream_kind: dependency.upstreamKind,
      upstream: dependency.upstream,
      required_state: dependency.requiredState || "kept",
    })),
  };
}

function wirePromise(promise) {
  return {
    uid: nullable(promise?.uid),
    record: String(promise?.record || ""),
    party: promise?.open ? null : nullable(promise?.party),
    open: Boolean(promise?.open),
    delta: promise?.direction === "gives" ? -Number(promise?.quantity) : Number(promise?.quantity),
    unit: nullable(promise?.unit),
    place: wirePlace(promise?.place),
    window_start: promise?.windowStart ? new Date(promise.windowStart).toISOString() : null,
    window_end: promise?.windowEnd ? new Date(promise.windowEnd).toISOString() : null,
    condition: nullable(promise?.condition),
    reserve_from: nullable(promise?.reserveFrom),
    reuse_policy: promise?.reusePolicy || "duplicate",
  };
}

function promiseFromProjection(promise) {
  const delta = Number(promise?.delta || 0);
  const open = Boolean(promise?.open || promise?.state === "open");
  const owner = String(promise?.person_uid || promise?.party || promise?.person || "");
  const proposer = String(
    promise?.proposer || promise?.proposer_person || promise?.proposer_person_uid || (open ? owner : ""),
  );
  return {
    uid: String(promise?.uid || ""),
    party: open ? "" : owner,
    proposer,
    open,
    record: String(promise?.record || promise?.record_uid || ""),
    direction: delta < 0 ? "gives" : "receives",
    quantity: Math.abs(delta),
    unit: String(promise?.unit_uid || promise?.unit || promise?.unit_name || ""),
    place: normalizePlace(promise?.location || promise?.place),
    windowStart: localDateTime(promise?.window_start),
    windowEnd: localDateTime(promise?.window_end),
    condition: String(promise?.condition || ""),
    reserveFrom: String(promise?.reserve_from || ""),
    reusePolicy: String(promise?.open_reuse_policy || promise?.reuse_policy || "duplicate"),
    originalWindowEnd: localDateTime(promise?.window_end),
  };
}

function normalizeDependencies(raw) {
  return (Array.isArray(raw) ? raw : []).map((dependency) => ({
    uid: String(dependency?.uid || ""),
    scope: String(dependency?.scope || (dependency?.promise || dependency?.promise_uid ? "promise" : "transfer")),
    promise: String(dependency?.promise || dependency?.promise_uid || ""),
    upstreamKind: String(dependency?.upstream_kind || dependency?.upstreamKind || "transfer"),
    upstream: String(dependency?.upstream || dependency?.upstream_uid || ""),
    requiredState: String(dependency?.required_state || dependency?.requiredState || "kept"),
  }));
}

function localDateTime(value) {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  const local = new Date(date.getTime() - date.getTimezoneOffset() * 60000);
  return local.toISOString().slice(0, 16);
}

function emptyPlace() { return { lat: "", lon: "", address: "" }; }

function normalizePlace(raw) {
  const place = raw && typeof raw === "object" ? raw : {};
  return {
    lat: place.lat == null ? "" : String(place.lat),
    lon: place.lon == null ? "" : String(place.lon),
    address: String(place.address || ""),
  };
}

function wirePlace(place) {
  if (!place || (String(place.lat).trim() === "" && String(place.lon).trim() === "" && !String(place.address || "").trim())) return null;
  const lat = String(place.lat ?? "").trim();
  const lon = String(place.lon ?? "").trim();
  return { lat: lat ? Number(lat) : null, lon: lon ? Number(lon) : null, address: nullable(place.address) };
}

function validPlace(place) {
  const lat = String(place?.lat ?? "").trim();
  const lon = String(place?.lon ?? "").trim();
  const address = String(place?.address || "").trim();
  if (!lat && !lon) return Boolean(address) || !address;
  if (!lat || !lon) return false;
  const latitude = Number(lat);
  const longitude = Number(lon);
  return Number.isFinite(latitude) && latitude >= -90 && latitude <= 90
    && Number.isFinite(longitude) && longitude >= -180 && longitude <= 180;
}

function requestId(kind) {
  if (globalThis.crypto?.randomUUID) return `transfer-${kind}:${globalThis.crypto.randomUUID()}`;
  return `transfer-${kind}:${Date.now().toString(36)}-${Math.random().toString(36).slice(2)}`;
}

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

function nullable(value) { return String(value || "").trim() || null; }
function wholeBetween(value, min, max) { const number = Number(value); return Number.isInteger(number) && number >= min && number <= max; }
