import {
  buildMoveCandidate,
  buildResizeCandidate,
  screenDeltaToWorldDelta,
} from "./grid.js";

function cloneCard(card) {
  return { ...card };
}

function clamp(value, min, max) {
  return Math.min(Math.max(value, min), max);
}

function buildPinnedMoveCandidate(origin, delta) {
  const maxX = Math.max(0, window.innerWidth - origin.width);
  const maxY = Math.max(0, window.innerHeight - origin.height);

  return {
    ...origin,
    x: clamp(origin.x + delta.x, 0, maxX),
    y: clamp(origin.y + delta.y, 0, maxY),
  };
}

function buildPinnedResizeCandidate(origin, handle, delta) {
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

  width = clamp(width, minWidth, Math.max(minWidth, window.innerWidth - x));
  height = clamp(height, minHeight, Math.max(minHeight, window.innerHeight - y));
  x = clamp(x, 0, Math.max(0, window.innerWidth - width));
  y = clamp(y, 0, Math.max(0, window.innerHeight - height));

  return {
    ...origin,
    x,
    y,
    width,
    height,
  };
}

export function attachBoardInteractions({
  boardElement,
  config,
  readCards,
  replaceCards,
  isEditMode,
  getScale,
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

  function onPointerDown(event) {
    if (!isEditMode() || event.button !== 0) {
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

    event.preventDefault();
    event.stopPropagation();

    interaction = {
      pointerId: event.pointerId,
      type: handle ? "resize" : "move",
      handle: handle?.dataset.resizeHandle || null,
      cardId: card.id,
      origin: cloneCard(card),
      current: cloneCard(card),
      baseCards: cards.map(cloneCard),
      startX: event.clientX,
      startY: event.clientY,
      scale: typeof getScale === "function" ? getScale() : 1,
    };

    cardElement.setPointerCapture?.(event.pointerId);
    document.documentElement.classList.add("pointer-locked");
    onInteractionStart(interaction.cardId, interaction.type);

    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", onPointerUp);
    window.addEventListener("pointercancel", onPointerUp);
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

    const candidate =
      interaction.origin.pinned === true
        ? interaction.type === "move"
          ? buildPinnedMoveCandidate(interaction.origin, delta)
          : buildPinnedResizeCandidate(interaction.origin, interaction.handle, delta)
        : interaction.type === "move"
          ? buildMoveCandidate(interaction.origin, delta, config)
          : buildResizeCandidate(
              interaction.origin,
              interaction.handle,
              delta,
              config,
            );

    if (interaction.type === "move" && interaction.origin.pinned !== true) {
      onEdgeTransferPreview?.(resolveEdgeTransferDirection(event.clientX));
    }

    interaction.current = candidate;
    const preview = interaction.baseCards.map((card) =>
      card.id === interaction.cardId ? candidate : cloneCard(card),
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

    const finalLayout = interaction.baseCards.map((card) =>
      card.id === interaction.cardId ? interaction.current : cloneCard(card),
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
