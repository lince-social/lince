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
    cards: normalizeLayout(
      Array.isArray(workspace?.cards) ? workspace.cards : [],
      config,
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

  const workspaces = Array.isArray(parsed.workspaces)
    ? parsed.workspaces
        .map((workspace, index) => normalizeWorkspace(workspace, index, config))
        .filter((workspace) => workspace.id)
    : fallback.workspaces;

  if (!workspaces.length) {
    return fallback;
  }

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
        name: `Area ${state.workspaces.length + 1}`,
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
