import {
  applyDensity,
  clampDensityLevel,
  findOpenPosition,
  normalizeLayout,
} from "./grid.js";

const DEFAULT_CARD_SIZE = { w: 3, h: 2 };

function cloneCard(card) {
  return { ...card };
}

function cloneCards(cards) {
  return cards.map(cloneCard);
}

function cloneWorkspace(workspace) {
  return {
    ...workspace,
    cards: cloneCards(workspace.cards),
  };
}

function cloneWorkspaces(workspaces) {
  return workspaces.map(cloneWorkspace);
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
  return {
    id: String(workspace?.id || `space-${index + 1}`),
    name: String(workspace?.name || `Area ${index + 1}`),
    cards: normalizeLayout(
      Array.isArray(workspace?.cards) ? workspace.cards : [],
      config,
    ),
  };
}

function createFallbackState(seedCards, config) {
  applyDensity(config, config.density);

  return {
    density: config.density,
    activeWorkspaceId: "space-1",
    workspaces: [
      {
        id: "space-1",
        name: "Area 1",
        cards: normalizeLayout(seedCards, config),
      },
      {
        id: "space-2",
        name: "Area 2",
        cards: [],
      },
    ],
  };
}

function loadState(initialBoardState, seedCards, config) {
  const fallback = createFallbackState(seedCards, config);
  const parsed =
    initialBoardState && typeof initialBoardState === "object"
      ? initialBoardState
      : null;

  if (!parsed) {
    return fallback;
  }

  applyDensity(config, clampDensityLevel(parsed.density));

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
    density: config.density,
    activeWorkspaceId,
    workspaces,
  };
}

function exportState(state) {
  return {
    density: state.density,
    activeWorkspaceId: state.activeWorkspaceId,
    workspaces: state.workspaces.map(({ id, name, cards }) => ({
      id,
      name,
      cards: cards.map(
        ({
          id: cardId,
          kind,
          title,
          description,
          text,
          html,
          author,
          permissions,
          packageName,
          serverId,
          viewId,
          x,
          y,
          w,
          h,
        }) => ({
          id: cardId,
          kind,
          title,
          description,
          text,
          html,
          author,
          permissions,
          packageName,
          serverId,
          viewId,
          x,
          y,
          w,
          h,
        }),
      ),
    })),
  };
}

function cardTemplate(index) {
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
    serverId: "",
    viewId: null,
    ...DEFAULT_CARD_SIZE,
    x: 1,
    y: 1,
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

    return {
      density: state.density,
      activeWorkspaceId: activeWorkspace.id,
      workspaces: cloneWorkspaces(state.workspaces),
      activeWorkspace: cloneWorkspace(activeWorkspace),
      cards: cloneCards(activeWorkspace.cards),
      boardState: exportState(state),
      layout: {
        cols: config.cols,
        rows: config.rows,
        gap: config.gap,
        density: config.density,
        densityLabel: config.densityLabel,
      },
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

    notify();
    return buildSnapshot();
  }

  function replaceActiveWorkspaceCards(nextCards, options = {}) {
    const activeWorkspace = getActiveWorkspace();
    activeWorkspace.cards = normalizeLayout(nextCards, config);
    return commit(options);
  }

  function replaceWorkspaceCards(workspaceId, nextCards, options = {}) {
    const workspace = getWorkspaceById(workspaceId);
    if (!workspace) {
      return buildSnapshot();
    }

    workspace.cards = normalizeLayout(nextCards, config);
    return commit(options);
  }

  function normalizeAllWorkspaces() {
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
    addCard() {
      const activeWorkspace = getActiveWorkspace();
      const nextCard = cardTemplate(activeWorkspace.cards.length + 1);
      const position = findOpenPosition(
        activeWorkspace.cards,
        DEFAULT_CARD_SIZE,
        config,
      );

      if (!position) {
        return null;
      }

      const created = { ...nextCard, ...position };
      replaceActiveWorkspaceCards([...activeWorkspace.cards, created]);
      return created;
    },
    addImportedCard(cardDefinition) {
      const activeWorkspace = getActiveWorkspace();
      const requestedSize = {
        w: Number(cardDefinition?.w) || DEFAULT_CARD_SIZE.w,
        h: Number(cardDefinition?.h) || DEFAULT_CARD_SIZE.h,
      };
      const position = findOpenPosition(
        activeWorkspace.cards,
        requestedSize,
        config,
      );

      if (!position) {
        return null;
      }

      const created = {
        id: nextEntityId("card"),
        kind: "package",
        title: String(cardDefinition?.title || "Card importado"),
        description: String(
          cardDefinition?.description || "Card importado de um package .lince.",
        ),
        text: "",
        html: String(cardDefinition?.html || ""),
        author: String(cardDefinition?.author || ""),
        permissions: Array.isArray(cardDefinition?.permissions)
          ? cardDefinition.permissions.map((permission) => String(permission))
          : [],
        packageName: String(cardDefinition?.packageName || ""),
        serverId: String(cardDefinition?.serverId || ""),
        viewId:
          cardDefinition?.viewId == null
            ? null
            : Number(cardDefinition.viewId) || null,
        ...requestedSize,
        ...position,
      };

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
      if (!card) {
        return null;
      }

      const position = findOpenPosition(
        targetWorkspace.cards,
        {
          w: Number(card.w) || DEFAULT_CARD_SIZE.w,
          h: Number(card.h) || DEFAULT_CARD_SIZE.h,
        },
        config,
      );
      if (!position) {
        return null;
      }

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
      const workspace = {
        id: nextEntityId("space"),
        name: `Area ${state.workspaces.length + 1}`,
        cards: [],
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
      normalizeAllWorkspaces();
      return commit();
    },
  };
}
