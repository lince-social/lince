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
      // group and clears any persisted lock.
      groupId: null,
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
