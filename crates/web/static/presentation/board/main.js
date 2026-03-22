import { attachBoardInteractions } from "./interactions.js";
import { createGridConfig } from "./grid.js";
import { createBoardStore } from "./store.js";
import { createWidgetBridge, enhancePackageHtml } from "./widget-bridge.js";

const PACKAGE_EXTENSION = ".html";
const LEGACY_PACKAGE_EXTENSION = ".sand";
const LEGACY_PACKAGE_ARCHIVE_EXTENSION = ".lince";
const WORKSPACE_ARCHIVE_EXTENSION = ".workspace.sand";
const LEGACY_WORKSPACE_ARCHIVE_EXTENSION = ".workspace.lince";
const DEFAULT_DROP_MESSAGE =
  "Solte um .html de widget ou um .workspace.sand para instalar no backend local.";

function stripPackageExtension(filename) {
  const value = String(filename || "");
  const lowercase = value.toLowerCase();
  if (lowercase.endsWith(PACKAGE_EXTENSION)) {
    return value.slice(0, -PACKAGE_EXTENSION.length);
  }
  if (lowercase.endsWith(LEGACY_PACKAGE_EXTENSION)) {
    return value.slice(0, -LEGACY_PACKAGE_EXTENSION.length);
  }
  if (lowercase.endsWith(LEGACY_PACKAGE_ARCHIVE_EXTENSION)) {
    return value.slice(0, -LEGACY_PACKAGE_ARCHIVE_EXTENSION.length);
  }
  return value;
}

function cloneJsonValue(value, fallback = null) {
  try {
    if (value === undefined) {
      return fallback;
    }

    return JSON.parse(JSON.stringify(value));
  } catch {
    return fallback;
  }
}

function isPlainObject(value) {
  return Boolean(
    value &&
    typeof value === "object" &&
    !Array.isArray(value) &&
    Object.getPrototypeOf(value) === Object.prototype,
  );
}

function applyJsonMergePatch(target, patch) {
  if (!isPlainObject(patch)) {
    return cloneJsonValue(patch, {});
  }

  const base = isPlainObject(target) ? cloneJsonValue(target, {}) : {};
  for (const [key, value] of Object.entries(patch)) {
    if (value === null) {
      delete base[key];
      continue;
    }

    if (isPlainObject(value) && isPlainObject(base[key])) {
      base[key] = applyJsonMergePatch(base[key], value);
      continue;
    }

    base[key] = cloneJsonValue(value, null);
  }

  return base;
}

const PERMISSION_DESCRIPTIONS = {
  bridge_state:
    "Permite receber estado compartilhado do host e sincronizar widgets via bridge interno.",
  control_spotify:
    "Permite controlar play, pause, proxima faixa e faixa anterior dentro do widget.",
  print_backend:
    "Permite acionar uma operacao demonstrativa no backend local por meio do host.",
  read_email:
    "Permite ler dados mock de email para resumos, alertas ou widgets de caixa de entrada.",
  read_location:
    "Permite usar localizacao como contexto para clima, proximidade ou widgets baseados em lugar.",
  read_metrics:
    "Permite consumir metricas ou listas tabulares mock dentro do package.",
  read_records:
    "Permite consultar registros mock para tabelas e visoes compactas.",
  read_spotify:
    "Permite ler faixa atual, capa, artista e estado de reproducao dentro do widget.",
  read_table:
    "Permite carregar linhas mock para tabelas, listas e pequenos datagrids.",
  read_tasks: "Permite ler tarefas persistidas pelo proprio widget.",
  read_view_stream:
    "Permite consumir um stream SSE de view mediado pelo backend local do host.",
  read_weather: "Permite ler dados mock de clima, temperatura e previsao.",
  terminal_session:
    "Permite abrir uma sessao de shell local via backend do host. Trate como capability de desenvolvimento.",
  write_records:
    "Permite executar mutacoes na tabela record por meio do backend local conectado ao servidor externo.",
  write_table:
    "Permite criar registros nas tabelas expostas pela API do backend remoto por meio do host local.",
  write_tasks:
    "Permite criar, atualizar e remover tarefas pelo proprio widget.",
};

const bootstrapElement = document.getElementById("lince-bootstrap");
const startupScreen = document.getElementById("startup-screen");
const startupErrorMessage = document.getElementById("startup-error-message");
const startupStatusPanel = document.getElementById("startup-status-panel");
const startupStatusLabel = startupStatusPanel?.querySelector(
  ".startup-status__label",
);
const startupStatusValue = document.getElementById("startup-status-value");
const startupProgress = document.getElementById("startup-progress");
const startupProgressFill = document.getElementById("startup-progress-fill");
const editToggle = document.getElementById("edit-toggle");
const addCardButton = document.getElementById("add-card-button");
const addCardImportButton = document.getElementById("add-card-import-button");
const addCardLocalButton = document.getElementById("add-card-local-button");
const addCardPopover = document.getElementById("add-card-popover");
const addWorkspaceButton = document.getElementById("add-workspace-button");
const importWorkspaceButton = document.getElementById(
  "import-workspace-button",
);
const exportWorkspaceButton = document.getElementById(
  "export-workspace-button",
);
const densityTag = document.getElementById("density-tag");
const modeLabel = document.getElementById("mode-label");
const streamsToggle = document.getElementById("streams-toggle");
const streamsToggleLabel = document.getElementById("streams-toggle-label");
const densitySlider = document.getElementById("density-slider");
const densityValue = document.getElementById("density-value");
const workspaceSwitcher = document.querySelector(".workspace-switcher");
const workspaceToggle = document.getElementById("workspace-toggle");
const workspaceCurrent = document.getElementById("workspace-current");
const workspacePopover = document.getElementById("workspace-popover");
const workspaceList = document.getElementById("workspace-list");
const workspaceEmpty = document.getElementById("workspace-empty");
const workspaceEmptyTitle = document.getElementById("workspace-empty-title");
const workspaceEmptyCopy = document.getElementById("workspace-empty-copy");
const boardShell = document.getElementById("board-shell");
const boardCanvas = document.getElementById("board-canvas");
const boardGrid = document.getElementById("board-grid");
const dropZoneOverlay = document.getElementById("drop-zone-overlay");
const dropZoneOverlayEyebrow = document.getElementById(
  "drop-zone-overlay-eyebrow",
);
const dropZoneOverlayTitle = document.getElementById("drop-zone-overlay-title");
const dropZoneOverlayCopy = document.getElementById("drop-zone-overlay-copy");
const importModalBackdrop = document.getElementById("import-modal-backdrop");
const importModalTitle = document.getElementById("import-modal-title");
const importModalDescription = document.getElementById(
  "import-modal-description",
);
const importPackageName = document.getElementById("import-package-name");
const importAuthor = document.getElementById("import-author");
const importVersion = document.getElementById("import-version");
const importSize = document.getElementById("import-size");
const importModalDetails = document.getElementById("import-modal-details");
const importPreviewDensity = document.getElementById("import-preview-density");
const importPreviewCells = document.getElementById("import-preview-cells");
const importPreviewCard = document.getElementById("import-preview-card");
const importPreviewFrame = document.getElementById("import-preview-frame");
const importPermissionsList = document.getElementById(
  "import-permissions-list",
);
const importCancelButton = document.getElementById("import-cancel-button");
const importConfirmButton = document.getElementById("import-confirm-button");
const importCloseButton = document.getElementById("import-close-button");
const localPackagesModalBackdrop = document.getElementById(
  "local-packages-modal-backdrop",
);
const localPackagesCloseButton = document.getElementById(
  "local-packages-close-button",
);
const localPackagesSummary = document.getElementById("local-packages-summary");
const localPackagesSearch = document.getElementById("local-packages-search");
const localPackageList = document.getElementById("local-package-list");
const deleteCardModalBackdrop = document.getElementById(
  "delete-card-modal-backdrop",
);
const deleteCardModalTitle = document.getElementById("delete-card-modal-title");
const deleteCardModalDescription = document.getElementById(
  "delete-card-modal-description",
);
const deleteCardModalName = document.getElementById("delete-card-modal-name");
const deleteCardCancelButton = document.getElementById(
  "delete-card-cancel-button",
);
const deleteCardConfirmButton = document.getElementById(
  "delete-card-confirm-button",
);
const deleteCardCloseButton = document.getElementById(
  "delete-card-close-button",
);
const serverLoginModalBackdrop = document.getElementById(
  "server-login-modal-backdrop",
);
const serverLoginModalDescription = document.getElementById(
  "server-login-modal-description",
);
const serverLoginServerName = document.getElementById(
  "server-login-server-name",
);
const serverLoginForm = document.getElementById("server-login-form");
const serverLoginUsernameInput = document.getElementById(
  "server-login-username",
);
const serverLoginPasswordInput = document.getElementById(
  "server-login-password",
);
const serverLoginPasswordToggle = document.getElementById(
  "server-login-password-toggle",
);
const serverLoginErrorMessage = document.getElementById(
  "server-login-error-message",
);
const serverLoginCancelButton = document.getElementById(
  "server-login-cancel-button",
);
const serverLoginConfirmButton = document.getElementById(
  "server-login-confirm-button",
);
const serverLoginCloseButton = document.getElementById(
  "server-login-close-button",
);
const widgetConfigModalBackdrop = document.getElementById(
  "widget-config-modal-backdrop",
);
const widgetConfigModalDescription = document.getElementById(
  "widget-config-modal-description",
);
const widgetConfigForm = document.getElementById("widget-config-form");
const widgetConfigServerId = document.getElementById("widget-config-server-id");
const widgetConfigViewIdField = document.getElementById(
  "widget-config-view-id-field",
);
const widgetConfigViewId = document.getElementById("widget-config-view-id");
const widgetConfigStreamsField = document.getElementById(
  "widget-config-streams-field",
);
const widgetConfigStreamsEnabled = document.getElementById(
  "widget-config-streams-enabled",
);
const widgetConfigHelp = document.getElementById("widget-config-help");
const widgetConfigCancelButton = document.getElementById(
  "widget-config-cancel-button",
);
const widgetConfigSaveButton = document.getElementById(
  "widget-config-save-button",
);
const widgetConfigCloseButton = document.getElementById(
  "widget-config-close-button",
);
const packageImportInput = document.getElementById("package-import-input");
const workspaceImportInput = document.getElementById("workspace-import-input");
const cardsLayer = document.getElementById("cards-layer");

if (
  !bootstrapElement ||
  !startupScreen ||
  !startupErrorMessage ||
  !startupStatusPanel ||
  !startupStatusLabel ||
  !startupStatusValue ||
  !startupProgress ||
  !startupProgressFill ||
  !editToggle ||
  !addCardButton ||
  !addCardImportButton ||
  !addCardLocalButton ||
  !addCardPopover ||
  !addWorkspaceButton ||
  !importWorkspaceButton ||
  !exportWorkspaceButton ||
  !densityTag ||
  !modeLabel ||
  !densitySlider ||
  !densityValue ||
  !workspaceSwitcher ||
  !workspaceToggle ||
  !workspaceCurrent ||
  !workspacePopover ||
  !workspaceList ||
  !workspaceEmpty ||
  !workspaceEmptyTitle ||
  !workspaceEmptyCopy ||
  !boardShell ||
  !boardCanvas ||
  !boardGrid ||
  !dropZoneOverlay ||
  !dropZoneOverlayEyebrow ||
  !dropZoneOverlayTitle ||
  !dropZoneOverlayCopy ||
  !importModalBackdrop ||
  !importModalTitle ||
  !importModalDescription ||
  !importPackageName ||
  !importAuthor ||
  !importVersion ||
  !importSize ||
  !importModalDetails ||
  !importPreviewDensity ||
  !importPreviewCells ||
  !importPreviewCard ||
  !importPreviewFrame ||
  !importPermissionsList ||
  !importCancelButton ||
  !importConfirmButton ||
  !importCloseButton ||
  !localPackagesModalBackdrop ||
  !localPackagesCloseButton ||
  !localPackagesSummary ||
  !localPackagesSearch ||
  !localPackageList ||
  !deleteCardModalBackdrop ||
  !deleteCardModalTitle ||
  !deleteCardModalDescription ||
  !deleteCardModalName ||
  !deleteCardCancelButton ||
  !deleteCardConfirmButton ||
  !deleteCardCloseButton ||
  !serverLoginModalBackdrop ||
  !serverLoginModalDescription ||
  !serverLoginServerName ||
  !serverLoginForm ||
  !serverLoginUsernameInput ||
  !serverLoginPasswordInput ||
  !serverLoginPasswordToggle ||
  !serverLoginErrorMessage ||
  !serverLoginCancelButton ||
  !serverLoginConfirmButton ||
  !serverLoginCloseButton ||
  !widgetConfigModalBackdrop ||
  !widgetConfigModalDescription ||
  !widgetConfigForm ||
  !widgetConfigServerId ||
  !widgetConfigViewIdField ||
  !widgetConfigViewId ||
  !widgetConfigHelp ||
  !widgetConfigCancelButton ||
  !widgetConfigSaveButton ||
  !widgetConfigCloseButton ||
  !packageImportInput ||
  !workspaceImportInput ||
  !cardsLayer
) {
  throw new Error("Lince bootstrap did not find the required DOM nodes.");
}

const bootstrap = JSON.parse(bootstrapElement.textContent || "{}");
const config = createGridConfig(bootstrap);
const hiddenCardCache = document.createElement("div");
hiddenCardCache.setAttribute("aria-hidden", "true");
hiddenCardCache.style.position = "fixed";
hiddenCardCache.style.width = "1px";
hiddenCardCache.style.height = "1px";
hiddenCardCache.style.overflow = "hidden";
hiddenCardCache.style.opacity = "0";
hiddenCardCache.style.pointerEvents = "none";
hiddenCardCache.style.inset = "-9999px auto auto -9999px";
document.body.append(hiddenCardCache);
const store = createBoardStore({
  seedCards: Array.isArray(bootstrap.cards) ? bootstrap.cards : [],
  initialBoardState: bootstrap.boardState,
  config,
  async persistState(nextState) {
    const response = await fetch(apiPath("/api/board/state"), {
      method: "PUT",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify(nextState),
    });

    if (!response.ok) {
      const payload = await response.json().catch(() => null);
      throw new Error(
        payload?.error || "Falha ao persistir o board no backend.",
      );
    }
  },
});
let editMode = false;
const widgetBridge = createWidgetBridge({
  statusNode: null,
  initialState: bootstrap.widgetBridge,
  getFrames: () =>
    Array.from(
      document.querySelectorAll("iframe.package-widget__frame"),
    ).filter(Boolean),
  getCardMeta(instanceId) {
    return getCardBridgeMeta(instanceId);
  },
  setCardState(instanceId, nextState) {
    updateCardWidgetState(instanceId, nextState);
  },
  patchCardState(instanceId, patch) {
    patchCardWidgetState(instanceId, patch);
  },
  setCardStreamsEnabled(instanceId, enabled) {
    updateCardStreamsEnabled(instanceId, enabled);
  },
  async invalidateServerAuth(serverId) {
    const target = String(serverId || "").trim();
    if (!target) {
      return;
    }

    await refreshServerProfiles();
    flashDropOverlayMessage(
      `A sessao do servidor ${target} expirou. Conecte novamente para desbloquear os widgets.`,
    );
  },
  onError(message) {
    flashDropOverlayMessage(message);
  },
});
let activeCardId = null;
let activeInteractionType = null;
let addCardPopoverOpen = false;
let blockedFlashTimeout = null;
let workspacePopoverOpen = false;
let pendingWorkspaceTransitionDirection = 0;
let lastRenderedWorkspaceId = null;
let workspaceTransitionCleanup = null;
let dropHoverDepth = 0;
let dropOverlayFlashTimeout = null;
let pendingImportPreview = null;
let pendingImportFile = null;
let pendingDeleteCardId = null;
let installedPackages = [];
let serverProfiles = Array.isArray(bootstrap?.servers) ? bootstrap.servers : [];
let pendingServerLogin = null;
let pendingWidgetConfigCardId = null;

const startupPaths = Array.from(startupScreen.querySelectorAll(".s0"));
const cardNodes = new Map(
  Array.from(cardsLayer.querySelectorAll("[data-card-id]")).map((node) => [
    node.dataset.cardId,
    node,
  ]),
);

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function formatWorkspaceNumber(index) {
  return String(index + 1).padStart(2, "0");
}

function apiPath(path) {
  if (path.startsWith("/api/")) {
    return `/host/${path.slice("/api/".length)}`;
  }

  return path;
}

function normalizeServerProfile(rawServer) {
  return {
    id: String(rawServer?.id || ""),
    name: String(rawServer?.name || "Servidor"),
    baseUrl: String(rawServer?.baseUrl || rawServer?.base_url || ""),
    authenticated: Boolean(rawServer?.authenticated),
    usernameHint: String(
      rawServer?.usernameHint || rawServer?.username_hint || "",
    ),
  };
}

function syncServerProfiles(nextProfiles) {
  serverProfiles = Array.isArray(nextProfiles)
    ? nextProfiles.map(normalizeServerProfile)
    : [];
}

function getServerProfile(serverId) {
  return (
    serverProfiles.find((server) => server.id === String(serverId || "")) ||
    null
  );
}

function cardRequiresServer(card) {
  const permissions = Array.isArray(card?.permissions) ? card.permissions : [];
  return (
    permissions.includes("read_view_stream") ||
    permissions.includes("write_records") ||
    permissions.includes("write_table")
  );
}

function cardRequiresViewId(card) {
  const permissions = Array.isArray(card?.permissions) ? card.permissions : [];
  return permissions.includes("read_view_stream");
}

function cardSupportsStream(card) {
  return cardRequiresViewId(card);
}

function getCardRecord(cardId) {
  const snapshot = store.getSnapshot();

  for (const workspace of snapshot.workspaces) {
    const card = workspace.cards.find((entry) => entry.id === cardId);
    if (card) {
      return { workspace, card };
    }
  }

  return null;
}

function getCardBridgeMeta(cardId) {
  const record = getCardRecord(cardId);
  const card = record?.card || null;
  const snapshot = store.getSnapshot();
  const globalEnabled = snapshot.globalStreamsEnabled !== false;
  const cardEnabled = card?.streamsEnabled !== false;

  return {
    instanceId: cardId,
    source: "host",
    mode: editMode ? "edit" : "view",
    serverId: card?.serverId || "",
    viewId: card?.viewId ?? null,
    cardState: cloneJsonValue(card?.widgetState, {}),
    streams: {
      globalEnabled,
      cardEnabled,
      enabled: globalEnabled && cardEnabled,
    },
  };
}

function updateCardWidgetState(cardId, nextState) {
  return store.updateCard(
    cardId,
    (card) => ({
      ...card,
      widgetState: cloneJsonValue(nextState, {}),
    }),
    { persist: true },
  );
}

function patchCardWidgetState(cardId, patch) {
  return store.updateCard(
    cardId,
    (card) => ({
      ...card,
      widgetState: applyJsonMergePatch(card.widgetState, patch),
    }),
    { persist: true },
  );
}

function updateCardStreamsEnabled(cardId, enabled) {
  return store.updateCard(
    cardId,
    (card) => ({
      ...card,
      streamsEnabled: Boolean(enabled),
    }),
    { persist: true },
  );
}

function resolveCardServerState(card) {
  if (!cardRequiresServer(card)) {
    return { state: "ready", server: null };
  }

  const serverId = String(card?.serverId || "").trim();
  if (!serverId) {
    return {
      state: "misconfigured",
      server: null,
      message: "Escolha um servidor para esse widget.",
    };
  }

  const server = getServerProfile(serverId);
  if (!server) {
    return {
      state: "misconfigured",
      server: null,
      message: "O servidor escolhido nao existe mais.",
    };
  }

  if (cardRequiresViewId(card)) {
    const viewId = Number(card?.viewId);
    if (!Number.isInteger(viewId) || viewId <= 0) {
      return {
        state: "misconfigured",
        server,
        message: "Defina um view id valido para esse widget.",
      };
    }
  }

  if (!server.authenticated) {
    return {
      state: "locked",
      server,
      message: `Conecte o servidor ${server.name} para liberar esse widget.`,
    };
  }

  return {
    state: "ready",
    server,
  };
}

function getActiveWorkspaceIndex(snapshot) {
  const index = snapshot.workspaces.findIndex(
    (workspace) => workspace.id === snapshot.activeWorkspaceId,
  );

  return index >= 0 ? index : 0;
}

function resolveWorkspaceDirection(fromIndex, toIndex) {
  if (fromIndex === toIndex) {
    return 0;
  }

  return toIndex > fromIndex ? 1 : -1;
}

function renderTrashIcon() {
  return `
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M3.5 4.5h9"></path>
      <path d="M6.5 2.75h3"></path>
      <path d="M5 4.5v7"></path>
      <path d="M8 4.5v7"></path>
      <path d="M11 4.5v7"></path>
      <path d="M4.5 4.5 5 13h6l.5-8.5"></path>
    </svg>
  `;
}

function renderCloseIcon() {
  return `
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M4 4l8 8"></path>
      <path d="M12 4 4 12"></path>
    </svg>
  `;
}

function renderConfigureIcon() {
  return `
    <svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.35" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">
      <path d="M8 3.25a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5Z"></path>
      <path d="M8 11.25a.75.75 0 1 0 0 1.5.75.75 0 0 0 0-1.5Z"></path>
      <path d="M3.25 8a.75.75 0 1 0 1.5 0 .75.75 0 0 0-1.5 0Z"></path>
      <path d="M11.25 8a.75.75 0 1 0 1.5 0 .75.75 0 0 0-1.5 0Z"></path>
      <path d="M8 4.75V11.25"></path>
      <path d="M4.75 8H11.25"></path>
    </svg>
  `;
}

function renderConfigureButton(card) {
  if (!cardRequiresServer(card)) {
    return "";
  }

  return `
    <button
      type="button"
      class="card-delete-button card-delete-button--secondary"
      data-card-action="configure"
      aria-label="Configurar ${escapeHtml(card.title)}"
    >
      <span class="card-delete-button__icon">${renderConfigureIcon()}</span>
      <span class="card-delete-button__label">CONFIGURAR</span>
    </button>
  `;
}

function renderHandles() {
  return `
    <button type="button" class="resize-handle resize-handle--nw" tabindex="-1" aria-hidden="true" data-resize-handle="nw"></button>
    <button type="button" class="resize-handle resize-handle--ne" tabindex="-1" aria-hidden="true" data-resize-handle="ne"></button>
    <button type="button" class="resize-handle resize-handle--sw" tabindex="-1" aria-hidden="true" data-resize-handle="sw"></button>
    <button type="button" class="resize-handle resize-handle--se" tabindex="-1" aria-hidden="true" data-resize-handle="se"></button>
    <button type="button" class="resize-handle resize-handle--n" tabindex="-1" aria-hidden="true" data-resize-handle="n"></button>
    <button type="button" class="resize-handle resize-handle--e" tabindex="-1" aria-hidden="true" data-resize-handle="e"></button>
    <button type="button" class="resize-handle resize-handle--s" tabindex="-1" aria-hidden="true" data-resize-handle="s"></button>
    <button type="button" class="resize-handle resize-handle--w" tabindex="-1" aria-hidden="true" data-resize-handle="w"></button>
  `;
}

function renderDeleteButton(card) {
  return `
    <button
      type="button"
      class="card-delete-button"
      data-card-action="delete"
      aria-label="Excluir ${escapeHtml(card.title)}"
    >
      <span class="card-delete-button__icon">${renderTrashIcon()}</span>
      <span class="card-delete-button__label">REMOVER</span>
    </button>
  `;
}

function renderTextBody(card) {
  return `
    <div class="text-widget">
      <p class="text-widget__content" data-card-text>
        ${escapeHtml(card.text)}
      </p>
      <div class="text-widget__meta">
        <span>snap grid</span>
        <span>workspace card</span>
      </div>
    </div>
  `;
}

function renderPackageBody(card) {
  const gate = resolveCardServerState(card);
  const preparedHtml = enhancePackageHtml(card.html || "");
  const frameAttributes =
    `data-lince-server-id="${escapeHtml(card.serverId || "")}" ` +
    `data-lince-view-id="${escapeHtml(card.viewId == null ? "" : String(card.viewId))}"`;

  if (gate.state !== "ready") {
    return `
      <div class="package-widget package-widget--locked">
        <div class="package-widget__locked">
          <div class="package-widget__locked-eyebrow">${
            gate.state === "locked" ? "Servidor bloqueado" : "Falta configurar"
          }</div>
          <strong class="package-widget__locked-title">${escapeHtml(card.title)}</strong>
          <p class="package-widget__locked-copy">${escapeHtml(gate.message || "Widget indisponivel.")}</p>
          <div class="package-widget__locked-actions">
            <button type="button" class="modal-button modal-button--ghost" data-card-action="configure" data-card-id="${escapeHtml(card.id)}">Configurar</button>
            <button type="button" class="modal-button modal-button--ghost" data-card-action="delete" data-card-id="${escapeHtml(card.id)}">Remover</button>
            ${
              gate.state === "locked" && gate.server
                ? `<button type="button" class="modal-button modal-button--primary" data-card-action="connect-server" data-card-id="${escapeHtml(card.id)}">Conectar ${escapeHtml(gate.server.name)}</button>`
                : ""
            }
          </div>
        </div>
      </div>
    `;
  }

  return `
    <div class="package-widget">
      <iframe
        class="package-widget__frame"
        title="${escapeHtml(card.title)}"
        loading="lazy"
        data-package-instance-id="${escapeHtml(card.id)}"
        ${frameAttributes}
        sandbox="allow-scripts allow-same-origin"
        srcdoc="${escapeHtml(preparedHtml)}"
      ></iframe>
    </div>
  `;
}

function renderCardMarkup(card) {
  const isPackageCard = card.kind === "package";
  if (isPackageCard) {
    return `
      <article class="board-card board-card--package" data-card-id="${escapeHtml(card.id)}" data-card-kind="${escapeHtml(card.kind || "package")}">
        ${renderDeleteButton(card)}
        ${renderConfigureButton(card)}
        ${renderPackageBody(card)}
        ${renderHandles()}
      </article>
    `;
  }

  return `
    <article class="board-card" data-card-id="${escapeHtml(card.id)}" data-card-kind="${escapeHtml(card.kind || "text")}">
      ${renderDeleteButton(card)}
      <header class="card-header">
        <span class="card-eyebrow">Widget</span>
        <h2 class="card-title" data-card-title>${escapeHtml(card.title)}</h2>
        <p class="card-copy" data-card-description>${escapeHtml(card.description || "")}</p>
      </header>
      <div class="card-body">
        ${renderTextBody(card)}
      </div>
      ${renderHandles()}
    </article>
  `;
}

function ensureCardNode(card) {
  let node = cardNodes.get(card.id);

  if (node && node.dataset.cardKind !== (card.kind || "text")) {
    node.remove();
    cardNodes.delete(card.id);
    node = null;
  }

  if (!node) {
    const template = document.createElement("template");
    template.innerHTML = renderCardMarkup(card).trim();
    node = template.content.firstElementChild;
    if (card.kind === "package") {
      node.dataset.packageRenderSignature = createPackageRenderSignature(card);
    }
    cardsLayer.appendChild(node);
    cardNodes.set(card.id, node);
  } else if (node.parentElement !== cardsLayer) {
    cardsLayer.appendChild(node);
  }

  return node;
}

function createPackageRenderSignature(card) {
  const gate = resolveCardServerState(card);
  if (gate.state === "ready") {
    return `ready:${cardRequiresServer(card) ? "server" : "local"}`;
  }

  return `${gate.state}:${gate.server?.id || ""}:${gate.message || ""}:${card.title || ""}`;
}

function syncCardNode(node, card) {
  node.style.gridColumn = `${card.x} / span ${card.w}`;
  node.style.gridRow = `${card.y} / span ${card.h}`;
  node.style.zIndex = card.id === activeCardId ? "4" : "1";
  node.dataset.cardKind = card.kind || "text";
  node.classList.toggle("board-card--package", card.kind === "package");
  node.classList.toggle("is-compact", card.w <= 2 || card.h <= 2);
  node.classList.toggle("is-tiny", card.w === 1 || card.h === 1);
  node.classList.toggle("is-active", card.id === activeCardId);
  node.classList.toggle(
    "is-dragging",
    card.id === activeCardId && activeInteractionType === "move",
  );
  node.classList.toggle(
    "is-resizing",
    card.id === activeCardId && activeInteractionType === "resize",
  );

  if (card.kind === "package") {
    const nextSignature = createPackageRenderSignature(card);
    if (node.dataset.packageRenderSignature !== nextSignature) {
      node.innerHTML =
        renderDeleteButton(card) +
        renderConfigureButton(card) +
        renderPackageBody(card) +
        renderHandles();
      node.dataset.packageRenderSignature = nextSignature;
    }
  }

  const titleNode = node.querySelector("[data-card-title]");
  const descriptionNode = node.querySelector("[data-card-description]");
  const textNode = node.querySelector("[data-card-text]");
  const frameNode = node.querySelector(".package-widget__frame");
  const deleteButton = node.querySelector("[data-card-action='delete']");

  if (titleNode) {
    titleNode.textContent = card.title;
  }

  if (descriptionNode) {
    descriptionNode.textContent = card.description || "";
  }

  if (textNode) {
    textNode.textContent = card.text || "";
  }

  if (frameNode) {
    const preparedHtml = enhancePackageHtml(card.html || "");
    if (frameNode.getAttribute("srcdoc") !== preparedHtml) {
      frameNode.setAttribute("srcdoc", preparedHtml);
    }
    frameNode.setAttribute("title", card.title || "External card");
    frameNode.dataset.packageInstanceId = card.id || "";
    frameNode.dataset.linceServerId = card.serverId || "";
    frameNode.dataset.linceViewId =
      card.viewId == null ? "" : String(card.viewId);
  }

  if (deleteButton) {
    deleteButton.setAttribute("aria-label", `Excluir ${card.title || "card"}`);
  }

  const configureButton = node.querySelector("[data-card-action='configure']");
  if (configureButton) {
    configureButton.setAttribute(
      "aria-label",
      `Configurar ${card.title || "card"}`,
    );
  }
}

function resolveAllCardIds(allCardIds) {
  if (allCardIds instanceof Set) {
    return allCardIds;
  }

  return new Set(
    store
      .getSnapshot()
      .workspaces.flatMap((workspace) =>
        workspace.cards.map((card) => card.id),
      ),
  );
}

function renderCards(cards, allCardIds) {
  const resolvedAllCardIds = resolveAllCardIds(allCardIds);
  const seenIds = new Set(cards.map((card) => card.id));

  for (const card of cards) {
    const node = ensureCardNode(card);
    syncCardNode(node, card);
  }

  for (const [cardId, node] of cardNodes.entries()) {
    if (seenIds.has(cardId)) {
      continue;
    }

    if (!resolvedAllCardIds.has(cardId)) {
      node.remove();
      cardNodes.delete(cardId);
      continue;
    }

    if (node.dataset.cardKind === "package") {
      hiddenCardCache.appendChild(node);
      continue;
    }

    node.remove();
    cardNodes.delete(cardId);
  }
}

function ensureBackgroundPackageCards(snapshot) {
  for (const workspace of snapshot.workspaces) {
    if (workspace.id === snapshot.activeWorkspaceId) {
      continue;
    }

    for (const card of workspace.cards) {
      if (card.kind !== "package") {
        continue;
      }

      const node = ensureCardNode(card);
      syncCardNode(node, card);
      if (node.parentElement !== hiddenCardCache) {
        hiddenCardCache.appendChild(node);
      }
    }
  }
}

function renderGrid(layout) {
  boardShell.style.setProperty("--board-cols", layout.cols);
  boardShell.style.setProperty("--board-rows", layout.rows);
  boardShell.style.setProperty("--board-gap", `${layout.gap}px`);

  const signature = `${layout.cols}:${layout.rows}`;
  if (boardGrid.dataset.signature === signature) {
    return;
  }

  boardGrid.dataset.signature = signature;
  boardGrid.innerHTML = Array.from(
    { length: layout.cols * layout.rows },
    () => '<div class="board-grid__cell"></div>',
  ).join("");
}

function renderWorkspaceList(snapshot) {
  const activeIndex = getActiveWorkspaceIndex(snapshot);
  const canDelete = snapshot.workspaces.length > 1;

  workspaceCurrent.textContent = formatWorkspaceNumber(activeIndex);
  workspaceToggle.setAttribute(
    "aria-label",
    `Area ${formatWorkspaceNumber(activeIndex)}. Abrir seletor de areas`,
  );

  workspaceList.innerHTML = snapshot.workspaces
    .map((workspace, index) => {
      const workspaceNumber = formatWorkspaceNumber(index);
      const isActive = workspace.id === snapshot.activeWorkspaceId;

      return `
        <div class="workspace-item${isActive ? " is-active" : ""}">
          <button
            type="button"
            class="workspace-item__switch"
            data-workspace-action="switch"
            data-workspace-id="${escapeHtml(workspace.id)}"
            aria-pressed="${String(isActive)}"
            aria-label="Ir para a area ${workspaceNumber}"
          >
            <span class="workspace-item__number">${workspaceNumber}</span>
          </button>
          <button
            type="button"
            class="workspace-item__delete"
            data-workspace-action="delete"
            data-workspace-id="${escapeHtml(workspace.id)}"
            aria-label="Apagar a area ${workspaceNumber}"
            ${canDelete ? "" : "disabled"}
          >
            ${renderTrashIcon()}
          </button>
        </div>
      `;
    })
    .join("");
}

function renderDensity(snapshot) {
  densitySlider.value = String(snapshot.density);
  densityValue.textContent = `${snapshot.layout.cols} x ${snapshot.layout.rows} · ${snapshot.layout.densityLabel}`;
}

function renderStreamsToggle(snapshot) {
  if (!streamsToggle || !streamsToggleLabel) {
    return;
  }

  const enabled = snapshot.globalStreamsEnabled !== false;
  streamsToggle.classList.toggle("is-active", enabled);
  streamsToggle.classList.toggle("is-paused", !enabled);
  streamsToggle.setAttribute("aria-pressed", String(enabled));
  streamsToggle.setAttribute(
    "aria-label",
    enabled ? "Pausar todos os streams" : "Retomar todos os streams",
  );
  streamsToggleLabel.textContent = enabled ? "Streams on" : "Streams off";
}

function renderEmptyState(snapshot) {
  const isEmpty = snapshot.cards.length === 0;
  workspaceEmpty.hidden = !isEmpty;

  if (!isEmpty) {
    return;
  }

  workspaceEmptyTitle.textContent = "Sem cards por aqui";
  workspaceEmptyCopy.textContent = editMode
    ? "Solte um .html ou use Add card para preencher esse espaco."
    : "Esse espaco esta livre. Entre em modo de edicao ou crie outro pelo seletor.";
}

function renderSnapshot(snapshot) {
  const workspaceChanged =
    lastRenderedWorkspaceId !== null &&
    lastRenderedWorkspaceId !== snapshot.activeWorkspaceId;

  if (!snapshot.cards.some((card) => card.id === activeCardId)) {
    activeCardId = null;
    activeInteractionType = null;
  }

  if (
    pendingDeleteCardId &&
    !snapshot.cards.some((card) => card.id === pendingDeleteCardId)
  ) {
    closeDeleteCardModal();
  }

  renderGrid(snapshot.layout);
  renderWorkspaceList(snapshot);
  renderDensity(snapshot);
  renderStreamsToggle(snapshot);
  const allCardIds = new Set(
    snapshot.workspaces.flatMap((workspace) =>
      workspace.cards.map((card) => card.id),
    ),
  );
  renderCards(snapshot.cards, allCardIds);
  ensureBackgroundPackageCards(snapshot);
  widgetBridge.syncFrames();
  renderEmptyState(snapshot);

  if (workspaceChanged && pendingWorkspaceTransitionDirection) {
    playWorkspaceTransition(pendingWorkspaceTransitionDirection);
  }

  pendingWorkspaceTransitionDirection = 0;
  lastRenderedWorkspaceId = snapshot.activeWorkspaceId;
}

function setActiveCard(cardId, interactionType = null) {
  activeCardId = cardId;
  activeInteractionType = interactionType;
  renderCards(store.getCards());
}

function clearActiveCard() {
  activeCardId = null;
  activeInteractionType = null;
  renderCards(store.getCards());
}

function flashBoardBlocked() {
  boardShell.classList.add("is-blocked");

  if (blockedFlashTimeout) {
    window.clearTimeout(blockedFlashTimeout);
  }

  blockedFlashTimeout = window.setTimeout(() => {
    boardShell.classList.remove("is-blocked");
  }, 400);
}

function setWorkspaceTransferPreview(direction) {
  boardShell.classList.toggle("is-transfer-left", direction < 0);
  boardShell.classList.toggle("is-transfer-right", direction > 0);
}

function handleWorkspaceEdgeTransfer(card, direction) {
  const snapshot = store.getSnapshot();
  const currentIndex = getActiveWorkspaceIndex(snapshot);
  const targetIndex = currentIndex + direction;

  if (targetIndex < 0 || targetIndex >= snapshot.workspaces.length) {
    return false;
  }

  queueWorkspaceTransition(direction);
  const transferred = store.moveCardToAdjacentWorkspace(card.id, direction, {
    persist: true,
  });
  if (!transferred) {
    clearWorkspaceTransition();
    flashBoardBlocked();
    return false;
  }

  return true;
}

function clearWorkspaceTransition() {
  if (workspaceTransitionCleanup) {
    window.clearTimeout(workspaceTransitionCleanup);
    workspaceTransitionCleanup = null;
  }

  boardShell.classList.remove("is-workspace-transitioning");
  boardGrid.classList.remove(
    "board-grid--entering",
    "board-grid--from-right",
    "board-grid--from-left",
    "is-active",
  );
  cardsLayer.classList.remove(
    "cards-layer--entering",
    "cards-layer--from-right",
    "cards-layer--from-left",
    "is-active",
  );
  workspaceEmpty.classList.remove(
    "workspace-empty--entering",
    "workspace-empty--from-right",
    "workspace-empty--from-left",
    "is-active",
  );

  for (const node of boardCanvas.querySelectorAll(
    ".workspace-transition-layer",
  )) {
    node.remove();
  }
}

function queueWorkspaceTransition(direction) {
  if (!direction) {
    return;
  }

  clearWorkspaceTransition();

  const layer = document.createElement("div");
  layer.className = `workspace-transition-layer ${
    direction > 0
      ? "workspace-transition-layer--to-left"
      : "workspace-transition-layer--to-right"
  }`;

  const gridClone = boardGrid.cloneNode(true);
  gridClone.removeAttribute("id");
  gridClone.classList.add("workspace-transition-layer__grid");
  layer.appendChild(gridClone);

  const cardsClone = cardsLayer.cloneNode(true);
  cardsClone.removeAttribute("id");
  cardsClone.classList.add("workspace-transition-layer__cards");
  for (const frame of cardsClone.querySelectorAll(
    "iframe.package-widget__frame",
  )) {
    const placeholder = document.createElement("div");
    placeholder.className = "workspace-transition-frame-placeholder";
    frame.replaceWith(placeholder);
  }
  layer.appendChild(cardsClone);

  if (!workspaceEmpty.hidden) {
    const emptyClone = workspaceEmpty.cloneNode(true);
    emptyClone.removeAttribute("id");
    emptyClone.hidden = false;

    for (const duplicateIdNode of emptyClone.querySelectorAll("[id]")) {
      duplicateIdNode.removeAttribute("id");
    }

    emptyClone.classList.add("workspace-transition-layer__empty");
    layer.appendChild(emptyClone);
  }

  boardCanvas.appendChild(layer);
  pendingWorkspaceTransitionDirection = direction;
}

function playWorkspaceTransition(direction) {
  const directionClass = direction > 0 ? "from-right" : "from-left";
  const outgoingLayer = boardCanvas.querySelector(
    ".workspace-transition-layer",
  );
  if (!outgoingLayer) {
    return;
  }

  boardShell.classList.add("is-workspace-transitioning");
  outgoingLayer.getBoundingClientRect();
  boardGrid.classList.add(
    "board-grid--entering",
    `board-grid--${directionClass}`,
  );
  cardsLayer.classList.add(
    "cards-layer--entering",
    `cards-layer--${directionClass}`,
  );
  workspaceEmpty.classList.add(
    "workspace-empty--entering",
    `workspace-empty--${directionClass}`,
  );
  boardGrid.getBoundingClientRect();
  cardsLayer.getBoundingClientRect();
  workspaceEmpty.getBoundingClientRect();

  window.requestAnimationFrame(() => {
    window.requestAnimationFrame(() => {
      outgoingLayer.classList.add("is-active");
      boardGrid.classList.add("is-active");
      cardsLayer.classList.add("is-active");
      workspaceEmpty.classList.add("is-active");
    });
  });

  workspaceTransitionCleanup = window.setTimeout(() => {
    clearWorkspaceTransition();
  }, 660);
}

function matchesWorkspaceArrowShortcut(event, direction) {
  const expectedCode = direction > 0 ? "ArrowRight" : "ArrowLeft";
  if (event.code !== expectedCode) {
    return false;
  }

  if (event.altKey && !event.metaKey && !event.ctrlKey) {
    return true;
  }

  if (event.shiftKey && !event.altKey && !event.metaKey && !event.ctrlKey) {
    return true;
  }

  if (event.shiftKey && event.ctrlKey && !event.altKey && !event.metaKey) {
    return true;
  }

  return false;
}

function setWorkspacePopoverOpen(nextOpen) {
  workspacePopoverOpen = Boolean(nextOpen);
  if (workspacePopoverOpen) {
    setAddCardPopoverOpen(false);
  }
  workspaceSwitcher.classList.toggle("is-open", workspacePopoverOpen);
  workspacePopover.hidden = !workspacePopoverOpen;
  workspaceToggle.setAttribute("aria-expanded", String(workspacePopoverOpen));
}

function setAddCardPopoverOpen(nextOpen) {
  addCardPopoverOpen = Boolean(nextOpen) && editMode;
  addCardPopover.hidden = !addCardPopoverOpen;
  addCardButton.setAttribute("aria-expanded", String(addCardPopoverOpen));
}

function normalizedPackageSearch() {
  return localPackagesSearch.value.trim().toLowerCase();
}

function matchesPackageQuery(pkg, query) {
  if (!query) {
    return true;
  }

  const tokens = [
    pkg.id,
    pkg.icon,
    pkg.filename,
    pkg.title,
    pkg.author,
    pkg.description,
    pkg.details,
    ...(Array.isArray(pkg.permissions) ? pkg.permissions : []),
  ]
    .filter(Boolean)
    .join(" ")
    .toLowerCase();

  return tokens.includes(query);
}

function summarizeLocalPackages(filteredPackages) {
  const visibleCount = filteredPackages.length;
  const totalCount = installedPackages.length;

  if (!installedPackages.length) {
    localPackagesSummary.textContent =
      "Nenhum widget no catalogo ainda. Importe um .html para adicionar um widget local.";
    return;
  }

  if (normalizedPackageSearch()) {
    localPackagesSummary.textContent = `${visibleCount} de ${totalCount} widgets correspondem a essa busca.`;
    return;
  }

  localPackagesSummary.textContent = `${totalCount} widget${totalCount > 1 ? "s" : ""} disponivel${
    totalCount > 1 ? "eis" : ""
  } no catalogo. Os oficiais sao renderizados em ~/.config/lince/web/sand; os locais ficam em ~/.config/lince/web/widgets.`;
}

function renderLocalPackageList() {
  const query = normalizedPackageSearch();
  const filteredPackages = installedPackages.filter((pkg) =>
    matchesPackageQuery(pkg, query),
  );
  summarizeLocalPackages(filteredPackages);

  if (!installedPackages.length) {
    localPackageList.innerHTML = `
      <div class="local-package-empty">
        <strong>Nada no catalogo ainda</strong>
        <span>Use Importar para mandar um .html para o backend local.</span>
      </div>
    `;
    return;
  }

  if (!filteredPackages.length) {
    localPackageList.innerHTML = `
      <div class="local-package-empty">
        <strong>Nenhum resultado</strong>
        <span>Tente buscar por nome, arquivo, autor ou permissao do widget.</span>
      </div>
    `;
    return;
  }

  localPackageList.innerHTML = filteredPackages
    .map((pkg) => {
      const width = Number(pkg.initialWidth ?? pkg.initial_width) || 3;
      const height = Number(pkg.initialHeight ?? pkg.initial_height) || 2;
      const permissions = Array.isArray(pkg.permissions)
        ? pkg.permissions.slice(0, 2)
        : [];
      return `
        <button
          type="button"
          class="local-package-card"
          data-local-package-id="${escapeHtml(pkg.id)}"
          aria-label="Adicionar ${escapeHtml(pkg.title)}"
        >
          <span class="local-package-card__icon" aria-hidden="true">${escapeHtml(pkg.icon || "◧")}</span>
          <span class="local-package-card__body">
            <span class="local-package-card__topline">
              <strong class="local-package-card__title">${escapeHtml(pkg.title)}</strong>
              <span class="local-package-card__size">${escapeHtml(`${width} x ${height}`)}</span>
            </span>
            <span class="local-package-card__description">${escapeHtml(
              pkg.description || "Widget local instalado no sistema.",
            )}</span>
            <span class="local-package-card__meta">${escapeHtml(
              `${pkg.filename} · ${pkg.author || "Lince Labs"}`,
            )}</span>
            <span class="local-package-card__footer">
              ${
                permissions.length
                  ? permissions
                      .map(
                        (permission) =>
                          `<span class="local-package-card__pill">${escapeHtml(permission)}</span>`,
                      )
                      .join("")
                  : '<span class="local-package-card__pill local-package-card__pill--muted">sem permissoes</span>'
              }
            </span>
          </span>
        </button>
      `;
    })
    .join("");
}

function upsertInstalledPackage(pkg) {
  const summary = {
    id: String(pkg.id || stripPackageExtension(String(pkg.filename || ""))),
    filename: String(pkg.filename || `widget${PACKAGE_EXTENSION}`),
    icon: String(pkg.icon || "◧"),
    title: String(pkg.title || "Widget local"),
    author: String(pkg.author || ""),
    version: String(pkg.version || "0.1.0"),
    description: String(
      pkg.description || "Widget local instalado no sistema.",
    ),
    details: String(pkg.details || ""),
    initial_width: Number(pkg.initial_width) || 3,
    initial_height: Number(pkg.initial_height) || 2,
    permissions: Array.isArray(pkg.permissions) ? pkg.permissions : [],
  };
  const nextPackages = installedPackages.filter(
    (entry) => entry.id !== summary.id,
  );
  nextPackages.push(summary);
  nextPackages.sort((left, right) =>
    left.title.localeCompare(right.title, "pt-BR"),
  );
  installedPackages = nextPackages;
  renderLocalPackageList();
}

async function loadInstalledPackages() {
  const response = await fetch(apiPath("/api/packages/local"));
  const payload = await parseJsonResponse(response);
  if (!response.ok) {
    throw new Error(
      payload?.error || "Falha ao carregar o catalogo local de widgets.",
    );
  }

  installedPackages = Array.isArray(payload) ? payload : [];
  renderLocalPackageList();
  return installedPackages;
}

async function requestInstalledPackage(packageId) {
  const response = await fetch(
    apiPath(`/api/packages/local/${encodeURIComponent(packageId)}`),
  );
  const payload = await parseJsonResponse(response);
  if (!response.ok) {
    throw new Error(payload?.error || "Falha ao abrir o widget local.");
  }

  return payload;
}

async function installUploadedPackage(file) {
  const formData = new FormData();
  formData.append("package", file);

  const response = await fetch(apiPath("/api/packages/install"), {
    method: "POST",
    body: formData,
  });
  const payload = await parseJsonResponse(response);
  if (!response.ok) {
    throw new Error(
      payload?.error || "Falha ao instalar o widget HTML no backend.",
    );
  }

  upsertInstalledPackage(payload);
  return payload;
}

function openLocalPackagesModal() {
  setAddCardPopoverOpen(false);
  renderLocalPackageList();
  localPackagesModalBackdrop.hidden = false;
  syncModalLock();
  window.setTimeout(() => {
    localPackagesSearch.focus();
    localPackagesSearch.select();
  }, 0);
  void loadInstalledPackages().catch((error) => {
    localPackagesSummary.textContent =
      error instanceof Error ? error.message : "Falha ao ler o catalogo local.";
  });
}

function closeLocalPackagesModal() {
  localPackagesModalBackdrop.hidden = true;
  syncModalLock();
}

function setEditMode(nextEditMode) {
  editMode = Boolean(nextEditMode);
  boardShell.classList.toggle("is-editing", editMode);
  document.documentElement.classList.toggle("edit-mode-active", editMode);
  editToggle.classList.toggle("is-active", editMode);
  editToggle.setAttribute("aria-pressed", String(editMode));
  addCardButton.hidden = !editMode;
  densityTag.hidden = !editMode;
  modeLabel.textContent = editMode ? "Edit mode" : "Dashboard";
  renderEmptyState(store.getSnapshot());

  if (!editMode) {
    setAddCardPopoverOpen(false);
    clearActiveCard();
    hideDropOverlay();
    closeLocalPackagesModal();
    closeDeleteCardModal();
  }

  widgetBridge.syncFrames();
}

function isTypingTarget(target) {
  return Boolean(
    target?.closest("input, textarea, select, [contenteditable='true']"),
  );
}

function addWorkspace() {
  setAddCardPopoverOpen(false);
  queueWorkspaceTransition(1);
  clearActiveCard();
  const created = store.addWorkspace();
  setWorkspacePopoverOpen(false);
  return created;
}

function switchWorkspace(workspaceId) {
  setAddCardPopoverOpen(false);
  const snapshot = store.getSnapshot();
  const currentIndex = getActiveWorkspaceIndex(snapshot);
  const nextIndex = snapshot.workspaces.findIndex(
    (workspace) => workspace.id === workspaceId,
  );
  if (nextIndex < 0) {
    setWorkspacePopoverOpen(false);
    return;
  }

  const direction = resolveWorkspaceDirection(currentIndex, nextIndex);
  if (!direction) {
    setWorkspacePopoverOpen(false);
    return;
  }

  queueWorkspaceTransition(direction);
  clearActiveCard();
  store.switchWorkspace(workspaceId);
  setWorkspacePopoverOpen(false);
}

function cycleWorkspace(direction) {
  setAddCardPopoverOpen(false);
  const snapshot = store.getSnapshot();
  if (snapshot.workspaces.length <= 1) {
    return;
  }

  queueWorkspaceTransition(direction);
  clearActiveCard();
  setWorkspacePopoverOpen(false);
  store.cycleWorkspace(direction);
}

function jumpToWorkspace(index) {
  setAddCardPopoverOpen(false);
  const snapshot = store.getSnapshot();
  const currentIndex = getActiveWorkspaceIndex(snapshot);
  if (index < 0 || index >= snapshot.workspaces.length) {
    return;
  }

  const direction = resolveWorkspaceDirection(currentIndex, index);
  if (!direction) {
    setWorkspacePopoverOpen(false);
    return;
  }

  queueWorkspaceTransition(direction);
  clearActiveCard();
  setWorkspacePopoverOpen(false);
  store.jumpToWorkspace(index);
}

function removeWorkspace(workspaceId) {
  setAddCardPopoverOpen(false);
  const snapshot = store.getSnapshot();
  const currentIndex = getActiveWorkspaceIndex(snapshot);
  const isActiveWorkspace = snapshot.activeWorkspaceId === workspaceId;
  if (isActiveWorkspace) {
    queueWorkspaceTransition(
      currentIndex >= snapshot.workspaces.length - 1 ? -1 : 1,
    );
  }

  clearActiveCard();
  const removed = store.removeWorkspace(workspaceId);

  if (!removed) {
    clearWorkspaceTransition();
    flashBoardBlocked();
    return;
  }

  setWorkspacePopoverOpen(false);
}

function primeStartupSequence() {
  startupPaths.forEach((path, index) => {
    const length = Math.ceil(path.getTotalLength());
    path.style.setProperty("--path-length", String(length));
    path.style.setProperty("--stroke-delay", `${240 + index * 150}ms`);
  });
}

function finishStartupSequence(reduceMotion) {
  document.body.classList.add("startup-complete");

  window.setTimeout(
    () => {
      startupScreen.hidden = true;
      document.body.classList.remove("startup-active");
      document.body.classList.remove("auth-pending");
    },
    reduceMotion ? 220 : 680,
  );
}

function playStartupSequence() {
  const reduceMotion = window.matchMedia(
    "(prefers-reduced-motion: reduce)",
  ).matches;
  startupStatusPanel.hidden = false;
  startupProgress.hidden = false;
  startupStatusLabel.textContent = "Board ready";
  startupStatusValue.textContent = "100%";
  startupProgressFill.style.transform = "scaleX(1)";
  window.setTimeout(
    () => {
      finishStartupSequence(reduceMotion);
    },
    reduceMotion ? 120 : 280,
  );
}

function setStartupError(message) {
  const text = String(message || "").trim();
  startupErrorMessage.textContent = text;
  startupErrorMessage.hidden = !text;
}

async function parseJsonResponse(response) {
  return response.json().catch(() => null);
}

function showStartupLoadingState(
  label = "Loading local workspace",
  progress = 0.12,
) {
  startupStatusPanel.hidden = false;
  startupProgress.hidden = false;
  startupStatusLabel.textContent = label;
  startupStatusValue.textContent = `${Math.round(progress * 100)}%`;
  startupProgressFill.style.transform = `scaleX(${progress})`;
}

async function requestServerProfiles() {
  const response = await fetch("/host/servers");
  const payload = await parseJsonResponse(response);
  if (!response.ok) {
    throw new Error(payload?.error || "Falha ao carregar os servidores.");
  }

  return Array.isArray(payload) ? payload : [];
}

function syncServerOptions(selectedId = "") {
  const placeholder = '<option value="">Escolha um servidor</option>';
  const options = serverProfiles.map(
    (server) =>
      `<option value="${escapeHtml(server.id)}">${escapeHtml(server.name)}</option>`,
  );
  widgetConfigServerId.innerHTML = [placeholder, ...options].join("");
  widgetConfigServerId.value = serverProfiles.some(
    (server) => server.id === selectedId,
  )
    ? selectedId
    : "";
}

async function refreshServerProfiles() {
  const profiles = await requestServerProfiles();
  syncServerProfiles(profiles);
  syncServerOptions(widgetConfigServerId.value);
  renderSnapshot(store.getSnapshot());
  return serverProfiles;
}

function setServerLoginError(message) {
  const text = String(message || "").trim();
  serverLoginErrorMessage.textContent = text;
  serverLoginErrorMessage.hidden = !text;
}

function setServerLoginPending(pending) {
  document.body.classList.toggle("auth-pending", pending);
  serverLoginUsernameInput.disabled = pending;
  serverLoginPasswordInput.disabled = pending;
  serverLoginPasswordToggle.disabled = pending;
  serverLoginCancelButton.disabled = pending;
  serverLoginCloseButton.disabled = pending;
  serverLoginConfirmButton.disabled = pending;
}

function syncPasswordVisibility(visible) {
  serverLoginPasswordInput.type = visible ? "text" : "password";
  serverLoginPasswordToggle.setAttribute(
    "aria-label",
    visible ? "Ocultar senha" : "Mostrar senha",
  );
  serverLoginPasswordToggle.setAttribute("aria-pressed", String(visible));
  serverLoginPasswordToggle.classList.toggle("is-active", visible);
}

function openServerLoginModal(serverId) {
  const server = getServerProfile(serverId);
  if (!server) {
    flashDropOverlayMessage("Servidor nao encontrado para esse widget.");
    return;
  }

  pendingServerLogin = server.id;
  serverLoginServerName.textContent = server.name;
  serverLoginModalDescription.textContent =
    "Use suas credenciais desse servidor para desbloquear os widgets dependentes.";
  serverLoginUsernameInput.value = server.usernameHint || "";
  serverLoginPasswordInput.value = "";
  setServerLoginError("");
  setServerLoginPending(false);
  syncPasswordVisibility(false);
  serverLoginModalBackdrop.hidden = false;
  syncModalLock();
  window.setTimeout(() => {
    serverLoginUsernameInput.focus();
    serverLoginUsernameInput.select();
  }, 0);
}

function closeServerLoginModal() {
  pendingServerLogin = null;
  serverLoginModalBackdrop.hidden = true;
  setServerLoginPending(false);
  setServerLoginError("");
  serverLoginPasswordInput.value = "";
  syncPasswordVisibility(false);
  syncModalLock();
}

async function submitServerLogin(serverId, username, password) {
  const response = await fetch(
    `/host/servers/${encodeURIComponent(serverId)}/session`,
    {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
      },
      body: JSON.stringify({
        username,
        password,
      }),
    },
  );

  const payload = await parseJsonResponse(response);
  if (!response.ok) {
    throw new Error(
      payload?.error || "Falha ao autenticar nesse servidor remoto.",
    );
  }

  return payload;
}

async function handleServerLoginFormSubmit(event) {
  event.preventDefault();

  if (!pendingServerLogin) {
    return;
  }

  const username = serverLoginUsernameInput.value.trim();
  const password = serverLoginPasswordInput.value;
  if (!username || !password) {
    setServerLoginError("Preencha login e senha do servidor.");
    return;
  }

  setServerLoginError("");
  setServerLoginPending(true);

  try {
    await submitServerLogin(pendingServerLogin, username, password);
    await refreshServerProfiles();
    closeServerLoginModal();
  } catch (error) {
    setServerLoginError(
      error instanceof Error
        ? error.message
        : "Falha ao autenticar nesse servidor remoto.",
    );
    serverLoginPasswordInput.focus();
  } finally {
    setServerLoginPending(false);
  }
}

function setWidgetConfigHelp(message) {
  const text = String(message || "").trim();
  widgetConfigHelp.textContent = text;
  widgetConfigHelp.hidden = !text;
}

function syncWidgetConfigDebug(card) {
  if (!card) {
    return;
  }

  const parts = [];

  if (cardRequiresViewId(card)) {
    parts.push("Escolha um servidor e informe o view id salvo nesse backend.");
  } else if (cardRequiresServer(card)) {
    parts.push("Escolha um servidor para esse widget.");
  } else {
    parts.push("Esse widget nao precisa de servidor.");
  }

  if (cardSupportsStream(card)) {
    parts.push(
      "Use o toggle de stream para pausar esse widget sem perder a configuracao.",
    );
  }

  setWidgetConfigHelp(parts.join(" "));
}

function openWidgetConfigModal(cardId) {
  const card = getCardById(cardId);
  if (!card) {
    flashBoardBlocked();
    return;
  }

  pendingWidgetConfigCardId = card.id;
  widgetConfigModalDescription.textContent = `Defina o servidor e os parametros usados por ${card.title}.`;
  syncServerOptions(card.serverId || "");
  widgetConfigViewIdField.hidden = !cardRequiresViewId(card);
  widgetConfigViewId.value =
    card.viewId == null ? "" : String(card.viewId || "");
  if (widgetConfigStreamsField) {
    widgetConfigStreamsField.hidden = !cardSupportsStream(card);
  }
  if (widgetConfigStreamsEnabled) {
    widgetConfigStreamsEnabled.checked = card.streamsEnabled !== false;
  }

  if (!serverProfiles.length && cardRequiresServer(card)) {
    widgetConfigSaveButton.disabled = true;
    setWidgetConfigHelp(
      "Nenhum servidor configurado. Edite ~/.config/lince/web/servers.json e recarregue a pagina.",
    );
  } else if (cardRequiresViewId(card)) {
    widgetConfigSaveButton.disabled = false;
    syncWidgetConfigDebug(card);
  } else {
    widgetConfigSaveButton.disabled = false;
    syncWidgetConfigDebug(card);
  }

  widgetConfigModalBackdrop.hidden = false;
  syncModalLock();
  window.setTimeout(() => {
    if (cardRequiresServer(card)) {
      widgetConfigServerId.focus();
      return;
    }

    widgetConfigViewId.focus();
  }, 0);
}

function closeWidgetConfigModal() {
  pendingWidgetConfigCardId = null;
  widgetConfigModalBackdrop.hidden = true;
  widgetConfigSaveButton.disabled = false;
  setWidgetConfigHelp("");
  syncModalLock();
}

function saveWidgetConfig(cardId, nextServerId, nextViewId) {
  const nextStreamsEnabled = widgetConfigStreamsEnabled
    ? widgetConfigStreamsEnabled.checked
    : null;
  store.updateCard(
    cardId,
    (card) => ({
      ...card,
      serverId: nextServerId,
      viewId: nextViewId,
      streamsEnabled: cardSupportsStream(card)
        ? (nextStreamsEnabled ?? card.streamsEnabled !== false)
        : card.streamsEnabled !== false,
    }),
    { persist: true },
  );
}

function handleWidgetConfigFormSubmit(event) {
  event.preventDefault();

  if (!pendingWidgetConfigCardId) {
    return;
  }

  const card = getCardById(pendingWidgetConfigCardId);
  if (!card) {
    closeWidgetConfigModal();
    return;
  }

  const nextServerId = cardRequiresServer(card)
    ? widgetConfigServerId.value.trim()
    : "";
  if (cardRequiresServer(card) && !nextServerId) {
    setWidgetConfigHelp("Escolha um servidor para esse widget.");
    widgetConfigServerId.focus();
    return;
  }

  let nextViewId = null;
  if (cardRequiresViewId(card)) {
    const parsedViewId = Number(widgetConfigViewId.value);
    if (!Number.isInteger(parsedViewId) || parsedViewId <= 0) {
      setWidgetConfigHelp("Informe um view id inteiro maior que zero.");
      widgetConfigViewId.focus();
      return;
    }

    nextViewId = parsedViewId;
  }

  saveWidgetConfig(pendingWidgetConfigCardId, nextServerId, nextViewId);
  closeWidgetConfigModal();
}

async function bootWorkspace() {
  primeStartupSequence();
  renderSnapshot(store.getSnapshot());
  renderLocalPackageList();
  setStartupError("");

  showStartupLoadingState("Loading local workspace", 0.14);

  try {
    showStartupLoadingState("Loading server profiles", 0.34);
    await refreshServerProfiles();
  } catch (error) {
    setStartupError(
      error instanceof Error
        ? error.message
        : "Falha ao carregar os servidores configurados.",
    );
  }

  try {
    showStartupLoadingState("Loading local widgets", 0.68);
    await loadInstalledPackages();
  } catch (error) {
    setStartupError(
      error instanceof Error
        ? error.message
        : "Falha ao carregar o catalogo local de widgets.",
    );
  }

  showStartupLoadingState("Preparing board", 0.9);
  playStartupSequence();
}

function renderPermissionsList(permissions) {
  const safePermissions = Array.isArray(permissions) ? permissions : [];
  importPermissionsList.innerHTML = safePermissions.length
    ? safePermissions
        .map(
          (permission) =>
            `
              <li class="permission-item">
                <span class="permission-pill">${escapeHtml(permission)}</span>
                <span class="permission-item__description">${escapeHtml(describePermission(permission))}</span>
              </li>
            `,
        )
        .join("")
    : `
        <li class="permission-item">
          <span class="permission-pill permission-pill--muted">Nenhuma permissao mock solicitada.</span>
          <span class="permission-item__description">Esse widget funciona fechado nele mesmo, sem pedir acesso adicional.</span>
        </li>
      `;
}

function describePermission(permission) {
  return (
    PERMISSION_DESCRIPTIONS[permission] ||
    "Permissao mock declarada pelo package para futuras integracoes e capacidades opcionais."
  );
}

function syncModalLock() {
  document.documentElement.classList.toggle(
    "modal-open",
    !importModalBackdrop.hidden ||
      !localPackagesModalBackdrop.hidden ||
      !deleteCardModalBackdrop.hidden ||
      !serverLoginModalBackdrop.hidden ||
      !widgetConfigModalBackdrop.hidden,
  );
}

function getCardById(cardId) {
  return getCardRecord(cardId)?.card || null;
}

function renderImportPreview(preview) {
  const snapshot = store.getSnapshot();
  const cols = snapshot.layout.cols;
  const rows = snapshot.layout.rows;
  const cardWidth = Math.max(
    1,
    Math.min(cols, Number(preview.initial_width) || 3),
  );
  const cardHeight = Math.max(
    1,
    Math.min(rows, Number(preview.initial_height) || 2),
  );
  const cardX = Math.max(1, Math.floor((cols - cardWidth) / 2) + 1);
  const cardY = Math.max(1, Math.floor((rows - cardHeight) / 2) + 1);
  const previewOverlay = importPreviewCard.parentElement;

  importPreviewDensity.textContent = `${cols} x ${rows} grid ativa · card ${cardWidth} x ${cardHeight}`;
  importPreviewCells.style.setProperty("--preview-cols", String(cols));
  importPreviewCells.style.setProperty("--preview-rows", String(rows));
  importPreviewCells.style.setProperty(
    "--preview-gap",
    `${Math.max(8, snapshot.layout.gap - 4)}px`,
  );
  previewOverlay?.style.setProperty("--preview-cols", String(cols));
  previewOverlay?.style.setProperty("--preview-rows", String(rows));
  previewOverlay?.style.setProperty(
    "--preview-gap",
    `${Math.max(8, snapshot.layout.gap - 4)}px`,
  );
  importPreviewCard.style.gridColumn = `${cardX} / span ${cardWidth}`;
  importPreviewCard.style.gridRow = `${cardY} / span ${cardHeight}`;

  const signature = `${cols}:${rows}`;
  if (importPreviewCells.dataset.signature !== signature) {
    importPreviewCells.dataset.signature = signature;
    importPreviewCells.innerHTML = Array.from(
      { length: cols * rows },
      () => '<div class="import-preview-cells__cell"></div>',
    ).join("");
  }
}

function openImportModal(preview, file = null) {
  pendingImportPreview = preview;
  pendingImportFile = file;
  importModalTitle.textContent = preview.title;
  importModalDescription.textContent = preview.description;
  importPackageName.textContent = preview.filename;
  importAuthor.textContent = preview.author;
  importVersion.textContent = preview.version || "0.1.0";
  importSize.textContent = `${preview.initial_width} x ${preview.initial_height}`;
  importModalDetails.textContent = preview.details || preview.description;
  renderImportPreview(preview);
  importPreviewFrame.setAttribute("srcdoc", enhancePackageHtml(preview.html));
  renderPermissionsList(preview.permissions);
  importModalBackdrop.hidden = false;
  syncModalLock();
  widgetBridge.syncFrames();
}

function closeImportModal() {
  pendingImportPreview = null;
  pendingImportFile = null;
  importPreviewFrame.setAttribute("srcdoc", "");
  importModalBackdrop.hidden = true;
  syncModalLock();
}

function openDeleteCardModal(cardId) {
  const card = getCardById(cardId);
  if (!card) {
    flashBoardBlocked();
    return;
  }

  pendingDeleteCardId = card.id;
  deleteCardModalTitle.textContent = "Excluir card?";
  deleteCardModalName.textContent = card.title;
  deleteCardModalDescription.textContent =
    card.kind === "package"
      ? "Esse widget sera removido do workspace atual. O arquivo original nao sera alterado."
      : "Esse card sera removido do workspace atual e o espaco volta a ficar livre na grid.";
  deleteCardModalBackdrop.hidden = false;
  syncModalLock();
}

function closeDeleteCardModal() {
  pendingDeleteCardId = null;
  deleteCardModalName.textContent = "";
  deleteCardModalBackdrop.hidden = true;
  syncModalLock();
}

function showDropOverlay(message, options = {}) {
  if (dropOverlayFlashTimeout) {
    window.clearTimeout(dropOverlayFlashTimeout);
    dropOverlayFlashTimeout = null;
  }

  const variant = options.variant || "mixed";
  dropZoneOverlay.hidden = false;
  dropZoneOverlay.dataset.state = options.locked ? "loading" : "ready";
  dropZoneOverlay.dataset.variant = variant;
  dropZoneOverlayEyebrow.textContent =
    variant === "workspace"
      ? "Import workspace"
      : variant === "package"
        ? "Import widget"
        : "Import local file";
  dropZoneOverlayTitle.textContent =
    variant === "workspace"
      ? "Solte um arquivo .workspace.sand"
      : variant === "package"
        ? "Solte um arquivo .html"
        : "Solte um .html ou .workspace.sand";
  dropZoneOverlayCopy.textContent = message;
}

function hideDropOverlay() {
  if (dropOverlayFlashTimeout) {
    window.clearTimeout(dropOverlayFlashTimeout);
    dropOverlayFlashTimeout = null;
  }

  dropHoverDepth = 0;
  dropZoneOverlay.hidden = true;
  dropZoneOverlay.dataset.state = "ready";
  dropZoneOverlay.dataset.variant = "mixed";
  dropZoneOverlayEyebrow.textContent = "Import local file";
  dropZoneOverlayTitle.textContent = "Solte um .html ou .workspace.sand";
  dropZoneOverlayCopy.textContent = DEFAULT_DROP_MESSAGE;
}

function flashDropOverlayMessage(message) {
  showDropOverlay(message, { locked: true });
  dropOverlayFlashTimeout = window.setTimeout(() => {
    hideDropOverlay();
  }, 1600);
}

function hasFilePayload(dataTransfer) {
  return (
    Array.from(dataTransfer?.items || []).some(
      (item) => item.kind === "file",
    ) || Number(dataTransfer?.files?.length || 0) > 0
  );
}

function isLinceFile(file) {
  const name = String(file?.name || "").toLowerCase();
  return (
    name.endsWith(PACKAGE_EXTENSION) ||
    name.endsWith(LEGACY_PACKAGE_EXTENSION) ||
    name.endsWith(LEGACY_PACKAGE_ARCHIVE_EXTENSION)
  );
}

function isWorkspaceArchiveFile(file) {
  const name = String(file?.name || "").toLowerCase();
  return (
    name.endsWith(WORKSPACE_ARCHIVE_EXTENSION) ||
    name.endsWith(LEGACY_WORKSPACE_ARCHIVE_EXTENSION)
  );
}

function getDroppedLinceFile(dataTransfer) {
  return (
    Array.from(dataTransfer?.files || []).find(
      (file) => isLinceFile(file) && !isWorkspaceArchiveFile(file),
    ) || null
  );
}

function getDroppedWorkspaceFile(dataTransfer) {
  return (
    Array.from(dataTransfer?.files || []).find(isWorkspaceArchiveFile) || null
  );
}

async function requestPackagePreview(file) {
  const formData = new FormData();
  formData.append("package", file);

  const response = await fetch(apiPath("/api/packages/preview"), {
    method: "POST",
    body: formData,
  });
  const payload = await response.json().catch(() => null);

  if (!response.ok) {
    throw new Error(payload?.error || "Nao foi possivel ler o widget HTML.");
  }

  return payload;
}

async function importLinceFile(file) {
  showDropOverlay(`Lendo ${file.name}...`, {
    locked: true,
    variant: "package",
  });

  try {
    const preview = await requestPackagePreview(file);
    hideDropOverlay();
    openImportModal(preview, file);
  } catch (error) {
    flashDropOverlayMessage(
      error instanceof Error ? error.message : "Falha ao importar.",
    );
  }
}

async function requestWorkspaceImport(file) {
  const formData = new FormData();
  formData.append("workspace", file);

  const response = await fetch(apiPath("/api/board/workspaces/import"), {
    method: "POST",
    body: formData,
  });
  const payload = await parseJsonResponse(response);
  if (!response.ok) {
    throw new Error(payload?.error || "Nao foi possivel importar o workspace.");
  }

  return payload;
}

async function importWorkspaceFile(file) {
  showDropOverlay(`Importando ${file.name}...`, {
    locked: true,
    variant: "workspace",
  });

  try {
    queueWorkspaceTransition(1);
    const workspace = await requestWorkspaceImport(file);
    store.appendWorkspace(workspace, { persist: false, activate: true });
    hideDropOverlay();
    setWorkspacePopoverOpen(false);
  } catch (error) {
    clearWorkspaceTransition();
    flashDropOverlayMessage(
      error instanceof Error ? error.message : "Falha ao importar workspace.",
    );
  }
}

function resolvePreviewSize(preview, fallback = null) {
  const width = Math.max(
    1,
    Number(preview?.initial_width ?? preview?.initialWidth) ||
      Number(
        fallback?.initial_width ?? fallback?.initialWidth ?? fallback?.w,
      ) ||
      3,
  );
  const height = Math.max(
    1,
    Number(preview?.initial_height ?? preview?.initialHeight) ||
      Number(
        fallback?.initial_height ?? fallback?.initialHeight ?? fallback?.h,
      ) ||
      2,
  );

  return { w: width, h: height };
}

function defaultServerIdForPreview(preview) {
  const permissions = Array.isArray(preview?.permissions)
    ? preview.permissions
    : [];
  if (
    !permissions.includes("read_view_stream") &&
    !permissions.includes("write_records")
  ) {
    return "";
  }

  return serverProfiles[0]?.id || "";
}

function createCardFromPreview(preview, sizeOverride = null) {
  const size = sizeOverride || resolvePreviewSize(preview);
  return store.addImportedCard({
    title: preview.title,
    description: preview.description,
    author: preview.author,
    permissions: preview.permissions,
    packageName: preview.filename,
    html: preview.html,
    serverId: defaultServerIdForPreview(preview),
    viewId:
      Array.isArray(preview?.permissions) &&
      preview.permissions.includes("read_view_stream")
        ? 1
        : null,
    w: size.w,
    h: size.h,
  });
}

async function addLocalPackageToWorkspace(packageId) {
  const preview = await requestInstalledPackage(packageId);
  const packageSummary =
    installedPackages.find((entry) => entry.id === packageId) || null;
  const created = createCardFromPreview(
    preview,
    resolvePreviewSize(packageSummary, preview),
  );
  if (!created) {
    throw new Error("Nao encontrei espaco livre na grid para esse widget.");
  }

  setActiveCard(created.id, null);
  closeLocalPackagesModal();
  return created;
}

function triggerWorkspaceExport() {
  const snapshot = store.getSnapshot();
  const activeWorkspace = snapshot.workspaces.find(
    (workspace) => workspace.id === snapshot.activeWorkspaceId,
  );
  if (!activeWorkspace) {
    flashBoardBlocked();
    return;
  }

  const anchor = document.createElement("a");
  anchor.href = apiPath(
    `/api/board/workspaces/${encodeURIComponent(activeWorkspace.id)}/export`,
  );
  anchor.download = `${activeWorkspace.name || "workspace"}.workspace.sand`;
  anchor.rel = "noreferrer";
  document.body.appendChild(anchor);
  anchor.click();
  anchor.remove();
  setWorkspacePopoverOpen(false);
}

function workspaceExportPayload() {
  const snapshot = store.getSnapshot();
  const activeWorkspace = snapshot.workspaces.find(
    (workspace) => workspace.id === snapshot.activeWorkspaceId,
  );
  if (!activeWorkspace) {
    return null;
  }

  const slug =
    String(activeWorkspace.name || "workspace")
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "") || "workspace";

  return {
    filename: `${slug}.workspace.sand`,
    url: `${window.location.origin}${apiPath(`/api/board/workspaces/${encodeURIComponent(activeWorkspace.id)}/export`)}`,
  };
}

async function confirmImportCard() {
  if (!pendingImportPreview || !pendingImportFile) {
    return;
  }

  importConfirmButton.disabled = true;
  importCancelButton.disabled = true;

  try {
    const installedPackage = await installUploadedPackage(pendingImportFile);
    const created = createCardFromPreview(
      installedPackage,
      resolvePreviewSize(pendingImportPreview, installedPackage),
    );

    if (!created) {
      flashBoardBlocked();
      return;
    }

    closeImportModal();
    setActiveCard(created.id, null);
  } catch (error) {
    flashDropOverlayMessage(
      error instanceof Error
        ? error.message
        : "Falha ao instalar o widget HTML.",
    );
  } finally {
    importConfirmButton.disabled = false;
    importCancelButton.disabled = false;
  }
}

function confirmDeleteCard() {
  if (!pendingDeleteCardId) {
    return;
  }

  const cardId = pendingDeleteCardId;
  closeDeleteCardModal();

  if (activeCardId === cardId) {
    activeCardId = null;
    activeInteractionType = null;
  }

  const removed = store.removeCard(cardId);
  if (!removed) {
    flashBoardBlocked();
  }
}

store.subscribe((snapshot) => {
  renderSnapshot(snapshot);
});

attachBoardInteractions({
  boardElement: boardCanvas,
  config,
  readCards: () => store.getCards(),
  replaceCards: (cards, options) => store.replaceCards(cards, options),
  isEditMode: () => editMode,
  onInteractionStart: (cardId, interactionType) => {
    setActiveCard(cardId, interactionType);
  },
  onInteractionEnd: () => {
    clearActiveCard();
  },
  onEdgeTransferPreview: (direction) => {
    setWorkspaceTransferPreview(direction);
  },
  onEdgeTransfer: (card, direction) =>
    handleWorkspaceEdgeTransfer(card, direction),
});

editToggle.addEventListener("click", () => {
  setEditMode(!editMode);
});

if (streamsToggle) {
  streamsToggle.addEventListener("click", () => {
    const snapshot = store.getSnapshot();
    store.setGlobalStreamsEnabled(snapshot.globalStreamsEnabled === false);
  });
}

addCardButton.addEventListener("click", () => {
  setWorkspacePopoverOpen(false);
  setAddCardPopoverOpen(!addCardPopoverOpen);
});

addCardImportButton.addEventListener("click", () => {
  setAddCardPopoverOpen(false);
  packageImportInput.value = "";
  packageImportInput.click();
});

addCardLocalButton.addEventListener("click", () => {
  openLocalPackagesModal();
});

workspaceToggle.addEventListener("click", () => {
  setAddCardPopoverOpen(false);
  setWorkspacePopoverOpen(!workspacePopoverOpen);
});

addWorkspaceButton.addEventListener("click", () => {
  addWorkspace();
});

importWorkspaceButton.addEventListener("click", () => {
  workspaceImportInput.value = "";
  workspaceImportInput.click();
});

exportWorkspaceButton.addEventListener("click", () => {
  triggerWorkspaceExport();
});

exportWorkspaceButton.setAttribute("draggable", "true");
exportWorkspaceButton.addEventListener("dragstart", (event) => {
  const payload = workspaceExportPayload();
  if (!payload || !event.dataTransfer) {
    return;
  }

  event.dataTransfer.effectAllowed = "copy";
  event.dataTransfer.setData("text/uri-list", payload.url);

  try {
    event.dataTransfer.setData(
      "DownloadURL",
      `application/zip:${payload.filename}:${payload.url}`,
    );
  } catch (error) {
    // Browser may not support DownloadURL.
  }
});

workspaceList.addEventListener("click", (event) => {
  const button = event.target.closest("[data-workspace-action]");
  if (!button) {
    return;
  }

  const workspaceId = button.dataset.workspaceId;
  const action = button.dataset.workspaceAction;
  if (!workspaceId || !action) {
    return;
  }

  if (action === "delete") {
    removeWorkspace(workspaceId);
    return;
  }

  switchWorkspace(workspaceId);
});

densitySlider.addEventListener("input", (event) => {
  clearActiveCard();
  store.updateDensity(Number(event.currentTarget.value));
});

cardsLayer.addEventListener("click", (event) => {
  const actionButton = event.target.closest("[data-card-action]");
  if (!actionButton) {
    return;
  }

  const cardId =
    actionButton.dataset.cardId ||
    actionButton.closest("[data-card-id]")?.dataset.cardId;
  if (!cardId) {
    return;
  }

  const action = actionButton.dataset.cardAction;
  if (action === "configure") {
    openWidgetConfigModal(cardId);
    return;
  }

  if (action === "connect-server") {
    const card = getCardById(cardId);
    const gate = card ? resolveCardServerState(card) : null;
    if (gate?.server) {
      openServerLoginModal(gate.server.id);
    } else {
      flashBoardBlocked();
    }
    return;
  }

  if (action !== "delete") {
    return;
  }

  const allowDeleteOutsideEdit = Boolean(
    actionButton.closest(".package-widget__locked-actions"),
  );
  if (!editMode && !allowDeleteOutsideEdit) {
    return;
  }

  openDeleteCardModal(cardId);
});

boardShell.addEventListener("dragenter", (event) => {
  if (!editMode || !hasFilePayload(event.dataTransfer)) {
    return;
  }

  event.preventDefault();
  dropHoverDepth += 1;
  showDropOverlay(DEFAULT_DROP_MESSAGE, { variant: "mixed" });
});

boardShell.addEventListener("dragover", (event) => {
  if (!editMode || !hasFilePayload(event.dataTransfer)) {
    return;
  }

  event.preventDefault();
  event.dataTransfer.dropEffect = "copy";

  if (dropZoneOverlay.hidden) {
    showDropOverlay(DEFAULT_DROP_MESSAGE);
  }
});

boardShell.addEventListener("dragleave", (event) => {
  if (!editMode || dropZoneOverlay.dataset.state === "loading") {
    return;
  }

  if (event.relatedTarget && boardShell.contains(event.relatedTarget)) {
    return;
  }

  dropHoverDepth = Math.max(0, dropHoverDepth - 1);
  if (dropHoverDepth === 0) {
    hideDropOverlay();
  }
});

boardShell.addEventListener("drop", async (event) => {
  if (!editMode || !hasFilePayload(event.dataTransfer)) {
    return;
  }

  event.preventDefault();
  const linceFile = getDroppedLinceFile(event.dataTransfer);
  const workspaceFile = getDroppedWorkspaceFile(event.dataTransfer);

  if (workspaceFile) {
    await importWorkspaceFile(workspaceFile);
    return;
  }

  if (!linceFile) {
    flashDropOverlayMessage(
      "Solte um arquivo .html ou .workspace.sand valido.",
    );
    return;
  }

  await importLinceFile(linceFile);
});

importCancelButton.addEventListener("click", () => {
  closeImportModal();
});

importCloseButton.addEventListener("click", () => {
  closeImportModal();
});

importConfirmButton.addEventListener("click", () => {
  void confirmImportCard();
});

importModalBackdrop.addEventListener("click", (event) => {
  if (event.target === importModalBackdrop) {
    closeImportModal();
  }
});

localPackagesCloseButton.addEventListener("click", () => {
  closeLocalPackagesModal();
});

localPackagesModalBackdrop.addEventListener("click", (event) => {
  if (event.target === localPackagesModalBackdrop) {
    closeLocalPackagesModal();
  }
});

localPackagesSearch.addEventListener("input", () => {
  renderLocalPackageList();
});

localPackageList.addEventListener("click", (event) => {
  const card = event.target.closest("[data-local-package-id]");
  if (!card?.dataset.localPackageId) {
    return;
  }

  void addLocalPackageToWorkspace(card.dataset.localPackageId).catch(
    (error) => {
      localPackagesSummary.textContent =
        error instanceof Error
          ? error.message
          : "Falha ao adicionar o widget local.";
    },
  );
});

deleteCardCancelButton.addEventListener("click", () => {
  closeDeleteCardModal();
});

deleteCardCloseButton.addEventListener("click", () => {
  closeDeleteCardModal();
});

deleteCardConfirmButton.addEventListener("click", () => {
  confirmDeleteCard();
});

deleteCardModalBackdrop.addEventListener("click", (event) => {
  if (event.target === deleteCardModalBackdrop) {
    closeDeleteCardModal();
  }
});

serverLoginForm.addEventListener("submit", (event) => {
  void handleServerLoginFormSubmit(event);
});

serverLoginPasswordToggle.addEventListener("click", () => {
  syncPasswordVisibility(serverLoginPasswordInput.type === "password");
});

serverLoginCancelButton.addEventListener("click", () => {
  closeServerLoginModal();
});

serverLoginCloseButton.addEventListener("click", () => {
  closeServerLoginModal();
});

serverLoginModalBackdrop.addEventListener("click", (event) => {
  if (event.target === serverLoginModalBackdrop) {
    closeServerLoginModal();
  }
});

widgetConfigForm.addEventListener("submit", (event) => {
  handleWidgetConfigFormSubmit(event);
});

widgetConfigCancelButton.addEventListener("click", () => {
  closeWidgetConfigModal();
});

widgetConfigCloseButton.addEventListener("click", () => {
  closeWidgetConfigModal();
});

widgetConfigModalBackdrop.addEventListener("click", (event) => {
  if (event.target === widgetConfigModalBackdrop) {
    closeWidgetConfigModal();
  }
});

widgetConfigServerId.addEventListener("change", () => {
  const card = pendingWidgetConfigCardId
    ? getCardById(pendingWidgetConfigCardId)
    : null;
  syncWidgetConfigDebug(card);
});

widgetConfigViewId.addEventListener("input", () => {
  const card = pendingWidgetConfigCardId
    ? getCardById(pendingWidgetConfigCardId)
    : null;
  syncWidgetConfigDebug(card);
});

if (widgetConfigStreamsEnabled) {
  widgetConfigStreamsEnabled.addEventListener("change", () => {
    const card = pendingWidgetConfigCardId
      ? getCardById(pendingWidgetConfigCardId)
      : null;
    syncWidgetConfigDebug(card);
  });
}

packageImportInput.addEventListener("change", () => {
  const file = packageImportInput.files?.[0];
  if (!file) {
    return;
  }

  void importLinceFile(file);
});

workspaceImportInput.addEventListener("change", () => {
  const file = workspaceImportInput.files?.[0];
  if (!file) {
    return;
  }

  void importWorkspaceFile(file);
});

document.addEventListener("pointerdown", (event) => {
  if (!workspacePopoverOpen) {
    if (
      addCardPopoverOpen &&
      !addCardPopover.contains(event.target) &&
      !addCardButton.contains(event.target)
    ) {
      setAddCardPopoverOpen(false);
    }
    return;
  }

  if (workspaceSwitcher.contains(event.target)) {
    return;
  }

  setWorkspacePopoverOpen(false);

  if (
    addCardPopoverOpen &&
    !addCardPopover.contains(event.target) &&
    !addCardButton.contains(event.target)
  ) {
    setAddCardPopoverOpen(false);
  }
});

document.addEventListener("keydown", (event) => {
  if (!serverLoginModalBackdrop.hidden && event.key === "Escape") {
    event.preventDefault();
    closeServerLoginModal();
    return;
  }

  if (!widgetConfigModalBackdrop.hidden && event.key === "Escape") {
    event.preventDefault();
    closeWidgetConfigModal();
    return;
  }

  if (!deleteCardModalBackdrop.hidden && event.key === "Escape") {
    event.preventDefault();
    closeDeleteCardModal();
    return;
  }

  if (!localPackagesModalBackdrop.hidden && event.key === "Escape") {
    event.preventDefault();
    closeLocalPackagesModal();
    return;
  }

  if (!importModalBackdrop.hidden && event.key === "Escape") {
    event.preventDefault();
    closeImportModal();
    return;
  }

  if (addCardPopoverOpen && event.key === "Escape") {
    event.preventDefault();
    setAddCardPopoverOpen(false);
    return;
  }

  if (workspacePopoverOpen && event.key === "Escape") {
    event.preventDefault();
    setWorkspacePopoverOpen(false);
    return;
  }

  if (isTypingTarget(event.target)) {
    return;
  }

  const key = event.key.toLowerCase();

  if ((event.metaKey || event.ctrlKey) && event.shiftKey && key === "n") {
    event.preventDefault();
    addWorkspace();
    return;
  }

  if (matchesWorkspaceArrowShortcut(event, 1)) {
    event.preventDefault();
    cycleWorkspace(1);
    return;
  }

  if (matchesWorkspaceArrowShortcut(event, -1)) {
    event.preventDefault();
    cycleWorkspace(-1);
    return;
  }

  if (event.altKey && /^[1-9]$/.test(event.key)) {
    event.preventDefault();
    jumpToWorkspace(Number(event.key) - 1);
  }
});

setWorkspacePopoverOpen(false);
setEditMode(false);
syncServerProfiles(serverProfiles);
syncServerOptions("");
syncPasswordVisibility(false);
void bootWorkspace();

window.LinceBoard = {
  addCard() {
    if (!editMode) {
      return;
    }

    setAddCardPopoverOpen(!addCardPopoverOpen);
  },
  addWorkspace,
  exportWorkspace() {
    triggerWorkspaceExport();
  },
  importWorkspace() {
    workspaceImportInput.value = "";
    workspaceImportInput.click();
  },
  nextWorkspace() {
    cycleWorkspace(1);
  },
  previousWorkspace() {
    cycleWorkspace(-1);
  },
  removeCard(cardId) {
    openDeleteCardModal(cardId);
  },
  removeWorkspace,
  setDensity(level) {
    clearActiveCard();
    store.updateDensity(level);
  },
  setEditMode,
  switchWorkspace,
  toggleWorkspacePopover() {
    setWorkspacePopoverOpen(!workspacePopoverOpen);
  },
};
