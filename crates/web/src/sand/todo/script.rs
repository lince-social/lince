pub(super) fn script() -> String {
    r##"
(() => {
  const frame = window.frameElement;
  const appRoot = document.getElementById("app");
  const statusDot = document.getElementById("todo-status");
  const detailsPanel = document.getElementById("todo-details");
  const listPanel = document.getElementById("todo-list-panel");
  const detailSource = document.getElementById("todo-detail-source");
  const detailActive = document.getElementById("todo-detail-active");
  const detailPreview = document.getElementById("todo-detail-preview");
  const detailEndpoint = document.getElementById("todo-detail-endpoint");
  const detailCount = document.getElementById("todo-detail-count");
  const detailSourceCount = document.getElementById("todo-detail-source-count");
  const serverId = String(frame?.dataset?.linceServerId || "").trim();
  const viewId = Number(String(frame?.dataset?.linceViewId || "").trim());

  const state = {
    controller: null,
    reconnectTimer: null,
    reconnectAttempt: 0,
    streamGeneration: 0,
    streamUrl: "",
    items: [],
    activeIndex: -1,
    snapshot: null,
    detailsOpen: false,
    chromeTimer: null,
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
      state.chromeTimer = window.setTimeout(() => {
        delete appRoot.dataset.pointerActive;
        state.chromeTimer = null;
      }, 900);
      return;
    }

    delete appRoot.dataset.pointerActive;
    if (state.chromeTimer) {
      window.clearTimeout(state.chromeTimer);
      state.chromeTimer = null;
    }
  }

  function setDetailsOpen(open) {
    state.detailsOpen = Boolean(open);
    if (detailsPanel) {
      detailsPanel.hidden = !state.detailsOpen;
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
    const id = readString(
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
    const body = readString(
      object.body,
      object.description,
      object.subtitle,
      object.note,
      object.details,
    );
    const quantity = readNumber(object.quantity, object.amount, object.count);
    const active = Boolean(
      object.active ?? object.is_active ?? object.focused ?? object.selected,
    );

    return {
      id,
      title: title || `Item ${index + 1}`,
      body,
      quantity,
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
      if (active) {
        const bits = [];
        if (active.body) {
          bits.push(active.body);
        }
        if (active.quantity != null) {
          bits.push(`quantity ${active.quantity}`);
        }
        detailPreview.textContent = bits.length ? bits.join(" · ") : "Selected item is active.";
      } else {
        detailPreview.textContent = "Open the stream to see the current item.";
      }
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

      if (index === activeIndex) {
        button.dataset.active = "true";
      }

      const main = document.createElement("span");
      main.className = "todoItemMain";

      const title = document.createElement("span");
      title.className = "todoItemTitle";
      title.textContent = item.title;
      main.appendChild(title);

      if (item.body) {
        const body = document.createElement("span");
        body.className = "todoItemBody";
        body.textContent = item.body;
        main.appendChild(body);
      }

      button.appendChild(main);

      if (item.quantity != null) {
        const meta = document.createElement("span");
        meta.className = "todoItemMeta";
        meta.textContent = String(item.quantity);
        button.appendChild(meta);
      }

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
    statusDot.addEventListener("click", () => {
      setDetailsOpen(!state.detailsOpen);
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
      setChromeVisible(true);
    });
    appRoot.addEventListener("pointerleave", () => {
      setChromeVisible(false);
    });
  }

  state.streamUrl = buildStreamUrl();
  setDetailsOpen(false);
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
