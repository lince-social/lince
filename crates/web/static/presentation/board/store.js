import {
  arrangeCardsInCircle,
  applyDensity,
  clampDensityLevel,
  defaultCamera,
  findOpenPosition,
  normalizeLayout,
  normalizeWorld,
  sanitizeCamera,
  sanitizeCard,
} from "./grid.js";

const DEFAULT_CARD_SIZE = { width: 640, height: 420 };

function cloneJsonValue(value, fallback = {}) {
  try {
    if (value == null) {
      return fallback;
    }

    return JSON.parse(JSON.stringify(value));
  } catch {
    return fallback;
  }
}

function cloneCard(card) {
  return {
    ...card,
    widgetState: cloneJsonValue(card?.widgetState, {}),
  };
}

function cloneCards(cards) {
  return cards.map(cloneCard);
}

function cloneCamera(camera) {
  return {
    x: Number(camera?.x) || 0,
    y: Number(camera?.y) || 0,
    scale: Number(camera?.scale) || 1,
  };
}

function cloneWorkspace(workspace) {
  return {
    ...workspace,
    camera: cloneCamera(workspace.camera),
    cards: cloneCards(workspace.cards),
  };
}

function cloneWorkspaces(workspaces) {
  return workspaces.map(cloneWorkspace);
}

function cloneShellCards(workspaces) {
  const sourceWorkspace = workspaces.find((workspace) =>
    workspace.cards.some((card) => card.pinned === true && card.system === true),
  );
  if (!sourceWorkspace) {
    return [];
  }

  return sourceWorkspace.cards
    .filter((card) => card.pinned === true && card.system === true)
    .map(cloneCard);
}

function ensureShellPins(workspaces, config) {
  const shellCards = cloneShellCards(workspaces);
  if (!shellCards.length) {
    return workspaces;
  }

  return workspaces.map((workspace) => {
    const existingIds = new Set(
      workspace.cards
        .filter((card) => card.pinned === true && card.system === true)
        .map((card) => card.id),
    );
    const missingShellCards = shellCards.filter(
      (card) => !existingIds.has(card.id),
    );

    if (!missingShellCards.length) {
      return workspace;
    }

    return {
      ...workspace,
      cards: layoutShellPins(
        normalizeLayout([...workspace.cards, ...missingShellCards], config),
      ),
    };
  });
}

function layoutShellPins(cards) {
  const viewportWidth =
    window.visualViewport?.width || document.documentElement.clientWidth || window.innerWidth || 1360;
  const viewportHeight =
    window.visualViewport?.height || document.documentElement.clientHeight || window.innerHeight || 760;
  const gap = 10;
  const padding = 20;
  const top = 16;
  const editWidth = 40;
  const notificationsWidth = 40;
  const workspaceWidth = 72;
  const minimumOperationWidth = 260;
  const preferredOperationWidth = 420;
  const brandWidth = 112;
  const reservedRight = editWidth + notificationsWidth + workspaceWidth + gap * 3 + padding;
  const availableOperationWidth =
    viewportWidth - padding - brandWidth - gap - reservedRight;
  const operationWidth = Math.max(
    minimumOperationWidth,
    Math.min(preferredOperationWidth, availableOperationWidth),
  );
  const editX = Math.max(padding, viewportWidth - padding - editWidth);
  const notificationsX = editX - gap - notificationsWidth;
  const workspaceX = notificationsX - gap - workspaceWidth;
  const operationX = workspaceX - gap - operationWidth;

  const layoutById = {
    "shell-logo": {
      x: padding,
      y: 28,
      width: brandWidth,
      height: 32,
    },
    "shell-operation": {
      x: Math.max(padding + brandWidth + gap, operationX),
      y: top,
      width: operationWidth,
      height: 40,
    },
    "shell-workspaces": {
      x: workspaceX,
      y: top,
      width: workspaceWidth,
      height: 40,
    },
    "shell-notifications": {
      x: notificationsX,
      y: top,
      width: notificationsWidth,
      height: 40,
    },
    "shell-edit": {
      x: editX,
      y: top,
      width: editWidth,
      height: 40,
    },
    "shell-zoom": {
      x: padding,
      y: Math.max(80, viewportHeight - 72),
      width: 242,
      height: 52,
    },
  };

  return cards.map((card) =>
    card?.system === true && layoutById[card.id]
      ? {
          ...card,
          ...layoutById[card.id],
        }
      : card,
  );
}

function nextEntityId(prefix) {
  if (
    typeof crypto !== "undefined" &&
    typeof crypto.randomUUID === "function"
  ) {
    return `${prefix}-${crypto.randomUUID()}`;
  }

  return `${prefix}-${Date.now()}-${Math.round(Math.random() * 1_000)}`;
}

function normalizeWorkspace(workspace, index, config) {
  const world = normalizeWorld(config.world);
  return {
    id: String(workspace?.id || `space-${index + 1}`),
    name: String(workspace?.name || `Area ${index + 1}`),
    camera: sanitizeCamera(workspace?.camera, world),
    cards: layoutShellPins(
      normalizeLayout(
        Array.isArray(workspace?.cards) ? workspace.cards : [],
        config,
      ),
    ),
  };
}

function createFallbackState(seedCards, config) {
  applyDensity(config, config.density);

  return {
    schemaVersion: 2,
    density: config.density,
    globalStreamsEnabled: true,
    world: normalizeWorld(config.world),
    activeWorkspaceId: "space-1",
    workspaces: [
      {
        id: "space-1",
        name: "Area 1",
        camera: defaultCamera(config.world),
        cards: normalizeLayout(seedCards, config),
      },
      {
        id: "space-2",
        name: "Area 2",
        camera: defaultCamera(config.world),
        cards: [],
      },
    ],
  };
}

function loadState(initialBoardState, seedCards, config) {
  const parsed =
    initialBoardState && typeof initialBoardState === "object"
      ? initialBoardState
      : null;

  if (parsed?.world) {
    config.world = normalizeWorld(parsed.world);
  }
  applyDensity(config, clampDensityLevel(parsed?.density ?? config.density));

  const fallback = createFallbackState(seedCards, config);
  if (!parsed) {
    return fallback;
  }

  let workspaces = Array.isArray(parsed.workspaces)
    ? parsed.workspaces
        .map((workspace, index) => normalizeWorkspace(workspace, index, config))
        .filter((workspace) => workspace.id)
    : fallback.workspaces;

  if (!workspaces.length) {
    return fallback;
  }
  workspaces = ensureShellPins(workspaces, config);

  const activeWorkspaceId = workspaces.some(
    (workspace) => workspace.id === parsed.activeWorkspaceId,
  )
    ? parsed.activeWorkspaceId
    : workspaces[0].id;

  return {
    schemaVersion: Number(parsed.schemaVersion) || 2,
    density: config.density,
    globalStreamsEnabled: parsed.globalStreamsEnabled !== false,
    world: normalizeWorld(config.world),
    activeWorkspaceId,
    workspaces,
  };
}

function exportCard(card) {
  const {
    id: cardId,
    kind,
    title,
    description,
    text,
    html,
    author,
    permissions,
    packageName,
    requiresServer,
    serverId,
    viewId,
    streamsEnabled,
    widgetState,
    x,
    y,
    width,
    height,
    pinned,
    system,
    zIndex,
    groupId,
    abiListen,
  } = card;

  return {
    id: cardId,
    kind,
    title,
    description,
    text,
    html,
    author,
    permissions,
    packageName,
    requiresServer,
    serverId,
    viewId,
    streamsEnabled,
    widgetState: cloneJsonValue(widgetState, {}),
    x,
    y,
    width,
    height,
    pinned: pinned === true,
    system: system === true,
    zIndex: Number(zIndex) || (pinned ? 50 : 1),
    groupId: groupId || null,
    abiListen: Array.isArray(abiListen)
      ? abiListen.map((topic) => String(topic)).filter(Boolean)
      : [],
  };
}

function exportState(state) {
  return {
    schemaVersion: 2,
    density: state.density,
    globalStreamsEnabled: state.globalStreamsEnabled !== false,
    world: normalizeWorld(state.world),
    activeWorkspaceId: state.activeWorkspaceId,
    workspaces: state.workspaces.map(({ id, name, camera, cards }) => ({
      id,
      name,
      camera: cloneCamera(camera),
      cards: cards.map(exportCard),
    })),
  };
}

function cardTemplate(index, centerPoint, config) {
  const position = findOpenPosition(
    [],
    DEFAULT_CARD_SIZE,
    config,
    centerPoint,
  );

  return {
    id: nextEntityId("card"),
    kind: "text",
    title: `Bloco ${index}`,
    description:
      "Novo card criado para texto curto, notas de contexto ou conteudo inicial.",
    text: "Novo card criado para texto curto, notas de contexto ou conteudo inicial de um widget futuro.",
    html: "",
    author: "",
    permissions: [],
    packageName: "",
    requiresServer: false,
    serverId: "",
    viewId: null,
    streamsEnabled: true,
    widgetState: {},
    pinned: false,
    system: false,
    zIndex: 1,
    groupId: null,
    abiListen: [],
    ...position,
  };
}

export function createBoardStore({
  seedCards,
  initialBoardState,
  config,
  persistState,
}) {
  let state = loadState(initialBoardState, seedCards, config);
  const listeners = new Set();
  let persistSequence = 0;

  function getWorkspaceIndexById(workspaceId) {
    return state.workspaces.findIndex(
      (workspace) => workspace.id === workspaceId,
    );
  }

  function getWorkspaceById(workspaceId) {
    const index = getWorkspaceIndexById(workspaceId);
    return index >= 0 ? state.workspaces[index] : null;
  }

  function getActiveWorkspace() {
    const index = getWorkspaceIndexById(state.activeWorkspaceId);
    return index >= 0 ? state.workspaces[index] : state.workspaces[0];
  }

  function buildSnapshot() {
    const activeWorkspace = getActiveWorkspace();
    const world = normalizeWorld(state.world);

    return {
      density: state.density,
      activeWorkspaceId: activeWorkspace.id,
      workspaces: cloneWorkspaces(state.workspaces),
      activeWorkspace: cloneWorkspace(activeWorkspace),
      activeCamera: cloneCamera(activeWorkspace.camera),
      cards: cloneCards(activeWorkspace.cards),
      boardState: exportState(state),
      layout: {
        world,
        density: config.density,
        densityLabel: config.densityLabel,
      },
      globalStreamsEnabled: state.globalStreamsEnabled !== false,
    };
  }

  function notify() {
    const snapshot = buildSnapshot();
    listeners.forEach((listener) => listener(snapshot));
  }

  function persist() {
    if (typeof persistState !== "function") {
      return;
    }

    const currentSequence = ++persistSequence;
    const payload = exportState(state);
    Promise.resolve(persistState(payload)).catch((error) => {
      if (currentSequence !== persistSequence) {
        return;
      }

      console.error("Failed to persist board state", error);
    });
  }

  function commit(options = {}) {
    if (options.persist !== false) {
      persist();
    }

    if (options.notify !== false) {
      notify();
    }
    return buildSnapshot();
  }

  function replaceActiveWorkspaceCards(nextCards, options = {}) {
    const activeWorkspace = getActiveWorkspace();
    activeWorkspace.cards = normalizeLayout(nextCards, config);
    return commit(options);
  }

  function normalizeAllWorkspaces() {
    state.world = normalizeWorld(config.world);
    state.workspaces = state.workspaces.map((workspace, index) =>
      normalizeWorkspace(workspace, index, config),
    );
  }

  return {
    getCards() {
      return cloneCards(getActiveWorkspace().cards);
    },
    getSnapshot() {
      return buildSnapshot();
    },
    subscribe(listener) {
      listeners.add(listener);
      listener(buildSnapshot());

      return () => {
        listeners.delete(listener);
      };
    },
    replaceCards(nextCards, options) {
      return replaceActiveWorkspaceCards(nextCards, options);
    },
    relayoutShellPins(options = {}) {
      for (const workspace of state.workspaces) {
        workspace.cards = layoutShellPins(workspace.cards);
      }
      return commit(options);
    },
    replaceState(nextState, options = {}) {
      state = loadState(nextState, [], config);
      return commit({
        ...options,
        persist: options.persist ?? false,
      });
    },
    appendWorkspace(workspaceLike, options = {}) {
      const workspace = normalizeWorkspace(
        {
          id: String(workspaceLike?.id || nextEntityId("space")),
          name: String(
            workspaceLike?.name || `Area ${state.workspaces.length + 1}`,
          ),
          camera: workspaceLike?.camera || defaultCamera(config.world),
          cards: Array.isArray(workspaceLike?.cards) ? workspaceLike.cards : [],
        },
        state.workspaces.length,
        config,
      );

      state.workspaces = [...state.workspaces, workspace];
      if (options.activate !== false) {
        state.activeWorkspaceId = workspace.id;
      }

      commit(options);
      return cloneWorkspace(workspace);
    },
    addCard(options = {}) {
      const activeWorkspace = getActiveWorkspace();
      const created = cardTemplate(
        activeWorkspace.cards.length + 1,
        options.center,
        config,
      );

      replaceActiveWorkspaceCards([...activeWorkspace.cards, created]);
      return created;
    },
    addImportedCard(cardDefinition, options = {}) {
      const activeWorkspace = getActiveWorkspace();
      const requestedSize = {
        width:
          Number(cardDefinition?.width) ||
          (Number(cardDefinition?.w) || 0) * 180 ||
          DEFAULT_CARD_SIZE.width,
        height:
          Number(cardDefinition?.height) ||
          (Number(cardDefinition?.h) || 0) * 160 ||
          DEFAULT_CARD_SIZE.height,
      };
      const position = findOpenPosition(
        activeWorkspace.cards,
        requestedSize,
        config,
        options.center,
      );

      const created = sanitizeCard(
        {
          id: nextEntityId("card"),
          kind: "package",
          title: String(cardDefinition?.title || "Card importado"),
          description: String(
            cardDefinition?.description || "Card importado de um widget HTML.",
          ),
          text: "",
          html: String(cardDefinition?.html || ""),
          author: String(cardDefinition?.author || ""),
          permissions: Array.isArray(cardDefinition?.permissions)
            ? cardDefinition.permissions.map((permission) => String(permission))
            : [],
          packageName: String(cardDefinition?.packageName || ""),
          requiresServer: cardDefinition?.requiresServer === true,
          serverId: String(cardDefinition?.serverId || ""),
          viewId:
            cardDefinition?.viewId == null
              ? null
              : Number(cardDefinition.viewId) || null,
          streamsEnabled: cardDefinition?.streamsEnabled !== false,
          widgetState: cloneJsonValue(cardDefinition?.widgetState, {}),
          abiListen: Array.isArray(cardDefinition?.abiListen)
            ? cardDefinition.abiListen
            : [],
          ...requestedSize,
          ...position,
        },
        activeWorkspace.cards.length,
        config,
      );

      replaceActiveWorkspaceCards([...activeWorkspace.cards, created]);
      return created;
    },
    removeCard(cardId) {
      const activeWorkspace = getActiveWorkspace();
      const nextCards = activeWorkspace.cards.filter(
        (card) => card.id !== cardId,
      );

      if (nextCards.length === activeWorkspace.cards.length) {
        return null;
      }

      replaceActiveWorkspaceCards(nextCards);
      return cardId;
    },
    moveCardToAdjacentWorkspace(cardId, direction, options = {}) {
      const step = direction > 0 ? 1 : direction < 0 ? -1 : 0;
      if (!step) {
        return null;
      }

      const sourceIndex = getWorkspaceIndexById(state.activeWorkspaceId);
      const targetIndex = sourceIndex + step;
      if (
        sourceIndex < 0 ||
        targetIndex < 0 ||
        targetIndex >= state.workspaces.length
      ) {
        return null;
      }

      const sourceWorkspace = state.workspaces[sourceIndex];
      const targetWorkspace = state.workspaces[targetIndex];
      const card = sourceWorkspace.cards.find((entry) => entry.id === cardId);
      if (!card || card.pinned === true) {
        return null;
      }

      const position = findOpenPosition(
        targetWorkspace.cards,
        {
          width: Number(card.width) || DEFAULT_CARD_SIZE.width,
          height: Number(card.height) || DEFAULT_CARD_SIZE.height,
        },
        config,
        options.center,
      );

      sourceWorkspace.cards = sourceWorkspace.cards.filter(
        (entry) => entry.id !== cardId,
      );
      targetWorkspace.cards = normalizeLayout(
        [
          ...targetWorkspace.cards,
          {
            ...card,
            ...position,
          },
        ],
        config,
      );
      state.activeWorkspaceId = targetWorkspace.id;
      commit(options);

      return {
        cardId,
        direction: step,
        targetWorkspaceId: targetWorkspace.id,
      };
    },
    addWorkspace() {
      const shellCards = cloneShellCards(state.workspaces);
      const workspace = {
        id: nextEntityId("space"),
        name: String(state.workspaces.length + 1).padStart(2, "0"),
        camera: defaultCamera(config.world),
        cards: normalizeLayout(shellCards, config),
      };

      state.workspaces = [...state.workspaces, workspace];
      state.activeWorkspaceId = workspace.id;
      commit();

      return cloneWorkspace(workspace);
    },
    removeWorkspace(workspaceId) {
      const currentIndex = getWorkspaceIndexById(workspaceId);
      if (currentIndex < 0 || state.workspaces.length <= 1) {
        return null;
      }

      const nextWorkspaces = state.workspaces.filter(
        (workspace) => workspace.id !== workspaceId,
      );
      const fallbackIndex = Math.max(
        0,
        Math.min(currentIndex, nextWorkspaces.length - 1),
      );

      state.workspaces = nextWorkspaces;

      if (state.activeWorkspaceId === workspaceId) {
        state.activeWorkspaceId = nextWorkspaces[fallbackIndex].id;
      }

      commit();
      return cloneWorkspaces(nextWorkspaces);
    },
    renameWorkspace(workspaceId, name) {
      const workspace = getWorkspaceById(workspaceId);
      const nextName = String(name || "").trim();
      if (!workspace || !nextName) {
        return buildSnapshot();
      }

      workspace.name = nextName;
      return commit();
    },
    switchWorkspace(workspaceId) {
      if (!state.workspaces.some((workspace) => workspace.id === workspaceId)) {
        return buildSnapshot();
      }

      state.activeWorkspaceId = workspaceId;
      return commit();
    },
    cycleWorkspace(direction) {
      const currentIndex = getWorkspaceIndexById(getActiveWorkspace().id);
      const nextIndex =
        (currentIndex + direction + state.workspaces.length) %
        state.workspaces.length;

      state.activeWorkspaceId = state.workspaces[nextIndex].id;
      return commit();
    },
    jumpToWorkspace(index) {
      if (index < 0 || index >= state.workspaces.length) {
        return buildSnapshot();
      }

      state.activeWorkspaceId = state.workspaces[index].id;
      return commit();
    },
    updateDensity(level) {
      applyDensity(config, clampDensityLevel(level));
      state.density = config.density;
      state.world = normalizeWorld(config.world);
      normalizeAllWorkspaces();
      return commit();
    },
    updateActiveCamera(camera, options = {}) {
      const activeWorkspace = getActiveWorkspace();
      activeWorkspace.camera = sanitizeCamera(camera, state.world);
      return commit(options);
    },
    reorganizeActiveCards(centerPoint, options = {}) {
      const activeWorkspace = getActiveWorkspace();
      const pinnedCards = activeWorkspace.cards.filter((card) => card.pinned === true);
      const worldCards = activeWorkspace.cards.filter((card) => card.pinned !== true);
      activeWorkspace.cards = [
        ...pinnedCards,
        ...arrangeCardsInCircle(worldCards, centerPoint, config),
      ];
      return commit(options);
    },
    setGlobalStreamsEnabled(enabled, options = {}) {
      state.globalStreamsEnabled = Boolean(enabled);
      return commit(options);
    },
    setCardsGroup(cardIds, groupId, options = {}) {
      const ids = new Set(cardIds);
      const activeWorkspace = getActiveWorkspace();
      let changed = false;
      activeWorkspace.cards = activeWorkspace.cards.map((card) => {
        if (!ids.has(card.id)) {
          return card;
        }
        changed = true;
        return { ...card, groupId: groupId || null };
      });
      if (!changed) {
        return null;
      }
      return commit(options);
    },
    reorderCard(cardId, direction, options = {}) {
      for (const workspace of state.workspaces) {
        const card = workspace.cards.find((entry) => entry.id === cardId);
        if (!card) {
          continue;
        }
        if (card.system === true) {
          return null;
        }

        // Reorder only within the card's layer band: unpinned cards occupy
        // z 1..49, pinned cards 50..89; system shell UI (90+) stays above.
        const layer = workspace.cards
          .filter(
            (entry) => entry.pinned === card.pinned && entry.system !== true,
          )
          .sort((a, b) => (Number(a.zIndex) || 1) - (Number(b.zIndex) || 1));

        const from = layer.indexOf(card);
        const to =
          direction === "front"
            ? layer.length - 1
            : direction === "back"
              ? 0
              : direction === "forward"
                ? Math.min(from + 1, layer.length - 1)
                : direction === "backward"
                  ? Math.max(from - 1, 0)
                  : from;
        if (to === from) {
          return null;
        }

        layer.splice(from, 1);
        layer.splice(to, 0, card);

        const base = card.pinned === true ? 50 : 1;
        const cap = card.pinned === true ? 90 : 50;
        layer.forEach((entry, index) => {
          entry.zIndex = Math.min(base + index, cap - 1);
        });
        commit(options);
        return cloneCard(card);
      }

      return null;
    },
    updateCard(cardId, updater, options = {}) {
      const mutator = typeof updater === "function" ? updater : null;
      if (!mutator) {
        return null;
      }

      for (const workspace of state.workspaces) {
        const index = workspace.cards.findIndex((card) => card.id === cardId);
        if (index < 0) {
          continue;
        }

        const nextCard = mutator(cloneCard(workspace.cards[index]));
        if (!nextCard) {
          return null;
        }

        const nextCards = workspace.cards.slice();
        nextCards[index] = nextCard;
        workspace.cards = normalizeLayout(nextCards, config);
        commit(options);
        return cloneCard(
          workspace.cards.find((card) => card.id === cardId) || nextCard,
        );
      }

      return null;
    },
  };
}
