const PRESETS = [
  {
    id: "donation",
    label: "Donation",
    head: "Donation",
    description: "Offer one resource without requiring a named recipient.",
    promises: [{ label: "Donation", direction: "gives", open: true, recordIndex: 0 }],
  },
  {
    id: "sale",
    label: "Sale",
    head: "Sale",
    description: "Give a resource and receive consideration from one invited person.",
    requiresInvitee: true,
    promises: [
      { label: "Resource", direction: "gives", partyRole: "creator", recordIndex: 0 },
      { label: "Consideration", direction: "receives", partyRole: "creator", recordIndex: 1 },
    ],
  },
  {
    id: "assignment",
    label: "Assignment",
    head: "Assignment",
    description: "Assign one responsibility to an invited person.",
    requiresInvitee: true,
    promises: [{ label: "Assigned responsibility", direction: "gives", partyRole: "invitee", recordIndex: 0 }],
  },
  {
    id: "group",
    label: "Group coordination",
    head: "Group coordination",
    description: "Start a shared plan with two editable responsibilities.",
    requiresInvitee: true,
    promises: [
      { label: "Creator responsibility", direction: "gives", partyRole: "creator", recordIndex: 0 },
      { label: "Invitee responsibility", direction: "gives", partyRole: "invitee", recordIndex: 1 },
    ],
  },
  {
    id: "service",
    label: "Service",
    head: "Service offer",
    description: "Offer one service without requiring a named recipient.",
    promises: [{ label: "Service", direction: "gives", open: true, recordIndex: 0 }],
  },
  {
    id: "information",
    label: "Information",
    head: "Information offer",
    description: "Offer one piece of information with explicit receipt confirmation.",
    promises: [{ label: "Information", direction: "gives", open: true, recordIndex: 0 }],
  },
  {
    id: "dependency",
    label: "Dependency plan",
    head: "Dependent transfer",
    description: "Wait for an existing transfer before this commitment can advance.",
    requiresInvitee: true,
    agreement: "dependency",
    promises: [{ label: "Dependent result", direction: "receives", partyRole: "creator", recordIndex: 0 }],
    dependency: true,
  },
  {
    id: "ride",
    label: "Ride",
    head: "Ride",
    description: "Arrange transportation with manual time and place terms.",
    requiresInvitee: true,
    promises: [{ label: "Transportation", direction: "receives", partyRole: "creator", recordIndex: 0 }],
  },
  {
    id: "delivery",
    label: "Delivery",
    head: "Delivery",
    description: "Arrange a manual delivery with explicit time and place terms.",
    requiresInvitee: true,
    promises: [{ label: "Delivery service", direction: "receives", partyRole: "creator", recordIndex: 0 }],
  },
];

export const WORKFLOW_PRESETS = PRESETS.map(({ id, label, description }) => ({
  id,
  label,
  description,
}));

export function workflowPreset(id) {
  return PRESETS.find((preset) => preset.id === String(id || "")) || null;
}

export function applyWorkflowPreset(draft, id, { makePromise, makeDependency, recordUids = [], records = [] }) {
  const preset = workflowPreset(id);
  if (!preset) return null;

  draft.head = preset.head;
  draft.agreement = preset.agreement || "full";
  draft.requireConfirmation = true;
  draft.promises = preset.promises.map((terms) => {
    const promise = makePromise("");
    promise.presetLabel = terms.label;
    promise.direction = terms.direction;
    promise.open = Boolean(terms.open);
    promise.presetPartyRole = terms.partyRole || (terms.open ? "open" : "creator");
    promise.presetRecordIndex = terms.recordIndex;
    promise.record = recordUids[terms.recordIndex] || "";
    const record = records.find((candidate) => candidate.uid === promise.record);
    if (record?.unit) promise.unit = record.unit;
    return promise;
  });
  draft.dependencies = preset.dependency ? [makeDependency()] : [];
  return preset;
}

export function bindPresetParties(draft, preset) {
  if (!preset) return;
  const invitee = draft.invitees.find(Boolean) || "";
  for (const promise of draft.promises) {
    if (promise.presetPartyRole === "open") {
      promise.open = true;
      promise.party = "";
      continue;
    }
    if (promise.presetPartyRole === "creator") promise.party = draft.creator;
    if (promise.presetPartyRole === "invitee") promise.party = invitee;
    promise.open = false;
  }
}

export function validatePresetPeople(draft, preset) {
  if (!preset?.requiresInvitee) return "";
  return draft.invitees.some(Boolean)
    ? ""
    : `${preset.label} requires one invited Person.`;
}
