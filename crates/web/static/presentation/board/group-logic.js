// Pure group-selection logic for the board marquee. Kept DOM-free so it can
// run under node in cargo tests (see crates/web/src/board_js_tests.rs).

export function newGroupId() {
  if (
    typeof crypto !== "undefined" &&
    typeof crypto.randomUUID === "function"
  ) {
    return `group-${crypto.randomUUID()}`;
  }

  return `group-${Date.now()}-${Math.round(Math.random() * 1_000_000)}`;
}

export function selectMarqueeMembers(cards, rect) {
  // Fully contained, unpinned, non-system world cards only.
  return cards.filter(
    (card) =>
      card.pinned !== true &&
      card.system !== true &&
      card.x >= rect.x &&
      card.y >= rect.y &&
      card.x + card.width <= rect.x + rect.width &&
      card.y + card.height <= rect.y + rect.height,
  );
}

export function resolveMarqueeGroup(cards, rect, idGenerator = newGroupId) {
  const members = selectMarqueeMembers(cards, rect);
  if (!members.length) {
    return null;
  }

  // A marquee that matches a locked group exactly re-activates it as locked.
  const lockedId = members[0].groupId;
  const matchesLocked =
    Boolean(lockedId) &&
    members.every((card) => card.groupId === lockedId) &&
    cards.filter((card) => card.groupId === lockedId).length === members.length;

  return {
    id: matchesLocked ? lockedId : idGenerator(),
    cardIds: members.map((card) => card.id),
    locked: matchesLocked,
  };
}

// --- Nested groups (Stage 8b, Phase 3) -------------------------------------
// A card's group membership is an ordered STACK, outermost -> innermost. Legacy
// cards carry a single `groupId`; treat it as a one-element stack. `groupId` is
// kept in sync as the innermost id so flat-group code keeps working unchanged.

export function groupStackOf(card) {
  if (card && Array.isArray(card.groupIds) && card.groupIds.length) {
    return card.groupIds.slice();
  }
  return card && card.groupId ? [card.groupId] : [];
}

export function innermostGroupId(card) {
  const stack = groupStackOf(card);
  return stack.length ? stack[stack.length - 1] : null;
}

export function outermostGroupId(card) {
  const stack = groupStackOf(card);
  return stack.length ? stack[0] : null;
}

function withStack(card, groupIds) {
  return {
    ...card,
    groupIds,
    groupId: groupIds.length ? groupIds[groupIds.length - 1] : null,
  };
}

// Wrap `memberIds` into a NEW group that becomes their outermost container,
// preserving any inner grouping they already have. Idempotent. Returns cards.
export function wrapInGroup(cards, memberIds, groupId) {
  const members = new Set(memberIds);
  return cards.map((card) => {
    if (!members.has(card.id)) {
      return card;
    }
    const stack = groupStackOf(card);
    if (stack[0] === groupId) {
      return withStack(card, stack);
    }
    return withStack(card, [groupId, ...stack]);
  });
}

// Disband one group by id wherever it sits in each card's stack. Disbanding the
// OUTER group only removes that id; inner groups (deeper in the stack) survive
// as their own groups. Returns updated cards.
export function disbandGroup(cards, groupId) {
  return cards.map((card) => {
    const stack = groupStackOf(card);
    if (!stack.includes(groupId)) {
      return card;
    }
    return withStack(card, stack.filter((id) => id !== groupId));
  });
}

// Do two cards share a group at ANY nesting level? (scopes ABI event delivery.)
export function sharesGroup(a, b) {
  const seen = new Set(groupStackOf(a));
  return groupStackOf(b).some((id) => seen.has(id));
}

export function buildGroupPinUpdates(cards, memberIds, rectById, canvasRect) {
  const members = new Set(memberIds);
  return cards.map((card) => {
    if (!members.has(card.id)) {
      return card;
    }

    const nodeRect = rectById.get(card.id);
    return {
      ...card,
      pinned: true,
      // Pinned cards are not valid group members, so pinning dissolves the
      // group and clears any persisted lock (at every nesting level).
      groupId: null,
      groupIds: [],
      zIndex: 89,
      ...(nodeRect
        ? {
            x: nodeRect.left - canvasRect.left,
            y: nodeRect.top - canvasRect.top,
          }
        : {}),
    };
  });
}
