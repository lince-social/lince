const state = {
  configured: false,
  status: null,
  draft: null,
  history: [],
  loading: false,
  sizeSaving: false,
  selectedModel: null,
  previewMode: "static",
  resizing: null,
};

const elements = {};
function apiPath(path) {
  if (path.startsWith("http://") || path.startsWith("https://")) {
    return path;
  }

  if (path.startsWith("/host/")) {
    return path;
  }

  return `/host${path.startsWith("/") ? path : `/${path}`}`;
}

document.addEventListener("DOMContentLoaded", () => {
  bindElements();
  bindEvents();
  hydrate();
});

function bindElements() {
  elements.keyGate = document.getElementById("ai-key-gate");
  elements.keyForm = document.getElementById("ai-key-form");
  elements.keyInput = document.getElementById("ai-api-key");
  elements.keyFeedback = document.getElementById("ai-key-form-feedback");
  elements.changeKeyButton = document.getElementById("ai-change-key-button");

  elements.statusPill = document.getElementById("ai-status-pill");
  elements.statusCopy = document.getElementById("ai-status-copy");
  elements.modelName = document.getElementById("ai-model-name");
  elements.modelSummary = document.getElementById("ai-model-summary");
  elements.modelPrice = document.getElementById("ai-model-price");
  elements.tokenEstimate = document.getElementById("ai-token-estimate");
  elements.refineEstimate = document.getElementById("ai-refine-estimate");
  elements.keyStorage = document.getElementById("ai-key-storage");
  elements.modelList = document.getElementById("ai-model-list");
  elements.modelNote = document.getElementById("ai-model-note");

  elements.generateForm = document.getElementById("ai-generate-form");
  elements.promptInput = document.getElementById("ai-prompt-input");
  elements.generateButton = document.getElementById("ai-generate-button");
  elements.resetDraftButton = document.getElementById("ai-reset-draft-button");
  elements.errorBanner = document.getElementById("ai-error-banner");
  elements.historyList = document.getElementById("ai-history-list");

  elements.previewTitle = document.getElementById("ai-preview-title");
  elements.previewSubtitle = document.getElementById("ai-preview-subtitle");
  elements.previewDimensions = document.getElementById("ai-preview-dimensions");
  elements.previewStage = document.getElementById("ai-preview-stage");
  elements.previewGrid =
    elements.previewStage.querySelector(".ai-preview-grid");
  elements.previewCard = document.getElementById("ai-preview-card");
  elements.previewFrame = document.getElementById("ai-preview-frame");
  elements.previewEmpty = document.getElementById("ai-preview-empty");
  elements.exportLink = document.getElementById("ai-export-link");
  elements.previewStaticButton = document.getElementById(
    "ai-preview-static-button",
  );
  elements.previewEditButton = document.getElementById(
    "ai-preview-edit-button",
  );
  elements.previewEditControls = document.getElementById(
    "ai-preview-edit-controls",
  );
  elements.widthDecrease = document.getElementById("ai-width-decrease");
  elements.widthIncrease = document.getElementById("ai-width-increase");
  elements.heightDecrease = document.getElementById("ai-height-decrease");
  elements.heightIncrease = document.getElementById("ai-height-increase");
  elements.widthValue = document.getElementById("ai-width-value");
  elements.heightValue = document.getElementById("ai-height-value");
  elements.previewHandles = Array.from(
    document.querySelectorAll("[data-size-handle]"),
  );

  elements.authorValue = document.getElementById("ai-author-value");
  elements.versionValue = document.getElementById("ai-version-value");
  elements.descriptionValue = document.getElementById("ai-description-value");
  elements.detailsValue = document.getElementById("ai-details-value");
  elements.permissionsList = document.getElementById("ai-permissions-list");
  elements.configPreview = document.getElementById("ai-config-preview");
  elements.usageSummary = document.getElementById("ai-usage-summary");
}

function bindEvents() {
  elements.keyForm.addEventListener("submit", handleKeySubmit);
  elements.generateForm.addEventListener("submit", handleGenerateSubmit);
  elements.resetDraftButton.addEventListener("click", resetDraft);
  elements.changeKeyButton.addEventListener("click", () => {
    elements.keyGate.hidden = false;
    elements.keyInput.focus();
  });

  elements.previewStaticButton.addEventListener("click", () =>
    setPreviewMode("static"),
  );
  elements.previewEditButton.addEventListener("click", () =>
    setPreviewMode("edit"),
  );
  elements.widthDecrease.addEventListener("click", () =>
    nudgeDraftSize("width", -1),
  );
  elements.widthIncrease.addEventListener("click", () =>
    nudgeDraftSize("width", 1),
  );
  elements.heightDecrease.addEventListener("click", () =>
    nudgeDraftSize("height", -1),
  );
  elements.heightIncrease.addEventListener("click", () =>
    nudgeDraftSize("height", 1),
  );

  for (const handle of elements.previewHandles) {
    handle.addEventListener("pointerdown", handlePreviewResizeStart);
  }
}

async function hydrate() {
  renderDraft();
  renderHistory();
  renderPreviewMode();

  try {
    const status = await requestJson(apiPath("/ai/status"));
    state.status = status;
    state.configured = status.configured;
    state.selectedModel = status.model;
    renderStatus();
  } catch (error) {
    showError(error.message);
  }
}

async function handleKeySubmit(event) {
  event.preventDefault();
  const apiKey = elements.keyInput.value.trim();

  if (!apiKey) {
    renderKeyFeedback("Insira uma API key valida.", true);
    return;
  }

  renderKeyFeedback("Salvando chave no backend...", false);

  try {
    const status = await requestJson(apiPath("/ai/key"), {
      method: "POST",
      body: JSON.stringify({ api_key: apiKey }),
    });

    state.status = status;
    state.configured = true;
    state.selectedModel = state.selectedModel || status.model;
    elements.keyInput.value = "";
    elements.keyGate.hidden = true;
    clearError();
    renderKeyFeedback(
      "Geracao inicial costuma ficar em torno de 8k-16k tokens. Refinos geralmente custam menos.",
      false,
    );
    renderStatus();
    elements.promptInput.focus();
  } catch (error) {
    renderKeyFeedback(error.message, true);
  }
}

async function handleGenerateSubmit(event) {
  event.preventDefault();
  const prompt = elements.promptInput.value.trim();

  if (!state.configured) {
    elements.keyGate.hidden = false;
    renderKeyFeedback("Cadastre uma API key antes de gerar um widget.", true);
    return;
  }

  if (!prompt) {
    showError("Descreva o widget ou a modificacao antes de enviar.");
    return;
  }

  setLoading(true);
  clearError();

  try {
    const response = await requestJson(apiPath("/ai/generate"), {
      method: "POST",
      body: JSON.stringify({
        prompt,
        draft_id: state.draft?.id ?? null,
        model: state.selectedModel,
      }),
    });

    state.draft = response.draft;
    state.history.unshift({
      prompt,
      revision: response.draft.revision,
      title: response.draft.title,
      model: getSelectedModel()?.name ?? state.selectedModel ?? "-",
    });

    elements.promptInput.value = "";
    renderDraft();
    renderHistory();
  } catch (error) {
    showError(error.message);
  } finally {
    setLoading(false);
  }
}

function resetDraft() {
  state.draft = null;
  state.history = [];
  state.previewMode = "static";
  clearError();
  renderDraft();
  renderHistory();
  renderPreviewMode();
  elements.promptInput.value = "";
  elements.promptInput.focus();
}

function renderStatus() {
  const status = state.status;
  if (!status) {
    return;
  }

  if (!findModel(state.selectedModel)) {
    state.selectedModel = status.model;
  }

  renderModelList();
  renderSelectedModelSummary();

  elements.keyStorage.textContent = status.key_storage;
  elements.modelNote.textContent = status.pricing_basis;

  if (status.configured) {
    elements.statusPill.textContent = "API key pronta";
    elements.statusPill.classList.add("is-live");
    elements.statusCopy.textContent =
      "A chave esta carregada no backend Rust desta sessao. O modelo so e chamado quando voce pede uma geracao.";
    elements.keyGate.hidden = true;
  } else {
    elements.statusPill.textContent = "Aguardando API key";
    elements.statusPill.classList.remove("is-live");
    elements.statusCopy.textContent =
      "A chave vive apenas no backend desta sessao. A primeira geracao valida se ela realmente tem acesso ao modelo.";
    elements.keyGate.hidden = false;
  }
}

function renderModelList() {
  const models = state.status?.models ?? [];
  elements.modelList.innerHTML = "";

  const fragment = document.createDocumentFragment();

  for (const model of models) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "ai-model-option";
    button.classList.toggle("is-selected", model.id === state.selectedModel);
    button.setAttribute(
      "aria-pressed",
      model.id === state.selectedModel ? "true" : "false",
    );
    button.disabled = state.loading;
    button.innerHTML = `
      <div class="ai-model-option__head">
        <strong class="ai-model-option__name">${escapeHtml(model.name)}</strong>
        <span class="ai-model-option__price">${escapeHtml(model.relative_price)}</span>
      </div>
      <p class="ai-model-option__summary">${escapeHtml(model.summary)}</p>
      <div class="ai-model-option__meta">
        <span>Geracao ~${escapeHtml(model.create_token_estimate)}</span>
        <span>Refino ~${escapeHtml(model.refine_token_estimate)}</span>
      </div>
    `;
    button.addEventListener("click", () => {
      state.selectedModel = model.id;
      renderModelList();
      renderSelectedModelSummary();
    });
    fragment.append(button);
  }

  elements.modelList.append(fragment);
}

function renderSelectedModelSummary() {
  const model = getSelectedModel();
  if (!model) {
    return;
  }

  elements.modelName.textContent = model.name;
  elements.modelSummary.textContent = model.summary;
  elements.modelPrice.textContent = model.relative_price;
  elements.tokenEstimate.textContent = `Geracao: ~${model.create_token_estimate}`;
  elements.refineEstimate.textContent = `Refinos: ~${model.refine_token_estimate}`;
}

function renderDraft() {
  const draft = state.draft;

  elements.generateButton.textContent = draft
    ? "Aplicar alteracoes"
    : "Gerar widget";
  elements.exportLink.hidden = !draft;
  elements.previewCard.hidden = !draft;
  elements.previewEmpty.hidden = Boolean(draft);
  elements.previewStage.classList.toggle("has-draft", Boolean(draft));

  if (!draft) {
    elements.previewTitle.textContent = "Sem draft";
    elements.previewSubtitle.textContent = "Gere um widget para ver o preview.";
    elements.previewDimensions.textContent = "3 x 2";
    elements.authorValue.textContent = "-";
    elements.versionValue.textContent = "-";
    elements.descriptionValue.textContent = "-";
    elements.detailsValue.textContent = "-";
    elements.permissionsList.innerHTML =
      '<span class="ai-permissions-list__empty">Nenhuma permissao ainda.</span>';
    elements.configPreview.textContent =
      '{\n  "title": "Widget title",\n  "author": "Author"\n}';
    elements.usageSummary.textContent = "Sem uso registrado ainda.";
    setPreviewFrameContent(
      "<!doctype html><html><body style='margin:0;background:#0f1217;'></body></html>",
    );
    positionPreviewCard(3, 2);
    updateSizeControls(3, 2);
    renderPreviewMode();
    return;
  }

  elements.previewTitle.textContent = draft.title;
  elements.previewSubtitle.textContent = `${draft.description} · rev ${String(draft.revision).padStart(2, "0")}`;
  elements.previewDimensions.textContent = `${draft.initial_width} x ${draft.initial_height}`;
  elements.authorValue.textContent = draft.author;
  elements.versionValue.textContent = draft.version;
  elements.descriptionValue.textContent = draft.description;
  elements.detailsValue.textContent = draft.details;
  elements.configPreview.textContent =
    draft.manifest_json || buildConfigPreview(draft);
  elements.exportLink.href = draft.download_url;
  elements.exportLink.textContent = "Exportar HTML";
  elements.exportLink.setAttribute("download", draft.filename);

  renderPermissions(draft.permissions);
  renderUsage(draft.usage);
  setPreviewFrameContent(draft.html);
  positionPreviewCard(draft.initial_width, draft.initial_height);
  updateSizeControls(draft.initial_width, draft.initial_height);
  renderPreviewMode();
}

function renderPreviewMode() {
  const hasDraft = Boolean(state.draft);
  const isEditing = hasDraft && state.previewMode === "edit";
  const disableSizeControls = !isEditing || state.sizeSaving || state.loading;

  elements.previewStaticButton.classList.toggle(
    "is-active",
    state.previewMode === "static",
  );
  elements.previewStaticButton.setAttribute(
    "aria-pressed",
    state.previewMode === "static" ? "true" : "false",
  );
  elements.previewEditButton.classList.toggle(
    "is-active",
    state.previewMode === "edit",
  );
  elements.previewEditButton.setAttribute(
    "aria-pressed",
    state.previewMode === "edit" ? "true" : "false",
  );

  elements.previewEditControls.hidden = !isEditing;
  elements.previewCard.classList.toggle("is-editing", isEditing);
  elements.previewCard.classList.toggle("is-size-saving", state.sizeSaving);
  elements.previewStage.classList.toggle("is-editing", isEditing);
  elements.previewFrame.style.pointerEvents = isEditing ? "none" : "auto";

  elements.widthDecrease.disabled = disableSizeControls;
  elements.widthIncrease.disabled = disableSizeControls;
  elements.heightDecrease.disabled = disableSizeControls;
  elements.heightIncrease.disabled = disableSizeControls;
}

function renderPermissions(permissions) {
  elements.permissionsList.innerHTML = "";

  if (!permissions.length) {
    const empty = document.createElement("span");
    empty.className = "ai-permissions-list__empty";
    empty.textContent = "Nenhuma permissao mock declarada.";
    elements.permissionsList.append(empty);
    return;
  }

  for (const permission of permissions) {
    const pill = document.createElement("span");
    pill.className = "ai-permission-pill";
    pill.textContent = permission;
    elements.permissionsList.append(pill);
  }
}

function renderUsage(usage) {
  const total = formatUsageValue(usage.total_tokens);
  const input = formatUsageValue(usage.input_tokens);
  const output = formatUsageValue(usage.output_tokens);

  elements.usageSummary.innerHTML = `
    <div class="ai-usage-summary__row"><span>Total</span><strong>${total}</strong></div>
    <div class="ai-usage-summary__row"><span>Entrada</span><strong>${input}</strong></div>
    <div class="ai-usage-summary__row"><span>Saida</span><strong>${output}</strong></div>
  `;
}

function renderHistory() {
  elements.historyList.innerHTML = "";

  if (!state.history.length) {
    const item = document.createElement("li");
    item.className = "ai-history__empty";
    item.textContent = "Nenhuma geracao ainda.";
    elements.historyList.append(item);
    return;
  }

  for (const entry of state.history) {
    const item = document.createElement("li");
    item.className = "ai-history__item";
    item.innerHTML = `
      <div class="ai-history__meta">
        <span class="ai-history__revision">Revision ${String(entry.revision).padStart(2, "0")}</span>
        <span class="ai-history__model">${escapeHtml(entry.model)}</span>
      </div>
      <div class="ai-history__prompt">${escapeHtml(entry.prompt)}</div>
      <span class="ai-history__name">${escapeHtml(entry.title)}</span>
    `;
    elements.historyList.append(item);
  }
}

function setPreviewMode(mode) {
  if (mode !== "static" && mode !== "edit") {
    return;
  }

  state.previewMode = mode;
  renderPreviewMode();
}

async function nudgeDraftSize(axis, delta) {
  if (
    !state.draft ||
    state.previewMode !== "edit" ||
    state.sizeSaving ||
    state.loading
  ) {
    return;
  }

  const previous = {
    width: state.draft.initial_width,
    height: state.draft.initial_height,
  };

  const nextWidth =
    axis === "width" ? clamp(previous.width + delta, 1, 6) : previous.width;
  const nextHeight =
    axis === "height" ? clamp(previous.height + delta, 1, 6) : previous.height;

  if (nextWidth === previous.width && nextHeight === previous.height) {
    return;
  }

  applyDraftSize(nextWidth, nextHeight);
  await persistDraftSize(nextWidth, nextHeight, previous);
}

function handlePreviewResizeStart(event) {
  if (
    !state.draft ||
    state.previewMode !== "edit" ||
    state.sizeSaving ||
    state.loading
  ) {
    return;
  }

  const handle = event.currentTarget.dataset.sizeHandle;
  const gridStyle = getComputedStyle(elements.previewGrid);
  const columnGap = parseFloat(gridStyle.columnGap || "12");
  const rowGap = parseFloat(gridStyle.rowGap || "12");
  const gridRect = elements.previewGrid.getBoundingClientRect();
  const cellWidth = (gridRect.width - columnGap * 5) / 6;
  const cellHeight = (gridRect.height - rowGap * 5) / 6;

  state.resizing = {
    handle,
    startX: event.clientX,
    startY: event.clientY,
    startWidth: state.draft.initial_width,
    startHeight: state.draft.initial_height,
    persistedWidth: state.draft.initial_width,
    persistedHeight: state.draft.initial_height,
    stepX: cellWidth + columnGap,
    stepY: cellHeight + rowGap,
  };

  elements.previewCard.classList.add("is-resizing");
  document.body.classList.add("ai-builder-is-resizing");
  window.addEventListener("pointermove", handlePreviewResizeMove);
  window.addEventListener("pointerup", handlePreviewResizeEnd);
  window.addEventListener("pointercancel", handlePreviewResizeEnd);
  event.preventDefault();
}

function handlePreviewResizeMove(event) {
  const session = state.resizing;
  if (!session || !state.draft) {
    return;
  }

  const deltaCols = Math.round(
    (event.clientX - session.startX) / session.stepX,
  );
  const deltaRows = Math.round(
    (event.clientY - session.startY) / session.stepY,
  );

  let nextWidth = session.startWidth;
  let nextHeight = session.startHeight;

  if (session.handle.includes("e")) {
    nextWidth = clamp(session.startWidth + deltaCols, 1, 6);
  }

  if (session.handle.includes("s")) {
    nextHeight = clamp(session.startHeight + deltaRows, 1, 6);
  }

  if (
    nextWidth === state.draft.initial_width &&
    nextHeight === state.draft.initial_height
  ) {
    return;
  }

  applyDraftSize(nextWidth, nextHeight);
}

function handlePreviewResizeEnd() {
  const session = state.resizing;
  if (!session) {
    return;
  }

  state.resizing = null;
  elements.previewCard.classList.remove("is-resizing");
  document.body.classList.remove("ai-builder-is-resizing");
  window.removeEventListener("pointermove", handlePreviewResizeMove);
  window.removeEventListener("pointerup", handlePreviewResizeEnd);
  window.removeEventListener("pointercancel", handlePreviewResizeEnd);

  if (
    !state.draft ||
    (state.draft.initial_width === session.persistedWidth &&
      state.draft.initial_height === session.persistedHeight)
  ) {
    return;
  }

  void persistDraftSize(state.draft.initial_width, state.draft.initial_height, {
    width: session.persistedWidth,
    height: session.persistedHeight,
  });
}

function applyDraftSize(width, height) {
  if (!state.draft) {
    return;
  }

  state.draft.initial_width = width;
  state.draft.initial_height = height;
  renderDraft();
}

async function persistDraftSize(width, height, previous) {
  if (!state.draft) {
    return;
  }

  state.sizeSaving = true;
  renderPreviewMode();

  try {
    const response = await requestJson(
      apiPath(`/ai/drafts/${state.draft.id}/size`),
      {
        method: "POST",
        body: JSON.stringify({
          initial_width: width,
          initial_height: height,
        }),
      },
    );

    state.draft = response.draft;
    clearError();
    renderDraft();
  } catch (error) {
    state.draft.initial_width = previous.width;
    state.draft.initial_height = previous.height;
    renderDraft();
    showError(error.message);
  } finally {
    state.sizeSaving = false;
    renderPreviewMode();
  }
}

function positionPreviewCard(width, height) {
  const cols = clamp(width, 1, 6);
  const rows = clamp(height, 1, 6);
  const startCol = Math.max(1, Math.floor((6 - cols) / 2) + 1);
  const startRow = Math.max(1, Math.floor((6 - rows) / 2) + 1);

  elements.previewCard.style.gridColumn = `${startCol} / span ${cols}`;
  elements.previewCard.style.gridRow = `${startRow} / span ${rows}`;
  elements.previewStage.style.setProperty("--widget-cols", cols);
  elements.previewStage.style.setProperty("--widget-rows", rows);
}

function updateSizeControls(width, height) {
  elements.widthValue.textContent = String(width);
  elements.heightValue.textContent = String(height);
}

function setPreviewFrameContent(html) {
  if (elements.previewFrame.dataset.srcdoc === html) {
    return;
  }

  elements.previewFrame.dataset.srcdoc = html;
  elements.previewFrame.srcdoc = html;
}

function buildConfigPreview(draft) {
  return JSON.stringify(
    {
      title: draft.title,
      author: draft.author,
      version: draft.version,
      description: draft.description,
      details: draft.details,
      initial_width: draft.initial_width,
      initial_height: draft.initial_height,
      permissions: Array.isArray(draft.permissions) ? draft.permissions : [],
    },
    null,
    2,
  );
}

function getSelectedModel() {
  return findModel(state.selectedModel) ?? findModel(state.status?.model);
}

function findModel(modelId) {
  if (!modelId) {
    return null;
  }

  return state.status?.models?.find((model) => model.id === modelId) ?? null;
}

function setLoading(isLoading) {
  state.loading = isLoading;
  elements.generateButton.disabled = isLoading;
  elements.resetDraftButton.disabled = isLoading;
  elements.generateButton.textContent = isLoading
    ? "Gerando..."
    : state.draft
      ? "Aplicar alteracoes"
      : "Gerar widget";
  renderModelList();
  renderPreviewMode();
}

function showError(message) {
  elements.errorBanner.hidden = false;
  elements.errorBanner.textContent = message;
}

function clearError() {
  elements.errorBanner.hidden = true;
  elements.errorBanner.textContent = "";
}

function renderKeyFeedback(message, isError) {
  elements.keyFeedback.textContent = message;
  elements.keyFeedback.classList.toggle("is-error", Boolean(isError));
}

async function requestJson(url, options = {}) {
  const init = {
    ...options,
    headers: {
      ...(options.body ? { "content-type": "application/json" } : {}),
      ...(options.headers ?? {}),
    },
  };

  const response = await fetch(url, init);
  const text = await response.text();
  const payload = text ? safeJsonParse(text) : {};

  if (!response.ok) {
    const message =
      payload?.error ||
      payload?.message ||
      `Request failed (${response.status})`;
    throw new Error(message);
  }

  return payload;
}

function safeJsonParse(value) {
  try {
    return JSON.parse(value);
  } catch {
    return {};
  }
}

function formatUsageValue(value) {
  if (typeof value !== "number" || Number.isNaN(value)) {
    return "n/d";
  }

  return Intl.NumberFormat("pt-BR").format(value);
}

function clamp(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function escapeToml(value) {
  return String(value).replaceAll("\\", "\\\\").replaceAll('"', '\\"');
}
