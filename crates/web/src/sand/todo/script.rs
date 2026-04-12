pub(super) fn script() -> String {
    r##"
(() => {
  const frame = window.frameElement;
  const appRoot = document.getElementById("app");
  const statusDot = document.getElementById("todo-status");
  const detailsPanel = document.getElementById("todo-details");
  const listPanel = document.getElementById("todo-list-panel");
  const blobLayer = document.getElementById("todo-blob-layer");
  const blobEnabledInput = document.getElementById("blob-enabled");
  const blobViscosityInput = document.getElementById("blob-viscosity");
  const blobEnergyInput = document.getElementById("blob-energy");
  const blobColorInput = document.getElementById("blob-color-input");
  const blobAddColorButton = document.getElementById("blob-add-color");
  const blobPalette = document.getElementById("blob-palette");
  const detailSource = document.getElementById("todo-detail-source");
  const detailActive = document.getElementById("todo-detail-active");
  const detailPreview = document.getElementById("todo-detail-preview");
  const detailEndpoint = document.getElementById("todo-detail-endpoint");
  const detailCount = document.getElementById("todo-detail-count");
  const detailSourceCount = document.getElementById("todo-detail-source-count");
  const serverId = String(frame?.dataset?.linceServerId || "").trim();
  const viewId = Number(String(frame?.dataset?.linceViewId || "").trim());
  const instanceId = String(frame?.dataset?.packageInstanceId || "preview").trim() || "preview";
  const blobSettingsKey = "todo-blob/" + instanceId;
  const defaultBlobColors = ["#51f3d2", "#7cc7ff", "#f5d36a"];
  const MAX_HISTORY = 100;

  const state = {
    controller: null,
    reconnectTimer: null,
    reconnectAttempt: 0,
    streamGeneration: 0,
    streamUrl: "",
    items: [],
    activeIndex: -1,
    snapshot: null,
    quantityHistory: [],
    quantityHistoryCursor: -1,
    detailsOpen: false,
    chromeTimer: null,
    chromeHovered: false,
    blobSettings: {
      enabled: false,
      viscosity: 0.62,
      energy: 0.56,
      colors: defaultBlobColors.slice(),
    },
    blob: {
      adapter: null,
      device: null,
      context: null,
      canvas: null,
      pipeline: null,
      bindGroup: null,
      uniformBuffer: null,
      uniformData: new Float32Array(24),
      ready: false,
      setupPending: false,
      frameId: 0,
      visible: 0,
      current: { x: 0, y: 0 },
      target: { x: 0, y: 0 },
      phase: "idle",
      phaseStarted: 0,
      origin: null,
      width: 0,
      height: 0,
      dpr: 1,
    },
  };

  function clamp(value, min, max) {
    return Math.min(max, Math.max(min, value));
  }

  function setStatus(text, tone = "idle") {
    if (!statusDot) {
      return;
    }

    statusDot.dataset.tone = tone;
    statusDot.setAttribute("aria-label", text);
    statusDot.title = text;
    statusDot.textContent = "";
  }

  function setChromeVisible(visible) {
    if (!appRoot) {
      return;
    }

    if (visible) {
      appRoot.dataset.pointerActive = "true";
      if (state.chromeTimer) {
        window.clearTimeout(state.chromeTimer);
      }
      if (!state.chromeHovered) {
        scheduleChromeHide();
      }
      return;
    }

    delete appRoot.dataset.pointerActive;
    if (state.chromeTimer) {
      window.clearTimeout(state.chromeTimer);
      state.chromeTimer = null;
    }
  }

  function scheduleChromeHide() {
    if (!appRoot) {
      return;
    }

    if (state.chromeTimer) {
      window.clearTimeout(state.chromeTimer);
    }

    if (state.chromeHovered) {
      appRoot.dataset.pointerActive = "true";
      state.chromeTimer = null;
      return;
    }

    state.chromeTimer = window.setTimeout(() => {
      if (state.chromeHovered) {
        return;
      }
      delete appRoot.dataset.pointerActive;
      state.chromeTimer = null;
    }, 900);
  }

  function setDetailsOpen(open) {
    state.detailsOpen = Boolean(open);
    if (detailsPanel) {
      detailsPanel.hidden = !state.detailsOpen;
    }
  }

  function lerp(from, to, amount) {
    return from + (to - from) * amount;
  }

  function easeOutCubic(value) {
    const next = clamp(value, 0, 1) - 1;
    return next * next * next + 1;
  }

  function normalizeHexColor(value) {
    const match = String(value || "").trim().match(/^#?([0-9a-f]{6})$/i);
    return match ? "#" + match[1].toLowerCase() : null;
  }

  function hexToRgb(color) {
    const normalized = normalizeHexColor(color) || defaultBlobColors[0];
    const value = Number.parseInt(normalized.slice(1), 16);
    return [
      ((value >> 16) & 255) / 255,
      ((value >> 8) & 255) / 255,
      (value & 255) / 255,
    ];
  }

  function readBlobSettings() {
    try {
      const raw = window.localStorage?.getItem?.(blobSettingsKey);
      if (!raw) {
        return null;
      }

      const parsed = JSON.parse(raw);
      if (!parsed || typeof parsed !== "object") {
        return null;
      }

      const colors = Array.isArray(parsed.colors)
        ? parsed.colors.map(normalizeHexColor).filter(Boolean).slice(0, 8)
        : [];

      return {
        enabled: parsed.enabled === true,
        viscosity: clamp(Number(parsed.viscosity ?? 0.62), 0, 1),
        energy: clamp(Number(parsed.energy ?? 0.56), 0, 1),
        colors: colors.length ? colors : defaultBlobColors.slice(),
      };
    } catch {
      return null;
    }
  }

  function writeBlobSettings() {
    try {
      window.localStorage?.setItem?.(
        blobSettingsKey,
        JSON.stringify({
          enabled: state.blobSettings.enabled,
          viscosity: state.blobSettings.viscosity,
          energy: state.blobSettings.energy,
          colors: state.blobSettings.colors,
        }),
      );
    } catch {
      // ignore storage failures
    }
  }

  function syncBlobControls() {
    if (blobEnabledInput instanceof HTMLInputElement) {
      blobEnabledInput.checked = state.blobSettings.enabled;
    }

    if (blobViscosityInput instanceof HTMLInputElement) {
      blobViscosityInput.value = String(Math.round(state.blobSettings.viscosity * 100));
    }

    if (blobEnergyInput instanceof HTMLInputElement) {
      blobEnergyInput.value = String(Math.round(state.blobSettings.energy * 100));
    }

    if (blobColorInput instanceof HTMLInputElement) {
      blobColorInput.value = state.blobSettings.colors[0] || defaultBlobColors[0];
    }
  }

  function renderBlobPalette() {
    if (!blobPalette) {
      return;
    }

    blobPalette.replaceChildren();
    for (const color of state.blobSettings.colors) {
      const swatch = document.createElement("button");
      swatch.type = "button";
      swatch.className = "paletteSwatch";
      swatch.dataset.color = color;
      swatch.style.background = color;
      swatch.title = "Remove " + color;
      swatch.setAttribute("aria-label", "Remove blob color " + color);
      blobPalette.appendChild(swatch);
    }
  }

  function buildStreamUrl() {
    if (!serverId) {
      return "";
    }

    if (!Number.isInteger(viewId) || viewId <= 0) {
      return "";
    }

    return (
      "/host/integrations/servers/" +
      encodeURIComponent(serverId) +
      "/views/" +
      encodeURIComponent(viewId) +
      "/stream"
    );
  }

  function parseEventBlock(block) {
    const lines = block.split("\n");
    let eventName = "message";
    const dataLines = [];

    for (const line of lines) {
      if (line.startsWith("event:")) {
        eventName = line.slice(6).trim();
      } else if (line.startsWith("data:")) {
        dataLines.push(line.slice(5).trimStart());
      }
    }

    return { event: eventName, data: dataLines.join("\n") };
  }

  function parseSsePayload(payload) {
    if (typeof payload !== "string") {
      return payload;
    }

    try {
      return JSON.parse(payload);
    } catch {
      return payload;
    }
  }

  function stopReconnectTimer() {
    if (state.reconnectTimer) {
      window.clearTimeout(state.reconnectTimer);
      state.reconnectTimer = null;
    }
  }

  function stopStream() {
    stopReconnectTimer();

    if (state.controller) {
      state.controller.abort();
      state.controller = null;
    }
  }

  function scheduleReconnect() {
    stopReconnectTimer();

    if (!state.streamUrl) {
      return;
    }

    const delay = Math.min(12000, 1200 * Math.max(1, state.reconnectAttempt + 1));
    state.reconnectAttempt += 1;
    state.reconnectTimer = window.setTimeout(() => connectStream(false), delay);
  }

  function asObject(value) {
    return value && typeof value === "object" && !Array.isArray(value) ? value : {};
  }

  function readString(...values) {
    for (const value of values) {
      if (value == null) {
        continue;
      }

      const text = String(value).trim();
      if (text) {
        return text;
      }
    }

    return "";
  }

  function readNumber(...values) {
    for (const value of values) {
      if (value == null || value === "") {
        continue;
      }

      const parsed = Number(value);
      if (Number.isFinite(parsed)) {
        return parsed;
      }
    }

    return null;
  }

  function normalizeRowsSource(snapshot) {
    if (Array.isArray(snapshot)) {
      return snapshot;
    }

    const object = asObject(snapshot);
    for (const key of ["rows", "items", "data", "list", "values"]) {
      if (Array.isArray(object[key])) {
        return object[key];
      }
    }

    return snapshot == null ? [] : [snapshot];
  }

  function normalizeItem(raw, index) {
    const object = asObject(raw);
    const recordId = readString(
      object.record_id,
      object.recordId,
      object.todo_id,
      object.todoId,
      object.id,
      object.key,
      index,
    );
    const title = readString(
      object.head,
      object.title,
      object.name,
      object.label,
      object.text,
      object.value,
      typeof raw === "string" || typeof raw === "number" ? raw : "",
    );
    const quantity = readNumber(
      object.quantity,
      object.qty,
      object.amount,
      object.row_quantity,
      object.rowQuantity,
    );
    const body = readString(
      object.body,
      object.description,
      object.subtitle,
      object.note,
      object.details,
    );
    const active = Boolean(
      object.active ?? object.is_active ?? object.focused ?? object.selected,
    );

    return {
      id: recordId,
      recordId,
      title: title || `Item ${index + 1}`,
      quantity,
      body,
      active,
      raw,
    };
  }

  function itemIdentity(item) {
    if (!item) {
      return "";
    }

    return String(item.id || item.title || "");
  }

  function focusedItem() {
    if (state.activeIndex < 0 || state.activeIndex >= state.items.length) {
      return null;
    }

    return state.items[state.activeIndex] || null;
  }

  function focusedRecordId() {
    const item = focusedItem();
    if (!item) {
      return null;
    }

    const recordId = String(item.recordId || item.id || "").trim();
    return recordId || null;
  }

  function readItemQuantity(item) {
    if (!item) {
      return null;
    }

    const quantity = Number(item.quantity);
    return Number.isFinite(quantity) ? quantity : null;
  }

  function applyItemQuantity(item, quantity) {
    if (!item || !Number.isFinite(quantity)) {
      return;
    }

    item.quantity = quantity;

    if (item.raw && typeof item.raw === "object") {
      item.raw.quantity = quantity;
    }
  }

  function pruneQuantityHistoryTail() {
    if (state.quantityHistoryCursor >= state.quantityHistory.length - 1) {
      return;
    }

    state.quantityHistory.splice(state.quantityHistoryCursor + 1);
  }

  function pushQuantityHistory(entry) {
    pruneQuantityHistoryTail();
    state.quantityHistory.push(entry);

    if (state.quantityHistory.length > MAX_HISTORY) {
      const overflow = state.quantityHistory.length - MAX_HISTORY;
      state.quantityHistory.splice(0, overflow);
    }

    state.quantityHistoryCursor = state.quantityHistory.length - 1;
  }

  function applyQuantityHistoryStep(direction) {
    const entryIndex =
      direction < 0 ? state.quantityHistoryCursor : state.quantityHistoryCursor + 1;
    const entry = state.quantityHistory[entryIndex];
    if (!entry) {
      return false;
    }

    const quantity = direction < 0 ? entry.from : entry.to;
    return patchRecordQuantity(entry.recordId, quantity, {
      action: direction < 0 ? "undo" : "redo",
      historyIndex: entryIndex,
      fromQuantity: direction < 0 ? entry.to : entry.from,
    });
  }

  async function zeroFocusedQuantity() {
    const recordId = focusedRecordId();
    if (!recordId) {
      return false;
    }

    const item = focusedItem();
    const fromQuantity = readItemQuantity(item);
    const updated = await patchRecordQuantity(recordId, 0, {
      action: "record",
      fromQuantity,
    });

    if (updated && state.items.length === 1) {
      startCompletionBlob(blobPointForRow(activeBlobRow()));
    }

    return updated;
  }

  async function patchRecordQuantity(recordId, quantity, options = {}) {
    const recordKey = String(recordId || "").trim();
    if (!recordKey || !Number.isFinite(quantity)) {
      return false;
    }

    const fromQuantity = Number.isFinite(options.fromQuantity)
      ? Number(options.fromQuantity)
      : readItemQuantity(state.items.find((item) => String(item.recordId || item.id || "") === recordKey));

    try {
      const response = await fetch(
        "/host/integrations/servers/" +
          encodeURIComponent(serverId) +
          "/table/record/" +
          encodeURIComponent(recordKey),
        {
          method: "PATCH",
          headers: {
            "Content-Type": "application/json",
          },
          body: JSON.stringify({ quantity }),
        },
      );

      if (response.status === 401) {
        window.LinceWidgetHost?.invalidateServerAuth?.(serverId);
        setStatus("Bloqueado", "error");
        return false;
      }

      if (!response.ok) {
        const raw = await response.text().catch(() => "");
        throw new Error(raw || "Nao foi possivel atualizar o record.");
      }

      const item = state.items.find((entry) => String(entry.recordId || entry.id || "") === recordKey);
      applyItemQuantity(item, quantity);

      if (options.action === "record" && Number.isFinite(fromQuantity) && fromQuantity !== quantity) {
        pushQuantityHistory({
          recordId: recordKey,
          from: fromQuantity,
          to: quantity,
        });
      } else if (options.action === "undo") {
        state.quantityHistoryCursor = Math.max(-1, options.historyIndex - 1);
      } else if (options.action === "redo") {
        state.quantityHistoryCursor = Math.min(
          state.quantityHistory.length - 1,
          options.historyIndex,
        );
      }

      renderItems(state.items, state.activeIndex);
      updateDetails(state.snapshot, state.items, state.activeIndex);
      return true;
    } catch (error) {
      if (error instanceof Error) {
        console.error(error);
      }

      return false;
    }
  }

  function resolveActiveIndex(snapshot, items) {
    if (!items.length) {
      return -1;
    }

    const object = asObject(snapshot);
    const indexCandidates = [
      object.active_index,
      object.activeIndex,
      object.focused_index,
      object.focusedIndex,
      object.selected_index,
      object.selectedIndex,
    ];

    for (const candidate of indexCandidates) {
      if (candidate == null || candidate === "") {
        continue;
      }

      const parsed = Number(candidate);
      if (Number.isInteger(parsed) && parsed >= 0 && parsed < items.length) {
        return parsed;
      }
    }

    const identityCandidates = [
      object.active_record_id,
      object.activeRecordId,
      object.active_id,
      object.activeId,
      object.focused_record_id,
      object.focusedRecordId,
      object.focused_id,
      object.focusedId,
      object.selected_record_id,
      object.selectedRecordId,
      object.selected_id,
      object.selectedId,
    ];

    for (const candidate of identityCandidates) {
      if (candidate == null || candidate === "") {
        continue;
      }

      const match = items.findIndex((item) => itemIdentity(item) === String(candidate));
      if (match >= 0) {
        return match;
      }
    }

    const activeItem =
      object.active_item ||
      object.activeItem ||
      object.focused_item ||
      object.focusedItem ||
      object.selected_item ||
      object.selectedItem;

    if (activeItem) {
      const activeId = itemIdentity(normalizeItem(activeItem, 0));
      const match = items.findIndex((item) => itemIdentity(item) === activeId);
      if (match >= 0) {
        return match;
      }
    }

    const explicitItem = items.findIndex((item) => item.active);
    if (explicitItem >= 0) {
      return explicitItem;
    }

    if (state.activeIndex >= 0 && state.activeIndex < items.length) {
      return state.activeIndex;
    }

    return 0;
  }

  function activeItem(items, activeIndex) {
    if (!Array.isArray(items) || activeIndex < 0 || activeIndex >= items.length) {
      return null;
    }

    return items[activeIndex] || null;
  }

  function updateDetails(snapshot, items, activeIndex) {
    const active = activeItem(items, activeIndex);
    const endpoint = state.streamUrl || "Waiting for connection.";

    if (detailSource) {
      detailSource.textContent = serverId && viewId
        ? `server ${serverId} · view ${viewId}`
        : "Waiting for connection";
    }

    if (detailActive) {
      detailActive.textContent = active ? active.title : "No active item";
    }

    if (detailPreview) {
      detailPreview.textContent = active
        ? active.title
        : "Open the stream to see the current item.";
    }

    if (detailEndpoint) {
      detailEndpoint.textContent = endpoint;
    }

    if (detailCount) {
      detailCount.textContent = `items: ${items.length}`;
    }

    if (detailSourceCount) {
      detailSourceCount.textContent = `active: ${active ? activeIndex + 1 : 0}`;
    }
  }

  function scrollActiveIntoView() {
    if (!listPanel) {
      return;
    }

    const active = listPanel.querySelector('[data-active="true"]');
    if (active && typeof active.scrollIntoView === "function") {
      active.scrollIntoView({ block: "nearest" });
    }
  }

  function renderEmptyState(message) {
    if (!listPanel) {
      return;
    }

    const frame = document.createElement("div");
    frame.className = "listFrame";

    const empty = document.createElement("div");
    empty.className = "emptyState";

    const title = document.createElement("div");
    title.className = "stateTitle";
    title.textContent = "Waiting for items";

    const copy = document.createElement("div");
    copy.className = "stateCopy";
    copy.textContent =
      message ||
      "The normal view SSE stream will populate this list and keep one item active.";

    empty.append(title, copy);
    frame.replaceChildren(empty);
    listPanel.replaceChildren(frame);
  }

  function renderItems(items, activeIndex) {
    if (!listPanel) {
      return;
    }

    if (!items.length) {
      renderEmptyState();
      scrollActiveIntoView();
      return;
    }

    const frame = document.createElement("div");
    frame.className = "listFrame";

    const list = document.createElement("div");
    list.className = "todoList";

    items.forEach((item, index) => {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "todoItem";
      button.dataset.index = String(index);
      button.dataset.itemId = item.id;
      button.dataset.recordId = item.recordId;

      if (Number.isFinite(item.quantity)) {
        button.dataset.rowQuantity = String(item.quantity);
      }

      if (index === activeIndex) {
        button.dataset.active = "true";
      }

      const main = document.createElement("span");
      main.className = "todoItemMain";

      const title = document.createElement("span");
      title.className = "todoItemTitle";
      title.textContent = item.title;
      main.appendChild(title);

      button.appendChild(main);

      list.appendChild(button);
    });

    frame.appendChild(list);
    listPanel.replaceChildren(frame);
    scrollActiveIntoView();
  }

  function renderSnapshot(snapshot) {
    const rows = normalizeRowsSource(snapshot);
    const items = rows.map(normalizeItem);
    const activeIndex = resolveActiveIndex(snapshot, items);

    state.snapshot = snapshot;
    state.items = items;
    state.activeIndex = activeIndex;

    renderItems(items, activeIndex);
    updateDetails(snapshot, items, activeIndex);
  }

  function setActiveIndex(index) {
    if (!state.items.length) {
      return;
    }

    const nextIndex = clamp(index, 0, state.items.length - 1);
    if (nextIndex === state.activeIndex) {
      return;
    }

    state.activeIndex = nextIndex;
    renderItems(state.items, state.activeIndex);
    updateDetails(state.snapshot, state.items, state.activeIndex);
  }

  function moveActive(delta) {
    if (!state.items.length) {
      return;
    }

    setActiveIndex((state.activeIndex < 0 ? 0 : state.activeIndex) + delta);
  }

  function handleListKeydown(event) {
    if (event.metaKey || event.ctrlKey || event.altKey) {
      return;
    }

    const key = event.key;
    if (key === "j" || key === "ArrowDown") {
      event.preventDefault();
      moveActive(1);
      return;
    }

    if (key === "k" || key === "ArrowUp") {
      event.preventDefault();
      moveActive(-1);
      return;
    }

    if (key === "u" && !event.shiftKey) {
      event.preventDefault();
      event.stopPropagation();
      void applyQuantityHistoryStep(-1);
      return;
    }

    if (key === "U" || (key === "u" && event.shiftKey)) {
      event.preventDefault();
      event.stopPropagation();
      void applyQuantityHistoryStep(1);
      return;
    }

    const isSpace =
      key === " " ||
      key === "Spacebar" ||
      key === "Space" ||
      event.code === "Space";

    if (isSpace) {
      event.preventDefault();
      event.stopPropagation();
      void zeroFocusedQuantity();
      return;
    }

    if (key === "Home") {
      event.preventDefault();
      setActiveIndex(0);
      return;
    }

    if (key === "End") {
      event.preventDefault();
      setActiveIndex(state.items.length - 1);
    }
  }

  function activeBlobRow() {
    if (!listPanel) {
      return null;
    }

    return listPanel.querySelector('[data-active="true"]');
  }

  function syncBlobState() {
    if (!blobLayer) {
      return;
    }

    if (!state.blobSettings.enabled) {
      blobLayer.hidden = true;
      if (state.blob.frameId) {
        window.cancelAnimationFrame(state.blob.frameId);
        state.blob.frameId = 0;
      }
      state.blob.phase = "idle";
      state.blob.visible = 0;
      state.blob.origin = null;
      if (state.blob.device && state.blob.canvas) {
        state.blob.device.queue.writeBuffer(
          state.blob.uniformBuffer,
          0,
          new Float32Array(state.blob.uniformData.length),
        );
      }
      return;
    }

    blobLayer.hidden = false;
    if (!navigator.gpu) {
      blobLayer.hidden = true;
      setStatus("WebGPU required", "error");
      return;
    }

    if (state.blob.ready) {
      if (!state.blob.frameId) {
        state.blob.frameId = window.requestAnimationFrame(renderBlobFrame);
      }
      return;
    }

    void setupBlobLayer();
  }

  function resizeBlobCanvas() {
    if (!blobLayer || !state.blob.canvas) {
      return { width: 0, height: 0 };
    }

    const rect = blobLayer.getBoundingClientRect();
    const cssWidth = Math.max(1, rect.width);
    const cssHeight = Math.max(1, rect.height);
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const pixelWidth = Math.max(1, Math.round(cssWidth * dpr));
    const pixelHeight = Math.max(1, Math.round(cssHeight * dpr));

    state.blob.canvas.style.width = cssWidth + "px";
    state.blob.canvas.style.height = cssHeight + "px";

    if (state.blob.canvas.width !== pixelWidth || state.blob.canvas.height !== pixelHeight) {
      state.blob.canvas.width = pixelWidth;
      state.blob.canvas.height = pixelHeight;
    }

    state.blob.width = cssWidth;
    state.blob.height = cssHeight;
    state.blob.dpr = dpr;
    return { width: cssWidth, height: cssHeight };
  }

  function blobPointForRow(row) {
    if (!blobLayer || !row) {
      return null;
    }

    const cellRect = row.getBoundingClientRect();
    const layerRect = blobLayer.getBoundingClientRect();

    if (cellRect.height <= 0 || cellRect.width <= 0) {
      return null;
    }

    return {
      x: cellRect.left - layerRect.left + 2,
      y: cellRect.top - layerRect.top + cellRect.height / 2,
    };
  }

  function startCompletionBlob(origin) {
    if (!blobLayer || !state.blobSettings.enabled || !origin) {
      return;
    }

    state.blob.phase = "completion";
    state.blob.phaseStarted = performance.now();
    state.blob.origin = { x: origin.x, y: origin.y };
    state.blob.current = { x: origin.x, y: origin.y };
    state.blob.target = { x: origin.x, y: origin.y };
    state.blob.visible = 1;
    void setupBlobLayer();
  }

  function updateCursorBlobTarget() {
    if (!state.blobSettings.enabled) {
      if (state.blob.phase === "cursor") {
        state.blob.phase = "idle";
      }
      return false;
    }

    const target = blobPointForRow(activeBlobRow());
    if (!target) {
      if (state.blob.phase === "cursor") {
        state.blob.phase = "idle";
      }
      return false;
    }

    if (state.blob.phase !== "cursor" || state.blob.visible < 0.05) {
      state.blob.current = { x: target.x, y: target.y };
    }

    state.blob.phase = "cursor";
    state.blob.target = target;
    return true;
  }

  function colorsForUniform() {
    const colors = state.blobSettings.colors.length
      ? state.blobSettings.colors
      : defaultBlobColors;

    return [
      hexToRgb(colors[0] || defaultBlobColors[0]),
      hexToRgb(colors[1] || colors[0] || defaultBlobColors[1]),
      hexToRgb(colors[2] || colors[1] || colors[0] || defaultBlobColors[2]),
    ];
  }

  function writeBlobUniforms(now, width, height, completion) {
    const data = state.blob.uniformData;
    const colors = colorsForUniform();

    data[0] = width;
    data[1] = height;
    data[2] = now / 1000;
    data[3] = state.blob.visible;
    data[4] = state.blob.current.x;
    data[5] = state.blob.current.y;
    data[6] = state.blob.target.x;
    data[7] = state.blob.target.y;
    data[8] = state.blobSettings.viscosity;
    data[9] = state.blobSettings.energy;
    data[10] = state.blob.phase === "completion" ? 1 : 0;
    data[11] = completion;

    for (let index = 0; index < 3; index += 1) {
      const base = 12 + index * 4;
      data[base] = colors[index][0];
      data[base + 1] = colors[index][1];
      data[base + 2] = colors[index][2];
      data[base + 3] = 1;
    }

    state.blob.device.queue.writeBuffer(state.blob.uniformBuffer, 0, data);
  }

  function renderBlobFrame(now) {
    if (!state.blob.ready || !state.blobSettings.enabled) {
      return;
    }

    const { width, height } = resizeBlobCanvas();
    let active = false;
    let completion = 0;
    let targetVisible = 0;

    if (state.blob.phase === "completion") {
      const age = now - state.blob.phaseStarted;
      const origin = state.blob.origin || state.blob.current;
      const center = { x: width / 2, y: height / 2 };
      active = true;

      if (age <= 2600) {
        completion = easeOutCubic(age / 2600);
        state.blob.target = {
          x: lerp(origin.x, center.x, completion),
          y: lerp(origin.y, center.y, completion),
        };
        targetVisible = 1;
      } else if (age <= 4300) {
        completion = 1;
        state.blob.target = center;
        targetVisible = 1 - clamp((age - 2600) / 1700, 0, 1);
      } else {
        state.blob.phase = "idle";
        state.blob.origin = null;
        active = false;
      }
    } else {
      active = updateCursorBlobTarget();
      targetVisible = active && state.blobSettings.enabled ? 1 : 0;
    }

    state.blob.visible += (targetVisible - state.blob.visible) * (targetVisible > state.blob.visible ? 0.18 : 0.1);

    const follow = state.blob.phase === "completion"
      ? 0.12 + state.blobSettings.energy * 0.04
      : 0.06 + (1 - state.blobSettings.viscosity) * 0.22;
    state.blob.current.x += (state.blob.target.x - state.blob.current.x) * follow;
    state.blob.current.y += (state.blob.target.y - state.blob.current.y) * follow;

    writeBlobUniforms(now, width, height, completion);

    const encoder = state.blob.device.createCommandEncoder();
    const textureView = state.blob.context.getCurrentTexture().createView();
    const pass = encoder.beginRenderPass({
      colorAttachments: [
        {
          view: textureView,
          clearValue: { r: 0, g: 0, b: 0, a: 0 },
          loadOp: "clear",
          storeOp: "store",
        },
      ],
    });

    pass.setPipeline(state.blob.pipeline);
    pass.setBindGroup(0, state.blob.bindGroup);
    pass.draw(3);
    pass.end();
    state.blob.device.queue.submit([encoder.finish()]);

    state.blob.frameId = window.requestAnimationFrame(renderBlobFrame);
  }

  async function setupBlobLayer() {
    if (!blobLayer || state.blob.ready || state.blob.setupPending || !state.blobSettings.enabled) {
      return;
    }

    if (!navigator.gpu) {
      setStatus("WebGPU required", "error");
      return;
    }

    state.blob.setupPending = true;

    try {
      const adapter = await navigator.gpu.requestAdapter();
      if (!adapter) {
        return;
      }

      const device = await adapter.requestDevice();
      device.addEventListener?.("uncapturederror", (event) => {
        console.error("Todo WebGPU error", event.error);
      });
      device.lost.then((info) => {
        console.error("Todo WebGPU device lost", info);
      });

      const canvas = document.createElement("canvas");
      const context = canvas.getContext("webgpu");
      if (!context) {
        return;
      }

      canvas.setAttribute("aria-hidden", "true");
      blobLayer.replaceChildren(canvas);
      state.blob.canvas = canvas;
      resizeBlobCanvas();

      const format = navigator.gpu.getPreferredCanvasFormat();
      context.configure({
        device,
        format,
        alphaMode: "premultiplied",
        usage: GPUTextureUsage.RENDER_ATTACHMENT,
      });

      const shaderResponse = await fetch("blob.wgsl", { cache: "no-store" });
      if (!shaderResponse.ok) {
        throw new Error("Unable to load blob.wgsl.");
      }

      const shaderSource = await shaderResponse.text();
      const shaderModule = device.createShaderModule({ code: shaderSource });
      if (typeof shaderModule.getCompilationInfo === "function") {
        const compilationInfo = await shaderModule.getCompilationInfo();
        const errors = compilationInfo.messages.filter((message) => message.type === "error");
        if (errors.length) {
          throw new Error(
            errors
              .map((message) => {
                return (
                  "WGSL " +
                  message.lineNum +
                  ":" +
                  message.linePos +
                  " " +
                  message.message
                );
              })
              .join("\n"),
          );
        }
      }

      const uniformBuffer = device.createBuffer({
        size: state.blob.uniformData.byteLength,
        usage: GPUBufferUsage.UNIFORM | GPUBufferUsage.COPY_DST,
      });

      const bindGroupLayout = device.createBindGroupLayout({
        entries: [
          {
            binding: 0,
            visibility: GPUShaderStage.FRAGMENT,
            buffer: { type: "uniform" },
          },
        ],
      });

      const pipelineDescriptor = {
        layout: device.createPipelineLayout({ bindGroupLayouts: [bindGroupLayout] }),
        vertex: {
          module: shaderModule,
          entryPoint: "vs_main",
        },
        fragment: {
          module: shaderModule,
          entryPoint: "fs_main",
          targets: [
            {
              format,
              blend: {
                color: {
                  srcFactor: "one",
                  dstFactor: "one-minus-src-alpha",
                  operation: "add",
                },
                alpha: {
                  srcFactor: "one",
                  dstFactor: "one-minus-src-alpha",
                  operation: "add",
                },
              },
            },
          ],
        },
        primitive: {
          topology: "triangle-list",
        },
      };

      let pipeline;
      if (typeof device.createRenderPipelineAsync === "function") {
        pipeline = await device.createRenderPipelineAsync(pipelineDescriptor);
      } else {
        device.pushErrorScope("validation");
        pipeline = device.createRenderPipeline(pipelineDescriptor);
        const validationError = await device.popErrorScope();
        if (validationError) {
          throw validationError;
        }
      }

      const bindGroup = device.createBindGroup({
        layout: bindGroupLayout,
        entries: [
          {
            binding: 0,
            resource: { buffer: uniformBuffer },
          },
        ],
      });

      state.blob.adapter = adapter;
      state.blob.device = device;
      state.blob.context = context;
      state.blob.pipeline = pipeline;
      state.blob.bindGroup = bindGroup;
      state.blob.uniformBuffer = uniformBuffer;
      state.blob.ready = true;
      blobLayer.hidden = false;

      if (!state.blob.frameId) {
        state.blob.frameId = window.requestAnimationFrame(renderBlobFrame);
      }
    } catch (error) {
      if (error instanceof Error) {
        console.error(error);
      }
      setStatus("WebGPU required", "error");
    } finally {
      state.blob.setupPending = false;
    }
  }

  async function connectStream(reset) {
    stopStream();

    if (!state.streamUrl) {
      setStatus("Configurar", "idle");
      renderEmptyState("The widget needs a server and view id.");
      updateDetails(null, [], -1);
      return;
    }

    if (reset) {
      setStatus("Connecting", "loading");
    }

    const generation = ++state.streamGeneration;
    const controller = new AbortController();
    state.controller = controller;

    try {
      const response = await fetch(state.streamUrl, {
        headers: {
          Accept: "text/event-stream",
        },
        cache: "no-store",
        signal: controller.signal,
      });

      if (controller.signal.aborted || generation !== state.streamGeneration) {
        return;
      }

      if (response.status === 401) {
        setStatus("Blocked", "error");
        return;
      }

      if (!response.ok || !response.body) {
        const raw = await response.text().catch(() => "");
        throw new Error(raw || `Nao foi possivel abrir o stream (${response.status}).`);
      }

      state.reconnectAttempt = 0;
      setStatus("Live", "live");

      const reader = response.body.getReader();
      const decoder = new TextDecoder();
      let buffer = "";

      while (true) {
        const { value, done } = await reader.read();
        if (done || controller.signal.aborted || generation !== state.streamGeneration) {
          break;
        }

        buffer += decoder.decode(value, { stream: true });
        const blocks = buffer.split("\n\n");
        buffer = blocks.pop() || "";

        for (const block of blocks) {
          const trimmed = block.trim();
          if (!trimmed) {
            continue;
          }

          const event = parseEventBlock(trimmed);
          if (!event.data) {
            continue;
          }

          if (event.event === "snapshot") {
            const payload = parseSsePayload(event.data);
            renderSnapshot(payload);
            setStatus("Live", "live");
            continue;
          }

          if (event.event === "error") {
            const message = parseSsePayload(event.data);
            const text = typeof message === "string" ? message : "Stream error";
            setStatus("Offline", "error");
            renderEmptyState(text);
            updateDetails(null, [], -1);
            return;
          }
        }
      }

      if (controller.signal.aborted || generation !== state.streamGeneration) {
        return;
      }

      setStatus("Reconnecting", "loading");
      scheduleReconnect();
    } catch (error) {
      if (controller.signal.aborted || generation !== state.streamGeneration) {
        return;
      }

      setStatus("Offline", "error");
      scheduleReconnect();

      if (error instanceof Error) {
        console.error(error);
      }
    } finally {
      if (state.controller === controller) {
        state.controller = null;
      }
    }
  }

  function reconnect() {
    state.streamUrl = buildStreamUrl();
    if (!state.streamUrl) {
      setStatus("Configurar", "idle");
      return;
    }

    state.reconnectAttempt = 0;
    connectStream(true);
  }

  window.TodoWidget = {
    reconnect,
  };

  if (statusDot) {
    statusDot.addEventListener("pointerenter", () => {
      state.chromeHovered = true;
      setChromeVisible(true);
    });
    statusDot.addEventListener("pointerleave", () => {
      state.chromeHovered = false;
      scheduleChromeHide();
    });
    statusDot.addEventListener("click", () => {
      setDetailsOpen(!state.detailsOpen);
    });
  }

  if (blobEnabledInput instanceof HTMLInputElement) {
    blobEnabledInput.addEventListener("change", () => {
      state.blobSettings.enabled = blobEnabledInput.checked;
      writeBlobSettings();
      syncBlobState();
    });
  }

  if (blobViscosityInput instanceof HTMLInputElement) {
    blobViscosityInput.addEventListener("input", () => {
      state.blobSettings.viscosity = clamp(Number(blobViscosityInput.value) / 100, 0, 1);
      writeBlobSettings();
    });
  }

  if (blobEnergyInput instanceof HTMLInputElement) {
    blobEnergyInput.addEventListener("input", () => {
      state.blobSettings.energy = clamp(Number(blobEnergyInput.value) / 100, 0, 1);
      writeBlobSettings();
    });
  }

  if (blobAddColorButton) {
    blobAddColorButton.addEventListener("click", () => {
      const color = normalizeHexColor(blobColorInput?.value);
      if (!color) {
        return;
      }

      const colors = state.blobSettings.colors.filter((item) => item !== color);
      colors.unshift(color);
      state.blobSettings.colors = colors.slice(0, 8);
      writeBlobSettings();
      syncBlobControls();
      renderBlobPalette();
    });
  }

  if (blobPalette) {
    blobPalette.addEventListener("click", (event) => {
      const target = event.target;
      if (!(target instanceof HTMLElement)) {
        return;
      }

      const swatch = target.closest("[data-color]");
      const color = swatch instanceof HTMLElement ? swatch.dataset.color : "";
      if (!color || state.blobSettings.colors.length <= 1) {
        return;
      }

      state.blobSettings.colors = state.blobSettings.colors.filter((item) => item !== color);
      writeBlobSettings();
      syncBlobControls();
      renderBlobPalette();
    });
  }

  if (listPanel) {
    listPanel.tabIndex = 0;
    listPanel.addEventListener("keydown", handleListKeydown);
    listPanel.addEventListener("pointerdown", () => {
      window.requestAnimationFrame(() => {
        listPanel.focus({ preventScroll: true });
      });
    });
  }

  if (appRoot) {
    appRoot.addEventListener("pointermove", () => {
      state.chromeHovered = false;
      setChromeVisible(true);
      scheduleChromeHide();
    });
    appRoot.addEventListener("pointerleave", () => {
      state.chromeHovered = false;
      setChromeVisible(false);
    });
  }

  state.streamUrl = buildStreamUrl();
  setDetailsOpen(false);
  const storedBlobSettings = readBlobSettings();
  if (storedBlobSettings) {
    state.blobSettings = storedBlobSettings;
  }

  syncBlobControls();
  renderBlobPalette();
  syncBlobState();

  if (!state.streamUrl) {
    setStatus("Configurar", "idle");
    renderEmptyState("The widget needs a server and view id.");
    updateDetails(null, [], -1);
    return;
  }

  updateDetails(null, [], -1);
  setStatus("Connecting", "loading");
  connectStream(false);
})();
"##.to_string()
}
