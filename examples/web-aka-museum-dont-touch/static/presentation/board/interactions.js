import {
  MIN_CARD_SIZE,
  buildMoveCandidate,
  buildResizeCandidate,
  clampCard,
  normalizeWorld,
  screenDeltaToWorldDelta,
} from "./grid.js";

function cloneCard(card) {
  return { ...card };
}

function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

function boardBounds(element) {
  const rect = element?.getBoundingClientRect?.();
  return {
    width: Math.max(1, rect?.width || window.innerWidth),
    height: Math.max(1, rect?.height || window.innerHeight),
  };
}

function buildPinnedMoveCandidate(origin, delta, bounds) {
  const maxX = Math.max(0, bounds.width - origin.width);
  const maxY = Math.max(0, bounds.height - origin.height);

  return {
    ...origin,
    x: clamp(origin.x + delta.x, 0, maxX),
    y: clamp(origin.y + delta.y, 0, maxY),
  };
}

function buildPinnedResizeCandidate(origin, handle, delta, bounds) {
  const minWidth = 48;
  const minHeight = 40;
  let x = origin.x;
  let y = origin.y;
  let width = origin.width;
  let height = origin.height;

  if (handle?.includes("e")) {
    width = origin.width + delta.x;
  }
  if (handle?.includes("s")) {
    height = origin.height + delta.y;
  }
  if (handle?.includes("w")) {
    width = origin.width - delta.x;
    x = origin.x + delta.x;
  }
  if (handle?.includes("n")) {
    height = origin.height - delta.y;
    y = origin.y + delta.y;
  }

  width = clamp(width, minWidth, Math.max(minWidth, bounds.width - x));
  height = clamp(height, minHeight, Math.max(minHeight, bounds.height - y));
  x = clamp(x, 0, Math.max(0, bounds.width - width));
  y = clamp(y, 0, Math.max(0, bounds.height - height));

  return {
    ...origin,
    x,
    y,
    width,
    height,
  };
}

export function groupBounds(cards) {
  let left = Infinity;
  let top = Infinity;
  let right = -Infinity;
  let bottom = -Infinity;

  for (const card of cards) {
    left = Math.min(left, card.x);
    top = Math.min(top, card.y);
    right = Math.max(right, card.x + card.width);
    bottom = Math.max(bottom, card.y + card.height);
  }

  return {
    x: left,
    y: top,
    width: Math.max(1, right - left),
    height: Math.max(1, bottom - top),
  };
}

export function buildGroupMoveCandidates(origins, delta, config) {
  const world = normalizeWorld(config.world);
  const box = groupBounds(origins);
  const dx = clamp(delta.x, -box.x, Math.max(-box.x, world.width - (box.x + box.width)));
  const dy = clamp(delta.y, -box.y, Math.max(-box.y, world.height - (box.y + box.height)));

  return origins.map((card) =>
    clampCard(
      {
        ...card,
        x: card.x + dx,
        y: card.y + dy,
      },
      config,
    ),
  );
}

export function buildGroupResizeCandidates(origins, handle, delta, config) {
  const world = normalizeWorld(config.world);
  const base = groupBounds(origins);
  let left = base.x;
  let right = base.x + base.width;
  let top = base.y;
  let bottom = base.y + base.height;

  if (handle?.includes("w")) {
    left += delta.x;
  }
  if (handle?.includes("e")) {
    right += delta.x;
  }
  if (handle?.includes("n")) {
    top += delta.y;
  }
  if (handle?.includes("s")) {
    bottom += delta.y;
  }

  // Stop scaling down once any member would go below the single-card minimum,
  // so members never drift apart from the proportional layout.
  const minScaleX = Math.max(
    ...origins.map((card) => MIN_CARD_SIZE.width / Math.max(1, card.width)),
  );
  const minScaleY = Math.max(
    ...origins.map((card) => MIN_CARD_SIZE.height / Math.max(1, card.height)),
  );
  const minWidth = base.width * Math.min(1, minScaleX);
  const minHeight = base.height * Math.min(1, minScaleY);

  const width = clamp(Math.abs(right - left), minWidth, world.width);
  const height = clamp(Math.abs(bottom - top), minHeight, world.height);
  if (handle?.includes("w")) {
    left = right - width;
  } else {
    right = left + width;
  }
  if (handle?.includes("n")) {
    top = bottom - height;
  } else {
    bottom = top + height;
  }
  left = clamp(left, 0, Math.max(0, world.width - width));
  top = clamp(top, 0, Math.max(0, world.height - height));

  const scaleX = width / base.width;
  const scaleY = height / base.height;

  return origins.map((card) =>
    clampCard(
      {
        ...card,
        x: left + (card.x - base.x) * scaleX,
        y: top + (card.y - base.y) * scaleY,
        width: card.width * scaleX,
        height: card.height * scaleY,
      },
      config,
    ),
  );
}

export function attachBoardInteractions({
  boardElement,
  config,
  readCards,
  replaceCards,
  isEditMode,
  isCardEditable,
  getScale,
  resolveCardGroupIds,
  getActiveGroupCardIds,
  onInteractionStart,
  onInteractionEnd,
  onEdgeTransferPreview,
  onEdgeTransfer,
}) {
  let interaction = null;
  const EDGE_TRANSFER_THRESHOLD = 48;

  function cleanup() {
    window.removeEventListener("pointermove", onPointerMove);
    window.removeEventListener("pointerup", onPointerUp);
    window.removeEventListener("pointercancel", onPointerUp);
    document.documentElement.classList.remove("pointer-locked");
    onEdgeTransferPreview?.(0);
  }

  function resolveEdgeTransferDirection(clientX) {
    if (clientX <= EDGE_TRANSFER_THRESHOLD) {
      return -1;
    }

    if (clientX >= window.innerWidth - EDGE_TRANSFER_THRESHOLD) {
      return 1;
    }

    return 0;
  }

  function beginInteraction(event, fields) {
    interaction = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      scale: typeof getScale === "function" ? getScale() : 1,
      ...fields,
    };

    document.documentElement.classList.add("pointer-locked");
    onInteractionStart(interaction.cardId, interaction.type);

    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
    window.addEventListener("pointercancel", onPointerUp);
  }

  function onPointerDown(event) {
    if (!isEditMode() || event.button !== 0) {
      return;
    }

    const groupHandle = event.target.closest("[data-group-resize-handle]");
    if (groupHandle) {
      const memberIds =
        typeof getActiveGroupCardIds === "function"
          ? getActiveGroupCardIds()
          : null;
      const cards = readCards();
      const members = Array.isArray(memberIds)
        ? cards.filter(
            (entry) => memberIds.includes(entry.id) && entry.pinned !== true,
          )
        : [];
      if (!members.length) {
        return;
      }

      event.preventDefault();
      event.stopPropagation();
      beginInteraction(event, {
        type: "group-resize",
        handle: groupHandle.dataset.groupResizeHandle || "se",
        cardId: members[0].id,
        cardIds: members.map((entry) => entry.id),
        originCards: members.map(cloneCard),
        origin: cloneCard(members[0]),
        currents: members.map(cloneCard),
        current: cloneCard(members[0]),
        baseCards: cards.map(cloneCard),
      });
      return;
    }

    const handle = event.target.closest("[data-resize-handle]");
    const cardElement = event.target.closest("[data-card-id]");

    if (!cardElement) {
      return;
    }

    const isInteractiveElement = event.target.closest(
      "a, button, input, select, textarea",
    );
    if (isInteractiveElement && !handle) {
      return;
    }

    const cards = readCards();
    const card = cards.find((entry) => entry.id === cardElement.dataset.cardId);

    if (!card) {
      return;
    }

    if (typeof isCardEditable === "function" && !isCardEditable(card)) {
      return;
    }

    const groupIds =
      typeof resolveCardGroupIds === "function"
        ? resolveCardGroupIds(card)
        : null;
    const members =
      Array.isArray(groupIds) && groupIds.length > 1 && card.pinned !== true
        ? cards.filter(
            (entry) =>
              groupIds.includes(entry.id) &&
              entry.pinned !== true &&
              entry.system !== true,
          )
        : null;

    event.preventDefault();
    event.stopPropagation();

    if (members && members.length > 1) {
      beginInteraction(event, {
        type: handle ? "group-resize" : "group-move",
        handle: handle?.dataset.resizeHandle || null,
        cardId: card.id,
        cardIds: members.map((entry) => entry.id),
        originCards: members.map(cloneCard),
        origin: cloneCard(card),
        currents: members.map(cloneCard),
        current: cloneCard(card),
        baseCards: cards.map(cloneCard),
      });
    } else {
      beginInteraction(event, {
        type: handle ? "resize" : "move",
        handle: handle?.dataset.resizeHandle || null,
        cardId: card.id,
        cardIds: [card.id],
        originCards: [cloneCard(card)],
        origin: cloneCard(card),
        currents: [cloneCard(card)],
        current: cloneCard(card),
        baseCards: cards.map(cloneCard),
      });
    }

    cardElement.setPointerCapture?.(event.pointerId);
  }

  function onPointerMove(event) {
    if (!interaction || event.pointerId !== interaction.pointerId) {
      return;
    }

    event.preventDefault();

    const screenDelta = {
      x: event.clientX - interaction.startX,
      y: event.clientY - interaction.startY,
    };
    const delta =
      interaction.origin.pinned === true
        ? screenDelta
        : screenDeltaToWorldDelta(screenDelta.x, screenDelta.y, interaction.scale);

    let candidates;
    if (interaction.type === "group-move") {
      candidates = buildGroupMoveCandidates(
        interaction.originCards,
        delta,
        config,
      );
    } else if (interaction.type === "group-resize") {
      candidates = buildGroupResizeCandidates(
        interaction.originCards,
        interaction.handle,
        delta,
        config,
      );
    } else {
      const candidate =
        interaction.origin.pinned === true
          ? interaction.type === "move"
            ? buildPinnedMoveCandidate(interaction.origin, delta, boardBounds(boardElement))
            : buildPinnedResizeCandidate(
                interaction.origin,
                interaction.handle,
                delta,
                boardBounds(boardElement),
              )
          : interaction.type === "move"
            ? buildMoveCandidate(interaction.origin, delta, config)
            : buildResizeCandidate(
                interaction.origin,
                interaction.handle,
                delta,
                config,
              );
      interaction.current = candidate;
      candidates = [candidate];
    }

    if (interaction.type === "move" && interaction.origin.pinned !== true) {
      onEdgeTransferPreview?.(resolveEdgeTransferDirection(event.clientX));
    }

    interaction.currents = candidates;
    const candidateById = new Map(
      candidates.map((candidate) => [candidate.id, candidate]),
    );
    const preview = interaction.baseCards.map(
      (card) => candidateById.get(card.id) || cloneCard(card),
    );

    replaceCards(preview, { persist: false });
  }

  function onPointerUp(event) {
    if (!interaction) {
      return;
    }

    if (event.pointerId !== undefined && event.pointerId !== interaction.pointerId) {
      return;
    }

    const transferDirection =
      interaction.type === "move" && interaction.origin.pinned !== true
        ? resolveEdgeTransferDirection(event.clientX)
        : 0;
    if (transferDirection) {
      const transferred = onEdgeTransfer?.(interaction.current, transferDirection);
      if (transferred) {
        onInteractionEnd();
        interaction = null;
        cleanup();
        return;
      }
    }

    const candidateById = new Map(
      interaction.currents.map((candidate) => [candidate.id, candidate]),
    );
    const finalLayout = interaction.baseCards.map(
      (card) => candidateById.get(card.id) || cloneCard(card),
    );

    replaceCards(finalLayout, { persist: true });
    onInteractionEnd();
    interaction = null;
    cleanup();
  }

  boardElement.addEventListener("pointerdown", onPointerDown);

  return () => {
    boardElement.removeEventListener("pointerdown", onPointerDown);
    cleanup();
  };
}
