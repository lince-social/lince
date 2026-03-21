import {
  buildMoveCandidate,
  buildResizeCandidate,
  deltaToGrid,
  isAreaAvailable,
  measureBoard,
} from "./grid.js";

function cloneCard(card) {
  return { ...card };
}

export function attachBoardInteractions({
  boardElement,
  config,
  readCards,
  replaceCards,
  isEditMode,
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

    const isInteractiveElement = event.target.closest("a, button, input, select, textarea");
    if (isInteractiveElement && !handle) {
      return;
    }

    const cards = readCards();
    const card = cards.find((entry) => entry.id === cardElement.dataset.cardId);

    if (!card) {
      return;
    }

    event.preventDefault();

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
      metrics: measureBoard(boardElement, config),
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

    const delta = deltaToGrid(
      event.clientX - interaction.startX,
      event.clientY - interaction.startY,
      interaction.metrics,
    );

    const candidate =
      interaction.type === "move"
        ? buildMoveCandidate(interaction.origin, delta, config)
        : buildResizeCandidate(interaction.origin, interaction.handle, delta, config);

    if (interaction.type === "move") {
      onEdgeTransferPreview?.(resolveEdgeTransferDirection(event.clientX));
    }

    if (
      !isAreaAvailable(interaction.baseCards, candidate, config, interaction.cardId)
    ) {
      return;
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
      interaction.type === "move" ? resolveEdgeTransferDirection(event.clientX) : 0;
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
