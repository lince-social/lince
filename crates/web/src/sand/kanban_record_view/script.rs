pub(super) fn script() -> String {
    let mut script = String::from(crate::sand::shared_markdown::JS_HELPERS);
    script.push_str(
        r##"            
            const frame = window.frameElement;
            const columns = [
                { key: "backlog", label: "Backlog", value: 0 },
                { key: "next", label: "Next", value: -1 },
                { key: "wip", label: "WIP", value: -2 },
                { key: "review", label: "Review", value: -3 },
                { key: "done", label: "Done", value: 1 },
            ];
            const DEFAULT_WIDTH = 260;
            const COLLAPSED_WIDTH = 64;
            const MIN_WIDTH = 80;
            const DEFAULT_BODY_MODE = "full";
            const UI_SCHEMA_VERSION = 2;
            const BODY_MODES = new Set(["head", "compact", "full"]);
            const app = document.getElementById("app");
            let datastarApi = null;
            const datastarReady = import("/static/vendored/datastar.js")
                .then((module) => {
                    datastarApi = module;
                    return module;
                })
                .catch(() => null);

            const elements = {
                toolbarState: document.getElementById("kanban-toolbar-state"),
                emptyOrError: document.getElementById("kanban-empty-or-error"),
                activeFilters: document.getElementById("kanban-active-filters"),
                columns: document.getElementById("kanban-columns"),
                viewSheet: document.getElementById("kanban-view-sheet"),
                viewSheetBody: document.getElementById("kanban-view-sheet-body"),
                sourceViewInput: null,
                sourceViewSuggestions: null,
                toggleUpdates: document.getElementById("kanban-toggle-updates"),
                openFilters: document.getElementById("kanban-open-filters"),
                openView: document.getElementById("kanban-open-view"),
                openCreate: document.getElementById("kanban-open-create"),
                reconnect: document.getElementById("kanban-reconnect"),
                toggleStream: document.getElementById("kanban-toggle-stream"),
                filterSheet: document.getElementById("kanban-filter-sheet"),
                filterSheetBody: document.getElementById("kanban-filter-sheet-body"),
                createSheet: document.getElementById("kanban-create-sheet"),
                createSheetBody: document.getElementById("kanban-create-sheet-body"),
                editSheet: document.getElementById("kanban-edit-sheet"),
                editSheetBody: document.getElementById("kanban-edit-sheet-body"),
                focusSheet: document.getElementById("kanban-focus-sheet"),
                focusCard: document.getElementById("kanban-focus-card"),
                focusActionPanel: document.getElementById("kanban-focus-action-panel"),
            };

            const state = {
                contract: null,
                hostMeta: normalizeHostMeta(null),
                hasHostState: false,
                ui: loadPreviewUi(),
                lastPersistedUiJson: "",
                loadingContract: false,
                loadingStream: false,
                connected: false,
                transportError: "",
                lastUpdate: "",
                reconnectAttempt: 0,
                reconnectTimer: null,
                streamController: null,
                streamGeneration: 0,
                persistTimer: null,
                dragRecordId: null,
                resize: null,
                viewMeta: null,
                activeSheet: "",
                formOptions: null,
                formOptionsPromise: null,
                formOptionsRequestId: 0,
                sourceViewQuery: "",
                focusDetail: null,
                focusPreview: null,
                focusDetailGeneration: 0,
                focusAction: null,
                pendingWorklogStops: [],
                activeWorklogIntervals: [],
                heartbeatTimer: null,
                draftFilters: emptyFilterState(),
                settings: normalizeKanbanSettings(null),
                updatesPaused: false,
                shellState: {
                    visible: false,
                    title: "",
                    copy: "",
                    detail: "",
                },
            };

            state.lastPersistedUiJson = serializeUi(state.ui);

            function instanceId() {
                return (
                    String(frame?.dataset?.packageInstanceId || "preview").trim() ||
                    "preview"
                );
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

            function withDatastar(callback) {
                if (datastarApi) {
                    callback(datastarApi);
                    return;
                }
                void datastarReady.then((module) => {
                    if (module) {
                        callback(module);
                    }
                });
            }

            function patchSignals(patch) {
                const safePatch = cloneJsonValue(patch, {});
                if (!safePatch || typeof safePatch !== "object") {
                    return;
                }
                withDatastar((module) => {
                    module?.mergePatch?.(safePatch);
                });
            }

            function readSignalPath(path) {
                if (datastarApi?.getPath) {
                    return cloneJsonValue(datastarApi.getPath(path), null);
                }
                return null;
            }

            function normalizeHostMeta(rawMeta) {
                const rawStreams = rawMeta?.streams || {};
                const globalEnabled = rawStreams.globalEnabled !== false;
                const cardEnabled = rawStreams.cardEnabled !== false;
                return {
                    mode: rawMeta?.mode === "edit" ? "edit" : "view",
                    serverId: String(rawMeta?.serverId || "").trim(),
                    viewId:
                        rawMeta?.viewId == null
                            ? null
                            : Number(rawMeta.viewId) > 0
                              ? Number(rawMeta.viewId)
                              : null,
                    cardState:
                        rawMeta?.cardState &&
                        typeof rawMeta.cardState === "object"
                            ? rawMeta.cardState
                            : {},
                    streams: {
                        globalEnabled,
                        cardEnabled,
                        enabled:
                            typeof rawStreams.enabled === "boolean"
                                ? rawStreams.enabled
                                : globalEnabled && cardEnabled,
                    },
                };
            }

            function dispatchUiEvent(type, detail) {
                app.dispatchEvent(
                    new CustomEvent(type, {
                        bubbles: true,
                        detail: detail || null,
                    }),
                );
            }

            function clampWidth(value) {
                const parsed = Number(value);
                if (!Number.isFinite(parsed)) {
                    return DEFAULT_WIDTH;
                }
                return Math.max(
                    MIN_WIDTH,
                    Math.round(parsed),
                );
            }

            function isBodyMode(value) {
                return BODY_MODES.has(String(value || ""));
            }

            function normalizeUi(rawUi) {
                const nextLanes = {};
                const rawLanes =
                    rawUi?.lanes && typeof rawUi.lanes === "object"
                        ? rawUi.lanes
                        : {};

                for (const column of columns) {
                    const lane = rawLanes[column.key] || {};
                    nextLanes[column.key] = {
                        collapsed: Boolean(lane.collapsed),
                        width: clampWidth(lane.width),
                    };
                }

                const cardModes = {};
                if (rawUi?.cardModes && typeof rawUi.cardModes === "object") {
                    for (const [key, value] of Object.entries(rawUi.cardModes)) {
                        if (isBodyMode(value)) {
                            cardModes[String(key)] = String(value);
                        }
                    }
                }

                const focusedRecordId = Number(rawUi?.focusedRecordId);
                const rawVersion = Number(rawUi?.uiVersion);
                const hasSchemaVersion =
                    Number.isInteger(rawVersion) && rawVersion >= UI_SCHEMA_VERSION;
                const rawDefaultBodyMode = String(rawUi?.defaultBodyMode || "");
                const nextDefaultBodyMode = hasSchemaVersion
                    ? (isBodyMode(rawUi?.defaultBodyMode)
                          ? String(rawUi.defaultBodyMode)
                          : DEFAULT_BODY_MODE)
                    : rawDefaultBodyMode === "head"
                      ? "head"
                      : rawDefaultBodyMode === "full"
                        ? "full"
                        : DEFAULT_BODY_MODE;
                return {
                    lanes: nextLanes,
                    uiVersion: UI_SCHEMA_VERSION,
                    defaultBodyMode: nextDefaultBodyMode,
                    cardModes,
                    focusedRecordId:
                        Number.isInteger(focusedRecordId) && focusedRecordId > 0
                            ? focusedRecordId
                            : null,
                };
            }

            function normalizeKanbanSettings(rawSettings) {
                const rawShowParent =
                    rawSettings?.show_parent_context ?? rawSettings?.showParentContext;
                const rawViewName = rawSettings?.view_name ?? rawSettings?.viewName;
                return {
                    showParentContext: rawShowParent == null ? true : rawShowParent === true,
                    viewName: normalizeViewName(rawViewName),
                };
            }

            function normalizeViewName(rawValue) {
                const value = String(rawValue || "").trim();
                return value;
            }

            function normalizeText(rawValue) {
                return String(rawValue || "").trim().toLowerCase();
            }

            function syncKanbanSettings(rawSettings) {
                const normalized = normalizeKanbanSettings(rawSettings);
                state.settings = normalized;
                if (state.contract) {
                    state.contract.settings = normalized;
                }
                persistStoredKanbanSettings(normalized);
                return normalized;
            }

            function persistKanbanSettingsToHost(settings) {
                persistStoredKanbanSettings(settings);
                window.LinceWidgetHost?.patchCardState?.({
                    kanban_settings: {
                        show_parent_context: Boolean(settings?.showParentContext),
                        view_name: normalizeViewName(settings?.viewName) || null,
                    },
                });
            }

            function kanbanSettingsPayload(settings) {
                const payload = {
                    show_parent_context: Boolean(settings?.showParentContext),
                };
                const normalizedViewName = normalizeViewName(settings?.viewName);
                if (normalizedViewName) {
                    payload.view_name = normalizedViewName;
                }
                return payload;
            }

            function storageKey() {
                return "lince.widget.kanban." + instanceId() + ".ui";
            }

            function kanbanSettingsStorageKey() {
                return "lince.widget.kanban." + instanceId() + ".settings";
            }

            function loadPreviewUi() {
                try {
                    const raw = localStorage.getItem(storageKey());
                    if (!raw) {
                        return normalizeUi(null);
                    }
                    const parsed = JSON.parse(raw);
                    return normalizeUi(parsed?.ui || parsed);
                } catch {
                    return normalizeUi(null);
                }
            }

            function persistPreviewUi(ui) {
                try {
                    localStorage.setItem(storageKey(), JSON.stringify({ ui }));
                } catch {
                }
            }

            function readStoredKanbanSettings() {
                try {
                    const raw = localStorage.getItem(kanbanSettingsStorageKey());
                    if (!raw) {
                        return null;
                    }
                    const parsed = JSON.parse(raw);
                    return normalizeKanbanSettings(parsed?.settings || parsed);
                } catch {
                    return null;
                }
            }

            function persistStoredKanbanSettings(settings) {
                try {
                    localStorage.setItem(
                        kanbanSettingsStorageKey(),
                        JSON.stringify({ settings: normalizeKanbanSettings(settings) }),
                    );
                } catch {
                }
            }

            function serializeUi(ui) {
                return JSON.stringify(normalizeUi(ui));
            }

            function contractUrl() {
                return "/host/widgets/" + encodeURIComponent(instanceId()) + "/contract";
            }

            function streamUrl() {
                return "/host/widgets/" + encodeURIComponent(instanceId()) + "/stream";
            }

            function actionUrl(action) {
                return (
                    "/host/widgets/" +
                    encodeURIComponent(instanceId()) +
                    "/actions/" +
                    encodeURIComponent(String(action || ""))
                );
            }

            function streamEnabled() {
                return state.hostMeta.streams.enabled !== false;
            }

            function laneToQuantity(key) {
                const column = columns.find((entry) => entry.key === key);
                return column ? column.value : 0;
            }

            function emptyFilterState() {
                return {
                    textQuery: "",
                    categories: [],
                    assigneeIds: [],
                    taskTypes: [],
                    quantities: [],
                    onlyWithOpenWorklog: false,
                };
            }

            function normalizeStringArray(values) {
                if (!Array.isArray(values)) {
                    return [];
                }
                const seen = new Set();
                const normalized = [];
                for (const raw of values) {
                    const value = String(raw || "").trim();
                    if (!value) {
                        continue;
                    }
                    const key = value.toLowerCase();
                    if (seen.has(key)) {
                        continue;
                    }
                    seen.add(key);
                    normalized.push(value);
                }
                return normalized;
            }

            function normalizeIntegerArray(values) {
                if (!Array.isArray(values)) {
                    return [];
                }
                const seen = new Set();
                const normalized = [];
                for (const raw of values) {
                    const value = Number(raw);
                    if (!Number.isInteger(value) || seen.has(value)) {
                        continue;
                    }
                    seen.add(value);
                    normalized.push(value);
                }
                return normalized;
            }

            function parseContractFilters(rows) {
                const next = emptyFilterState();
                if (!Array.isArray(rows)) {
                    return next;
                }

                for (const row of rows) {
                    const field = String(row?.field || "");
                    if (field === "text_query") {
                        next.textQuery = String(row?.value || "").trim();
                    } else if (field === "categories_any_json") {
                        next.categories = normalizeStringArray(row?.value);
                    } else if (field === "assignee_ids_any_json") {
                        next.assigneeIds = normalizeIntegerArray(row?.value);
                    } else if (field === "task_types_json") {
                        next.taskTypes = normalizeStringArray(row?.value);
                    } else if (field === "quantities_json") {
                        next.quantities = normalizeIntegerArray(row?.value);
                    } else if (field === "only_with_open_worklog") {
                        next.onlyWithOpenWorklog = row?.value === true;
                    }
                }

                return next;
            }

            function buildFilterRows(filterState) {
                const rows = [];
                const next = filterState || emptyFilterState();
                if (next.textQuery.trim()) {
                    rows.push({
                        field: "text_query",
                        operator: "contains",
                        value: next.textQuery.trim(),
                    });
                }
                if (next.categories.length) {
                    rows.push({
                        field: "categories_any_json",
                        operator: "any_of",
                        value: next.categories,
                    });
                }
                if (next.assigneeIds.length) {
                    rows.push({
                        field: "assignee_ids_any_json",
                        operator: "any_of",
                        value: next.assigneeIds,
                    });
                }
                if (next.taskTypes.length) {
                    rows.push({
                        field: "task_types_json",
                        operator: "any_of",
                        value: next.taskTypes,
                    });
                }
                if (next.quantities.length) {
                    rows.push({
                        field: "quantities_json",
                        operator: "any_of",
                        value: next.quantities,
                    });
                }
                if (next.onlyWithOpenWorklog) {
                    rows.push({
                        field: "only_with_open_worklog",
                        operator: "equals",
                        value: true,
                    });
                }
                return rows;
            }

            function parseTagInput(value) {
                return normalizeStringArray(
                    String(value || "")
                        .split(",")
                        .map((entry) => entry.trim()),
                );
            }

            function clearReconnectTimer() {
                if (state.reconnectTimer) {
                    window.clearTimeout(state.reconnectTimer);
                    state.reconnectTimer = null;
                }
            }

            function stopStream() {
                clearReconnectTimer();
                if (state.streamController) {
                    state.streamController.abort();
                    state.streamController = null;
                }
                state.connected = false;
                state.loadingStream = false;
            }

            function scheduleReconnect() {
                clearReconnectTimer();
                if (!streamEnabled()) {
                    return;
                }
                const delay = Math.min(15000, 1500 * Math.max(1, state.reconnectAttempt + 1));
                state.reconnectAttempt += 1;
                state.reconnectTimer = window.setTimeout(() => {
                    if (state.contract) {
                        connectStream(false);
                        return;
                    }

                    refreshRuntime(false).catch((error) => {
                        state.transportError =
                            error instanceof Error ? error.message : String(error);
                        updateStatus();
                        scheduleReconnect();
                    });
                }, delay);
            }

            function setShellState(title, copy, detail) {
                state.shellState = {
                    visible: true,
                    title: String(title || ""),
                    copy: String(copy || ""),
                    detail: String(detail || ""),
                };
                syncShellSignals();
            }

            function clearShellState() {
                state.shellState = {
                    visible: false,
                    title: "",
                    copy: "",
                    detail: "",
                };
                syncShellSignals();
            }

            function syncShellSignals() {
                const source = state.contract?.source || {};
                let statusLabel = "Waiting";
                let statusClass = "status";
                let statusCopy = "Waiting for the instance-aware Kanban stream.";

                if (!state.contract && state.loadingContract) {
                    statusLabel = "Loading";
                    statusCopy = "Resolving the Kanban contract from the host.";
                } else if (source.requires_auth && source.authenticated === false) {
                    statusLabel = "Locked";
                    statusClass += " is-error";
                    statusCopy = "This widget needs the host login to reconnect the configured server.";
                } else if (state.hostMeta.streams.globalEnabled === false) {
                    statusLabel = "Paused globally";
                    statusClass += " is-paused";
                    statusCopy = "The board disabled streams globally for this workspace.";
                } else if (state.hostMeta.streams.cardEnabled === false) {
                    statusLabel = "Disconnected";
                    statusClass += " is-paused";
                    statusCopy = "This widget disconnected its live stream.";
                } else if (state.updatesPaused) {
                    statusLabel = "Paused updates";
                    statusClass += " is-paused";
                    statusCopy = "The connection is live, but incoming merges are paused locally.";
                } else if (state.connected) {
                    statusLabel = "Live";
                    statusClass += " is-live";
                    statusCopy = state.lastUpdate
                        ? "Live update received at " + state.lastUpdate + "."
                        : "Connected to the filtered Kanban stream.";
                } else if (state.transportError) {
                    statusLabel = "Offline";
                    statusClass += " is-error";
                    statusCopy = state.transportError;
                } else if (state.loadingStream) {
                    statusLabel = "Connecting";
                    statusCopy = "Opening the instance-aware filtered stream.";
                }

                const title =
                    state.contract?.widget?.title ||
                    state.hostMeta?.cardState?.title ||
                    "Kanban";
                const queryText = String(
                    state.viewMeta?.query ||
                        state.contract?.diagnostics?.effective_sql ||
                        "",
                ).trim();
                const viewId =
                    state.viewMeta?.view_id ??
                    state.contract?.source?.view_id ??
                    state.hostMeta.viewId;
                let queryLabel = queryText
                    ? `Query (${queryText.length} chars)`
                    : "No query available";
                if (viewId) {
                    queryLabel = `View ${String(viewId)} query`;
                }

                patchSignals({
                    shell: {
                        title,
                        queryText,
                        queryLabel,
                        queryDisabled: !queryText,
                        statusLabel,
                        statusClass,
                        statusCopy,
                        warningVisible: state.shellState.visible,
                        warningTitle: state.shellState.title,
                        warningCopy: state.shellState.copy,
                        warningDetail: state.shellState.detail,
                        toggleUpdatesLabel: state.updatesPaused
                            ? "Resume updates"
                            : "Pause updates",
                        toggleStreamLabel:
                            state.hostMeta.streams.cardEnabled === false
                                ? "Connect widget"
                                : "Disconnect widget",
                        toggleStreamAccent:
                            state.hostMeta.streams.cardEnabled === false,
                        toggleStreamPaused:
                            state.hostMeta.streams.cardEnabled !== false,
                        reconnectDisabled:
                            !state.contract ||
                            state.hostMeta.streams.enabled === false ||
                            (source.requires_auth &&
                                source.authenticated === false),
                    },
                });
            }

            function updateStatus() {
                const source = state.contract?.source || {};
                syncShellSignals();
            }

            async function fetchContract() {
                state.loadingContract = true;
                updateStatus();
                try {
                    const response = await fetch(contractUrl(), {
                        cache: "no-store",
                    });
                    const payload = await response.json().catch(() => null);
                    const shouldRetry =
                        response.status === 404 || response.status >= 500;
                    if (response.status === 401) {
                        window.LinceWidgetHost?.invalidateServerAuth?.(
                            state.hostMeta.serverId || "",
                        );
                        state.contract = null;
                        setShellState(
                            "Host login required",
                            "This Kanban cannot resolve the configured server session.",
                            payload?.error || "",
                        );
                        return false;
                    }
                    if (!response.ok) {
                        state.contract = null;
                        setShellState(
                            response.status === 422 ? "Kanban misconfigured" : "Kanban unavailable",
                            payload?.error || "The Kanban contract could not be resolved.",
                            "",
                        );
                        if (shouldRetry) {
                            scheduleReconnect();
                        }
                        return false;
                    }

                    state.contract = payload;
                    syncKanbanSettings(
                        payload?.settings ||
                            state.hostMeta?.cardState?.kanban_settings ||
                            null,
                    );
                    state.reconnectAttempt = 0;
                    clearReconnectTimer();
                    state.formOptions = payload?.formOptions || null;
                    state.draftFilters = parseContractFilters(payload?.filters?.rows);
                    renderActiveFilters();
                    clearShellState();
                    if (
                        Array.isArray(payload?.filters?.rows) &&
                        payload.filters.rows.length
                    ) {
                        elements.toolbarState.dataset.filtersVersion = String(
                            payload.filters.filtersVersion ||
                                payload.filters.filters_version ||
                                0,
                        );
                    }
                    if (state.activeSheet === "view") {
                        renderViewSheet();
                    }
                    return true;
                } catch (error) {
                    state.contract = null;
                    setShellState(
                        "Kanban unavailable",
                        "The widget could not load its host contract.",
                        error instanceof Error ? error.message : String(error),
                    );
                    scheduleReconnect();
                    return false;
                } finally {
                    state.loadingContract = false;
                    updateStatus();
                }
            }

            async function loadFormOptions() {
                try {
                    return await postAction("load-form-options", {});
                } catch (error) {
                    state.transportError =
                        error instanceof Error ? error.message : String(error);
                    updateStatus();
                    return null;
                }
            }

            async function ensureFormOptionsLoaded(force = false) {
                if (!force && state.formOptions) {
                    return state.formOptions;
                }
                if (!force && state.formOptionsPromise) {
                    return state.formOptionsPromise;
                }
                if (force) {
                    state.formOptionsRequestId += 1;
                }
                const requestId = state.formOptionsRequestId;
                state.formOptionsPromise = loadFormOptions()
                    .then((formOptions) => {
                        if (state.formOptionsRequestId === requestId) {
                            state.formOptions = formOptions;
                        }
                        return state.formOptionsRequestId === requestId
                            ? state.formOptions
                            : null;
                    })
                    .catch(() => null)
                    .finally(() => {
                        if (state.formOptionsRequestId === requestId) {
                            state.formOptionsPromise = null;
                        }
                    });
                return state.formOptionsPromise;
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

            function patchHtml(node, html) {
                if (!node) {
                    return;
                }
                node.innerHTML = typeof html === "string" ? html : "";
            }

            function hydrateMarkdownBlocks(root) {
                if (!root) {
                    return;
                }
                for (const block of root.querySelectorAll("[data-markdown-source]")) {
                    const source = String(block.dataset.markdownSource || "");
                    if (block.dataset.markdownRendered === source) {
                        continue;
                    }
                    block.innerHTML = source ? renderMarkdown(source) : "";
                    block.dataset.markdownRendered = source;
                }
            }

            function isoToInput(value) {
                const text = String(value || "").trim();
                if (!text) {
                    return "";
                }
                return text.replace("Z", "").slice(0, 16);
            }

            function inputToIso(value) {
                const text = String(value || "").trim();
                if (!text) {
                    return null;
                }
                const date = new Date(text);
                if (Number.isNaN(date.getTime())) {
                    return text;
                }
                return date.toISOString().replace(/\.\d{3}Z$/, "Z");
            }

            function openSheet(name) {
                if (name === "filter") {
                    dispatchUiEvent("kanban-open-filter");
                } else if (name === "create") {
                    dispatchUiEvent("kanban-open-create");
                } else if (name === "edit") {
                    dispatchUiEvent("kanban-open-edit");
                }
            }

            function closeSheet(name) {
                if (name === "filter" || name === "create" || name === "edit") {
                    dispatchUiEvent("kanban-close-sheets");
                }
            }

            function openFocusSheet() {
                dispatchUiEvent("kanban-open-focus");
            }

            function closeFocusSheet() {
                dispatchUiEvent("kanban-close-focus");
            }

            function isSheetVisible(sheet) {
                return Boolean(
                    sheet &&
                        sheet.isConnected &&
                        window.getComputedStyle(sheet).display !== "none",
                );
            }

            function renderActiveFilters() {
                const chips = [];
                if (state.draftFilters.textQuery.trim()) {
                    chips.push({
                        key: "text",
                        label: `text: ${state.draftFilters.textQuery.trim()}`,
                    });
                }
                if (state.draftFilters.categories.length) {
                    chips.push({
                        key: "categories",
                        label: `categories: ${state.draftFilters.categories.join(", ")}`,
                    });
                }
                if (state.draftFilters.assigneeIds.length) {
                    chips.push({
                        key: "assignees",
                        label: `assignees: ${state.draftFilters.assigneeIds.length}`,
                    });
                }
                if (state.draftFilters.taskTypes.length) {
                    chips.push({
                        key: "taskTypes",
                        label: `types: ${state.draftFilters.taskTypes.join(", ")}`,
                    });
                }
                if (state.draftFilters.quantities.length) {
                    chips.push({
                        key: "quantities",
                        label: `columns: ${state.draftFilters.quantities.join(", ")}`,
                    });
                }
                if (state.draftFilters.onlyWithOpenWorklog) {
                    chips.push({
                        key: "openWorklog",
                        label: "open worklog",
                    });
                }

                if (!chips.length) {
                    elements.activeFilters.innerHTML = "";
                    return;
                }

                elements.activeFilters.innerHTML = chips
                    .map(
                        (chip) =>
                            `<span class="chip">${escapeHtml(chip.label)} <button type="button" data-clear-filter="${escapeHtml(chip.key)}">×</button></span>`,
                    )
                    .join("");
            }

            function clearFilterKey(key) {
                const next = {
                    ...state.draftFilters,
                    categories: [...state.draftFilters.categories],
                    assigneeIds: [...state.draftFilters.assigneeIds],
                    taskTypes: [...state.draftFilters.taskTypes],
                    quantities: [...state.draftFilters.quantities],
                };
                if (key === "text") {
                    next.textQuery = "";
                } else if (key === "categories") {
                    next.categories = [];
                } else if (key === "assignees") {
                    next.assigneeIds = [];
                } else if (key === "taskTypes") {
                    next.taskTypes = [];
                } else if (key === "quantities") {
                    next.quantities = [];
                } else if (key === "openWorklog") {
                    next.onlyWithOpenWorklog = false;
                }
                state.draftFilters = next;
                renderActiveFilters();
            }

            async function applyFiltersAndRefresh() {
                const rows = buildFilterRows(state.draftFilters);
                const outcome = await postAction("apply-filters", { filters: rows });
                if (state.contract) {
                    state.contract.filters = state.contract.filters || {};
                    state.contract.filters.rows = rows;
                    state.contract.filters.filtersVersion =
                        outcome?.detail?.filters_version || 0;
                }
                window.LinceWidgetHost?.patchCardState?.({
                    filters: rows,
                    filters_version: outcome?.detail?.filters_version || 0,
                });
                renderActiveFilters();
                await refreshRuntime(true);
            }

            function assigneeOptions() {
                return Array.isArray(state.formOptions?.assignees)
                    ? state.formOptions.assignees
                    : [];
            }

            function renderCheckboxGroup(name, values, selectedValues) {
                const selected = new Set(selectedValues || []);
                return values
                    .map((entry) => {
                        const value = entry.value;
                        const label = entry.label;
                        const checked = selected.has(value) ? "checked" : "";
                        return `<label class="checkRow"><input type="checkbox" name="${escapeHtml(name)}" value="${escapeHtml(value)}" ${checked}> <span>${escapeHtml(label)}</span></label>`;
                    })
                    .join("");
            }

            function parentOptions() {
                return Array.isArray(state.formOptions?.parentRecords)
                    ? state.formOptions.parentRecords
                    : [];
            }

            function parentCategoryQuery() {
                return String(state.formOptions?.parentCategoryQuery || "").trim();
            }

            function parentCategoryDefault(mode, draft, options) {
                const fromOptions = String(options.parentCategoryQuery || "").trim();
                if (fromOptions) {
                    return fromOptions;
                }
                if (mode === "edit") {
                    return normalizeStringArray(draft.categories || []).join(", ");
                }
                return "";
            }

            function parentRecordMatchesFilters(record, headQuery, categoryTerms, currentRecordId) {
                const recordId = Number(record?.id || 0);
                if (Number.isInteger(currentRecordId) && currentRecordId > 0 && recordId === currentRecordId) {
                    return false;
                }

                const head = String(record?.head || "").toLowerCase();
                if (headQuery && !head.includes(headQuery)) {
                    return false;
                }

                if (!categoryTerms.length) {
                    return true;
                }

                const categories = normalizeStringArray(record?.categories || []).map((entry) => entry.toLowerCase());
                if (!categories.length) {
                    return false;
                }

                return categoryTerms.some((term) =>
                    categories.some((category) =>
                        category.includes(term) || term.includes(category),
                    ),
                );
            }

            function renderParentChoices(root) {
                if (!root) {
                    return;
                }

                const options = parentOptions();
                const picker = root.querySelector("[data-parent-picker]");
                if (!picker) {
                    return;
                }

                const currentRecordId = Number(root.dataset.recordId || 0) || null;
                const hiddenParent = root.querySelector("[name='parentId']");
                const selectedParentId = Number(hiddenParent?.value || 0) || null;
                const headQuery = String(
                    root.querySelector("[data-parent-search-head]")?.value || "",
                )
                    .trim()
                    .toLowerCase();
                const categoryTerms = parseTagInput(
                    root.querySelector("[data-parent-search-categories]")?.value || "",
                ).map((value) => value.toLowerCase());
                const filtered = options.filter((record) =>
                    parentRecordMatchesFilters(record, headQuery, categoryTerms, currentRecordId),
                );

                const selectedParent = options.find(
                    (record) => Number(record.id || 0) === selectedParentId,
                ) || null;
                const summary = picker.querySelector("[data-parent-selection-summary]");
                if (summary) {
                    summary.textContent = selectedParent
                        ? `Selected: #${selectedParent.id} ${String(selectedParent.head || "Untitled")}`
                        : selectedParentId > 0
                          ? `Selected: #${selectedParentId} ${String(root.dataset.parentHead || "Untitled")}`
                          : "Selected: no parent";
                }

                const matchCount = picker.querySelector("[data-parent-match-count]");
                if (matchCount) {
                    matchCount.textContent = filtered.length
                        ? `${filtered.length} matches`
                        : "No matches";
                }

                if (hiddenParent) {
                    hiddenParent.value = selectedParentId > 0 ? String(selectedParentId) : "";
                }

                const list = picker.querySelector("[data-parent-choices]");
                if (!list) {
                    return;
                }

                const categoryChips = (categories) =>
                    (() => {
                        const normalizedCategories = normalizeStringArray(categories || []);
                        return normalizedCategories.length
                            ? normalizedCategories
                                  .map((category) => `<span class="chip">${escapeHtml(category)}</span>`)
                                  .join("")
                            : `<span class="small">No categories</span>`;
                    })();
                const renderedChoices = filtered.length
                    ? filtered
                                  .map((record) => {
                              const recordId = Number(record.id || 0);
                              const selectedClass =
                                  recordId > 0 && recordId === selectedParentId
                                      ? " is-selected"
                                      : "";
                              return `
                                  <button class="parentChoice${selectedClass}" type="button" data-parent-choice="${escapeHtml(String(recordId))}" data-parent-head="${escapeHtml(String(record.head || "Untitled"))}">
                                      <span class="parentChoice__head">#${escapeHtml(String(recordId))} ${escapeHtml(String(record.head || "Untitled"))}</span>
                                      <span class="chipBar parentChoice__chips">${categoryChips(record.categories || [])}</span>
                                  </button>
                              `;
                          })
                          .join("")
                    : `<p class="small parentChooserEmpty">No possible fathers match these filters.</p>`;

                list.innerHTML = `
                    <button class="parentChoice${selectedParentId ? "" : " is-selected"}" type="button" data-parent-choice="" data-parent-head="">
                        <span class="parentChoice__head">No parent</span>
                        <span class="small">Clear the parent link</span>
                    </button>
                    ${renderedChoices}
                `;
            }

            function renderFilterSheet() {
                const assignees = assigneeOptions();
                const taskTypes =
                    Array.isArray(state.contract?.data_contract?.task_type_enum)
                        ? state.contract.data_contract.task_type_enum
                        : ["epic", "feature", "task", "other"];
                const quantities = columns.map((column) => ({
                    value: String(column.value),
                    label: column.label,
                }));
                const categories = normalizeStringArray(state.formOptions?.categories || []);
                elements.filterSheetBody.innerHTML = `
                    <div class="sheetBody">
                        <div class="fieldBlock">
                            <label class="fieldLabel" for="filter-text-query">Text contains</label>
                            <input class="field" id="filter-text-query" type="text" value="${escapeHtml(state.draftFilters.textQuery)}" placeholder="search in head or body">
                        </div>
                        <div class="fieldBlock">
                            <label class="fieldLabel" for="filter-categories">Categories</label>
                            <input class="field" id="filter-categories" type="text" list="kanban-category-options" value="${escapeHtml(state.draftFilters.categories.join(", "))}" placeholder="project-1, design">
                            <datalist id="kanban-category-options">
                                ${categories.map((category) => `<option value="${escapeHtml(category)}"></option>`).join("")}
                            </datalist>
                        </div>
                        <div class="fieldBlock">
                            <div class="fieldLabel">Assignees</div>
                            <div class="checkGrid">${renderCheckboxGroup(
                                "filter-assignee",
                                assignees.map((assignee) => ({
                                    value: String(assignee.id),
                                    label: assignee.name || assignee.username || `user ${assignee.id}`,
                                })),
                                state.draftFilters.assigneeIds.map(String),
                            )}</div>
                        </div>
                        <div class="fieldBlock">
                            <div class="fieldLabel">Task types</div>
                            <div class="checkGrid">${renderCheckboxGroup(
                                "filter-task-type",
                                taskTypes.map((value) => ({ value, label: value })),
                                state.draftFilters.taskTypes,
                            )}</div>
                        </div>
                        <div class="fieldBlock">
                            <div class="fieldLabel">Columns</div>
                            <div class="checkGrid">${renderCheckboxGroup(
                                "filter-quantity",
                                quantities,
                                state.draftFilters.quantities.map(String),
                            )}</div>
                        </div>
                        <label class="checkRow"><input type="checkbox" id="filter-open-worklog" ${state.draftFilters.onlyWithOpenWorklog ? "checked" : ""}> <span>Only tasks with open worklog</span></label>
                        <div class="sheetActions">
                            <button class="toolbarBtn" type="button" data-clear-filters="true">Clear all</button>
                            <button class="toolbarBtn toolbarBtn--accent" type="button" data-apply-filters="true">Apply filters</button>
                        </div>
                    </div>
                `;
            }

            function viewOptions() {
                return Array.isArray(state.formOptions?.views)
                    ? state.formOptions.views
                    : [];
            }

            function currentSourceViewId() {
                const candidate =
                    Number(state.contract?.source?.view_id || state.hostMeta.viewId || 0) || 0;
                return candidate > 0 ? candidate : null;
            }

            function viewLabel(view) {
                const id = Number(view?.id || 0);
                const name = String(view?.name || "").trim() || "Untitled";
                return `#${id} ${name}`;
            }

            function findViewById(viewId) {
                const id = Number(viewId || 0);
                if (!Number.isInteger(id) || id <= 0) {
                    return null;
                }
                return viewOptions().find((view) => Number(view?.id || 0) === id) || null;
            }

            function currentSourceView() {
                const currentId = currentSourceViewId();
                return currentId ? findViewById(currentId) : null;
            }

            function sourceViewSuggestionElements() {
                return {
                    input: elements.viewSheetBody?.querySelector("#kanban-source-view"),
                    panel: elements.viewSheetBody?.querySelector("#kanban-source-view-suggestions"),
                    summary: elements.viewSheetBody?.querySelector("#kanban-source-view-summary"),
                    saveButton: elements.viewSheetBody?.querySelector("[data-save-view-config]"),
                };
            }

            function sourceViewMatchesQuery(view, query) {
                const needle = normalizeText(query);
                if (!needle) {
                    return true;
                }

                const idText = String(Number(view?.id || 0));
                const label = normalizeText(viewLabel(view));
                const name = normalizeText(view?.name || "");
                return (
                    idText.includes(needle) ||
                    name.includes(needle) ||
                    label.includes(needle)
                );
            }

            function filteredSourceViewSuggestions() {
                const { input } = sourceViewSuggestionElements();
                const query = String(input?.value || "").trim();
                const rows = viewOptions()
                    .filter((view) => sourceViewMatchesQuery(view, query))
                    .sort((left, right) => {
                        const leftName = normalizeText(left?.name || "");
                        const rightName = normalizeText(right?.name || "");
                        if (leftName !== rightName) {
                            return leftName.localeCompare(rightName);
                        }
                        return Number(left?.id || 0) - Number(right?.id || 0);
                    });
                return rows.slice(0, 12);
            }

            function hideSourceViewSuggestions() {
                const { panel } = sourceViewSuggestionElements();
                if (!panel) {
                    return;
                }
                panel.hidden = true;
                panel.innerHTML = "";
            }

            function renderSourceViewSuggestions() {
                const { panel, input } = sourceViewSuggestionElements();
                if (!panel || !input) {
                    return;
                }
                const rows = filteredSourceViewSuggestions();
                if (!rows.length) {
                    hideSourceViewSuggestions();
                    return;
                }
                panel.innerHTML = rows
                    .map((view) => {
                        const selectedClass =
                            Number(view?.id || 0) === currentSourceViewId()
                                ? " is-selected"
                                : "";
                        const meta =
                            Number(view?.id || 0) === currentSourceViewId()
                                ? "Current source view"
                                : "Type to narrow the list.";
                        return `
                            <button
                                class="suggestionButton${selectedClass}"
                                type="button"
                                data-source-view-id="${escapeHtml(String(view?.id || 0))}"
                                data-source-view-name="${escapeHtml(String(view?.name || ""))}"
                            >
                                <strong>${escapeHtml(viewLabel(view))}</strong>
                                <span class="suggestionMeta">${escapeHtml(meta)}</span>
                            </button>
                        `;
                    })
                    .join("");
                panel.hidden = false;
            }

            function updateSourceViewSummary() {
                const { summary, saveButton } = sourceViewSuggestionElements();
                if (!summary) {
                    return;
                }
                const current = currentSourceView();
                const currentLabel = current
                    ? viewLabel(current)
                    : currentSourceViewId()
                      ? `#${currentSourceViewId()}`
                      : "";
                const draft = String(sourceViewSuggestionElements().input?.value || "").trim();
                if (draft && normalizeText(draft) !== normalizeText(currentLabel)) {
                    summary.textContent = `Selected source view: ${draft}`;
                } else if (currentLabel) {
                    summary.textContent = `Current source view: ${currentLabel}`;
                } else {
                    summary.textContent = "Pick a plain SQL view to derive this board from.";
                }
                if (saveButton) {
                    const sourceValue = String(
                        sourceViewSuggestionElements().input?.value || "",
                    ).trim();
                    const viewNameValue = String(
                        elements.viewSheetBody?.querySelector("#kanban-view-name")?.value || "",
                    ).trim();
                    saveButton.disabled = !sourceValue || !viewNameValue;
                }
            }

            function resolveSourceViewFromInput(rawValue) {
                const query = String(rawValue || "").trim();
                if (!query) {
                    return null;
                }
                const exactId = Number(query.replace(/^#/, ""));
                if (Number.isInteger(exactId) && exactId > 0) {
                    const exact = findViewById(exactId);
                    if (exact) {
                        return exact;
                    }
                }
                const normalizedQuery = normalizeText(query);
                const exactName = viewOptions().find((view) => {
                    const name = normalizeText(view?.name || "");
                    const label = normalizeText(viewLabel(view));
                    return name === normalizedQuery || label === normalizedQuery;
                }) || null;
                if (exactName) {
                    return exactName;
                }
                return filteredSourceViewSuggestions()[0] || null;
            }

            function reconnectDisabled() {
                const source = state.contract?.source || {};
                return (
                    !state.contract ||
                    state.hostMeta.streams.enabled === false ||
                    (source.requires_auth && source.authenticated === false)
                );
            }

            function renderViewSheet() {
                if (!elements.viewSheetBody) {
                    return;
                }

                const settings = normalizeKanbanSettings(state.settings);
                const activeBodyMode = isBodyMode(state.ui.defaultBodyMode)
                    ? state.ui.defaultBodyMode
                    : DEFAULT_BODY_MODE;
                const viewName = String(settings.viewName || "");
                const currentSource = currentSourceView();
                const sourceViewQuery = String(
                    state.sourceViewQuery ||
                        (currentSource
                            ? viewLabel(currentSource)
                            : currentSourceViewId()
                              ? `#${currentSourceViewId()}`
                              : ""),
                );
                const toggleUpdatesLabel = state.updatesPaused
                    ? "Resume updates"
                    : "Pause updates";
                const toggleStreamLabel =
                    state.hostMeta.streams.cardEnabled === false
                        ? "Connect widget"
                        : "Disconnect widget";
                const streamButtonClass =
                    state.hostMeta.streams.cardEnabled === false
                        ? "toolbarBtn toolbarBtn--accent"
                        : "toolbarBtn toolbarBtn--paused";
                const reconnectAttr = reconnectDisabled() ? "disabled" : "";
                const presets = [
                    { mode: "head", label: "All head" },
                    { mode: "compact", label: "All compact" },
                    { mode: "full", label: "All full" },
                ];

                elements.viewSheetBody.innerHTML = `
                    <div class="sheetBody">
                        <section class="viewSection">
                            <div class="fieldBlock">
                                <div class="fieldLabel">Source view</div>
                                <p class="small">Choose the plain SQL view this board will derive from.</p>
                                <div class="autocompleteHost">
                                    <input class="field" id="kanban-source-view" type="text" autocomplete="off" spellcheck="false" placeholder="name or id" value="${escapeHtml(sourceViewQuery)}">
                                    <div id="kanban-source-view-suggestions" class="suggestionPanel" hidden></div>
                                </div>
                                <p class="small" id="kanban-source-view-summary"></p>
                            </div>
                        </section>
                        <section class="viewSection">
                            <div class="fieldBlock">
                                <div class="fieldLabel">Derived view name</div>
                                <p class="small">Set a human-readable name so this board reuses the same view instead of generating a generic one.</p>
                                <input class="field" id="kanban-view-name" type="text" value="${escapeHtml(viewName)}" placeholder="My Kanban">
                            </div>
                        </section>
                        <div class="sheetActions">
                            <button class="toolbarBtn toolbarBtn--accent" type="button" data-save-view-config="true">Save view</button>
                        </div>
                        <section class="viewSection">
                            <div class="fieldBlock">
                                <div class="fieldLabel">Filters</div>
                                <p class="small">Open the full filter editor and keep the active chips visible in the header.</p>
                            </div>
                            <div class="sheetActions">
                                <button class="toolbarBtn" type="button" data-open-filters-from-view="true">Open filters</button>
                            </div>
                        </section>
                        <section class="viewSection">
                            <div class="fieldBlock">
                                <div class="fieldLabel">Card density</div>
                                <p class="small">Set the default body mode for every card and clear temporary per-card overrides.</p>
                            </div>
                            <div class="viewPresetGrid">
                                ${presets
                                    .map((preset) => {
                                        const activeClass =
                                            preset.mode === activeBodyMode ? " is-active" : "";
                                        return `<button class="toolbarBtn${activeClass}" type="button" data-set-default-body-mode="${escapeHtml(preset.mode)}">${escapeHtml(preset.label)}</button>`;
                                    })
                                    .join("")}
                            </div>
                        </section>
                        <section class="viewSection">
                            <div class="fieldBlock">
                                <div class="fieldLabel">Parent context</div>
                                <p class="small">Show the parent task above each card title. The derived query only joins parent data while this stays enabled.</p>
                            </div>
                            <label class="checkRow"><input type="checkbox" data-setting-show-parent ${settings.showParentContext ? "checked" : ""}> <span>Show parent task above card titles</span></label>
                        </section>
                        <section class="viewSection">
                            <div class="fieldBlock">
                                <div class="fieldLabel">Live widget</div>
                                <p class="small">Control local merge behavior and the live stream from the same place.</p>
                            </div>
                            <div class="sheetActions">
                                <button class="toolbarBtn" type="button" data-toggle-updates-from-view="true">${escapeHtml(toggleUpdatesLabel)}</button>
                                <button class="${streamButtonClass}" type="button" data-toggle-stream-from-view="true">${escapeHtml(toggleStreamLabel)}</button>
                                <button class="toolbarBtn toolbarBtn--accent" type="button" data-reconnect-from-view="true" ${reconnectAttr}>Reconnect</button>
                            </div>
                        </section>
                    </div>
                `;
                elements.sourceViewInput = elements.viewSheetBody.querySelector("#kanban-source-view");
                elements.sourceViewSuggestions = elements.viewSheetBody.querySelector("#kanban-source-view-suggestions");
                updateSourceViewSummary();
                renderSourceViewSuggestions();
            }

            async function updateKanbanSettings(nextSettings) {
                const previous = normalizeKanbanSettings(state.settings);
                if (
                    Object.prototype.hasOwnProperty.call(nextSettings || {}, "viewName") &&
                    !normalizeViewName(nextSettings.viewName)
                ) {
                    throw new Error("Kanban view name is required.");
                }
                const optimistic = normalizeKanbanSettings({
                    ...previous,
                    ...nextSettings,
                });
                const payload = {
                    show_parent_context: Boolean(optimistic.showParentContext),
                };
                const normalizedViewName = normalizeViewName(optimistic.viewName);
                if (normalizedViewName) {
                    payload.view_name = normalizedViewName;
                }

                syncKanbanSettings(optimistic);
                persistKanbanSettingsToHost(optimistic);
                if (isSheetVisible(elements.viewSheet)) {
                    renderViewSheet();
                }

                try {
                    const outcome = await postAction("update-settings", payload);
                    const resolved = syncKanbanSettings(
                        outcome?.detail?.settings || optimistic,
                    );
                    persistKanbanSettingsToHost(resolved);
                    if (isSheetVisible(elements.viewSheet)) {
                        renderViewSheet();
                    }
                    await refreshRuntime(true);
                } catch (error) {
                    syncKanbanSettings(previous);
                    persistKanbanSettingsToHost(previous);
                    if (isSheetVisible(elements.viewSheet)) {
                        renderViewSheet();
                    }
                    throw error;
                }
            }

            async function saveViewConfig() {
                const previousSourceId = currentSourceViewId();
                const { input } = sourceViewSuggestionElements();
                const selectedView = resolveSourceViewFromInput(input?.value || "");
                const viewNameInput = elements.viewSheetBody?.querySelector("#kanban-view-name");
                const viewName = normalizeViewName(viewNameInput?.value || "");

                if (!selectedView) {
                    throw new Error("Choose a plain SQL source view first.");
                }
                if (!viewName) {
                    throw new Error("Kanban view name is required.");
                }

                const optimisticLabel = viewLabel(selectedView);
                if (input) {
                    input.value = optimisticLabel;
                }
                state.sourceViewQuery = optimisticLabel;
                updateSourceViewSummary();

                try {
                    const outcome = await postAction("update-view-config", {
                        viewId: Number(selectedView.id || 0),
                        viewName,
                    });
                    const resolvedView =
                        outcome?.detail?.sourceView || {
                            id: Number(selectedView.id || 0),
                            name: String(selectedView.name || ""),
                        };
                    state.sourceViewQuery = viewLabel(resolvedView);
                    if (input) {
                        input.value = state.sourceViewQuery;
                    }
                    if (viewNameInput) {
                        viewNameInput.value = viewName;
                    }
                    updateSourceViewSummary();
                    hideSourceViewSuggestions();
                } catch (error) {
                    state.sourceViewQuery =
                        previousSourceId && findViewById(previousSourceId)
                            ? viewLabel(findViewById(previousSourceId))
                            : "";
                    if (input) {
                        input.value = state.sourceViewQuery || "";
                    }
                    updateSourceViewSummary();
                    hideSourceViewSuggestions();
                    throw error;
                }

                await refreshRuntime(true);
            }

            function formOptionsReady() {
                return state.formOptions || {
                    assignees: [],
                    categories: [],
                    parentRecords: [],
                    views: [],
                    parentCategoryQuery: "",
                    sourceView: null,
                };
            }

            function renderRecordForm(mode, draft) {
                const options = formOptionsReady();
                const taskTypes =
                    Array.isArray(state.contract?.data_contract?.task_type_enum)
                        ? state.contract.data_contract.task_type_enum
                        : ["epic", "feature", "task", "other"];
                const categoryInput = (draft.categories || []).join(", ");
                const currentParentId = draft.parentId == null ? "" : String(draft.parentId);
                const assigneeIds = new Set((draft.assigneeIds || []).map(Number));
                const parentCategoryInput = parentCategoryDefault(mode, draft, options);
                const quantityOptions = columns
                    .map(
                        (column) =>
                            `<option value="${column.value}" ${Number(draft.quantity) === column.value ? "selected" : ""}>${escapeHtml(column.label)}</option>`,
                    )
                    .join("");
                const taskTypeOptions = [
                    `<option value="">(not in Kanban)</option>`,
                    ...taskTypes.map(
                        (value) =>
                            `<option value="${escapeHtml(value)}" ${draft.taskType === value ? "selected" : ""}>${escapeHtml(value)}</option>`,
                    ),
                ].join("");
                const assigneeChecks = renderCheckboxGroup(
                    "record-assignee",
                    options.assignees.map((assignee) => ({
                        value: String(assignee.id),
                        label: assignee.name || assignee.username || `user ${assignee.id}`,
                    })),
                    Array.from(assigneeIds).map(String),
                );
                const warning =
                    mode === "edit" && !draft.taskType
                        ? `<p class="small">Saving without task_type will remove this record from the Kanban after refresh.</p>`
                        : "";
                const categoryOptions = normalizeStringArray(options.categories || []);

                return `
                    <div class="sheetBody" data-record-form-root="" data-record-id="${escapeHtml(String(draft.recordId || ""))}" data-parent-head="${escapeHtml(String(draft.parentHead || ""))}">
                        ${warning}
                        <div class="formGrid">
                            <div class="fieldBlock">
                                <label class="fieldLabel" for="${mode}-head">Head</label>
                                <input class="field" id="${mode}-head" name="head" type="text" value="${escapeHtml(draft.head || "")}">
                            </div>
                            <div class="fieldBlock">
                                <label class="fieldLabel" for="${mode}-body">Body</label>
                                <textarea class="textarea" id="${mode}-body" name="body">${escapeHtml(draft.body || "")}</textarea>
                            </div>
                            <div class="fieldBlock">
                                <label class="fieldLabel" for="${mode}-quantity">Column</label>
                                <select class="select" id="${mode}-quantity" name="quantity">${quantityOptions}</select>
                            </div>
                            <div class="fieldBlock">
                                <label class="fieldLabel" for="${mode}-task-type">Task type</label>
                                <select class="select" id="${mode}-task-type" name="taskType">${taskTypeOptions}</select>
                            </div>
                            <div class="fieldBlock">
                                <label class="fieldLabel" for="${mode}-categories">Categories</label>
                                <input class="field" id="${mode}-categories" name="categories" type="text" value="${escapeHtml(categoryInput)}" placeholder="task, project-1, design">
                            </div>
                            <div class="fieldBlock">
                                <label class="fieldLabel" for="${mode}-start-at">Start</label>
                                <input class="field" id="${mode}-start-at" name="startAt" type="datetime-local" value="${escapeHtml(isoToInput(draft.startAt))}">
                            </div>
                            <div class="fieldBlock">
                                <label class="fieldLabel" for="${mode}-end-at">End</label>
                                <input class="field" id="${mode}-end-at" name="endAt" type="datetime-local" value="${escapeHtml(isoToInput(draft.endAt))}">
                            </div>
                            <div class="fieldBlock">
                                <label class="fieldLabel" for="${mode}-estimate-seconds">Estimate seconds</label>
                                <input class="field" id="${mode}-estimate-seconds" name="estimateSeconds" type="number" min="0" step="60" value="${escapeHtml(draft.estimateSeconds == null ? "" : String(draft.estimateSeconds))}">
                            </div>
                            <div class="fieldBlock">
                                <div class="fieldLabel">Assignees</div>
                                <div class="checkGrid">${assigneeChecks}</div>
                            </div>
                            <div class="fieldBlock">
                                <div class="parentChooser" data-parent-picker="">
                                    <div class="parentChooserHeader">
                                        <div>
                                            <div class="fieldLabel">Parent</div>
                                            <div class="small" data-parent-selection-summary="">Selected: no parent</div>
                                        </div>
                                        <div class="small" data-parent-match-count=""></div>
                                    </div>
                                    <div class="fieldBlock">
                                        <label class="fieldLabel" for="${mode}-parent-head-search">Search head</label>
                                        <input class="field" id="${mode}-parent-head-search" data-parent-search-head="" type="text" value="" placeholder="Search possible fathers by head">
                                    </div>
                                    <div class="fieldBlock">
                                        <label class="fieldLabel" for="${mode}-parent-categories-search">Category filter</label>
                                        <input class="field" id="${mode}-parent-categories-search" data-parent-search-categories="" type="text" list="${mode}-parent-category-options" value="${escapeHtml(parentCategoryInput)}" placeholder="task, project-1, design">
                                        <datalist id="${mode}-parent-category-options">
                                            ${categoryOptions.map((category) => `<option value="${escapeHtml(category)}"></option>`).join("")}
                                        </datalist>
                                    </div>
                                    <input type="hidden" id="${mode}-parent-id" name="parentId" value="${escapeHtml(currentParentId)}">
                                    <div class="parentChooserList" data-parent-choices="">
                                        <p class="small">Loading possible fathers...</p>
                                    </div>
                                </div>
                            </div>
                        </div>
                        <div class="sheetActions">
                            ${mode === "edit" ? `<button class="toolbarBtn" type="button" data-submit-delete="${draft.recordId}">Delete</button>` : ""}
                            <button class="toolbarBtn toolbarBtn--accent" type="button" data-submit-record="${mode}">${mode === "edit" ? "Save changes" : "Create task"}</button>
                        </div>
                    </div>
                `;
            }

            function readFilterSheet() {
                return {
                    textQuery: String(elements.filterSheetBody.querySelector("#filter-text-query")?.value || "").trim(),
                    categories: parseTagInput(elements.filterSheetBody.querySelector("#filter-categories")?.value || ""),
                    assigneeIds: Array.from(elements.filterSheetBody.querySelectorAll("input[name='filter-assignee']:checked")).map((node) => Number(node.value)).filter(Number.isInteger),
                    taskTypes: Array.from(elements.filterSheetBody.querySelectorAll("input[name='filter-task-type']:checked")).map((node) => String(node.value)),
                    quantities: Array.from(elements.filterSheetBody.querySelectorAll("input[name='filter-quantity']:checked")).map((node) => Number(node.value)).filter(Number.isInteger),
                    onlyWithOpenWorklog: elements.filterSheetBody.querySelector("#filter-open-worklog")?.checked === true,
                };
            }

            function readRecordForm(root) {
                return {
                    head: String(root.querySelector("[name='head']")?.value || ""),
                    body: String(root.querySelector("[name='body']")?.value || ""),
                    quantity: Number(root.querySelector("[name='quantity']")?.value || 0),
                    taskType: String(root.querySelector("[name='taskType']")?.value || "").trim(),
                    categories: parseTagInput(root.querySelector("[name='categories']")?.value || ""),
                    startAt: inputToIso(root.querySelector("[name='startAt']")?.value || ""),
                    endAt: inputToIso(root.querySelector("[name='endAt']")?.value || ""),
                    estimateSeconds: (() => {
                        const value = String(root.querySelector("[name='estimateSeconds']")?.value || "").trim();
                        return value ? Number(value) : null;
                    })(),
                    assigneeIds: Array.from(root.querySelectorAll("input[name='record-assignee']:checked")).map((node) => Number(node.value)).filter(Number.isInteger),
                    parentId: (() => {
                        const value = String(root.querySelector("[name='parentId']")?.value || "").trim();
                        return value ? Number(value) : null;
                    })(),
                };
            }

            function renderCreateSheet() {
                elements.createSheetBody.innerHTML = renderRecordForm("create", {
                    head: "",
                    body: "",
                    quantity: 0,
                    taskType: "",
                    categories: [],
                    startAt: null,
                    endAt: null,
                    estimateSeconds: null,
                    assigneeIds: [],
                    parentId: null,
                });
                renderParentChoices(elements.createSheetBody);
            }

            function renderEditSheet() {
                if (!state.focusDetail) {
                    elements.editSheetBody.innerHTML = `<p class="small">Load a task detail before editing.</p>`;
                    return;
                }
                elements.editSheetBody.innerHTML = renderRecordForm("edit", {
                    recordId: state.focusDetail.record_id,
                    head: state.focusDetail.head || "",
                    body: state.focusDetail.body || "",
                    quantity: state.focusDetail.quantity || 0,
                    taskType: state.focusDetail.task_type || "",
                    categories: Array.isArray(state.focusDetail.categories) ? state.focusDetail.categories : [],
                    startAt: state.focusDetail.start_at || null,
                    endAt: state.focusDetail.end_at || null,
                    estimateSeconds: state.focusDetail.estimate_seconds ?? null,
                    assigneeIds: Array.isArray(state.focusDetail.assignees) ? state.focusDetail.assignees.map((entry) => Number(entry.id)).filter(Number.isInteger) : [],
                    parentId: Number(state.focusDetail.parent?.id || 0) || null,
                    parentHead: state.focusDetail.parent?.head || "",
                });
                renderParentChoices(elements.editSheetBody);
            }

            async function syncActiveSheet(activeSheet) {
                const nextSheet = String(activeSheet || "");
                if (state.activeSheet === nextSheet) {
                    return;
                }

                state.activeSheet = nextSheet;

                if (nextSheet === "view") {
                    if (state.formOptions) {
                        renderViewSheet();
                        void ensureFormOptionsLoaded();
                        return;
                    }
                    elements.viewSheetBody.innerHTML = `<p class="small">Loading view options...</p>`;
                    await ensureFormOptionsLoaded();
                    if (state.activeSheet === nextSheet) {
                        renderViewSheet();
                    }
                    return;
                }

                if (nextSheet === "filter") {
                    if (state.formOptions) {
                        renderFilterSheet();
                        void ensureFormOptionsLoaded();
                        return;
                    }
                    elements.filterSheetBody.innerHTML = `<p class="small">Loading filter options...</p>`;
                    await ensureFormOptionsLoaded();
                    if (state.activeSheet === nextSheet) {
                        renderFilterSheet();
                    }
                    return;
                }

                if (nextSheet === "create") {
                    elements.createSheetBody.innerHTML = `<p class="small">Loading task form...</p>`;
                    await ensureFormOptionsLoaded(true);
                    if (state.activeSheet === nextSheet) {
                        renderCreateSheet();
                    }
                    return;
                }

                if (nextSheet === "edit") {
                    if (!state.focusDetail) {
                        elements.editSheetBody.innerHTML = `<p class="small">Load a task detail before editing.</p>`;
                        return;
                    }
                    elements.editSheetBody.innerHTML = `<p class="small">Loading task form...</p>`;
                    await ensureFormOptionsLoaded(true);
                    if (state.activeSheet === nextSheet) {
                        renderEditSheet();
                    }
                    return;
                }

                elements.viewSheetBody.innerHTML = "";
                elements.filterSheetBody.innerHTML = "";
                elements.createSheetBody.innerHTML = "";
                elements.editSheetBody.innerHTML = "";
            }

            async function openViewSheet() {
                await syncActiveSheet("view");
            }

            async function openFilterSheet() {
                await syncActiveSheet("filter");
            }

            async function openCreateSheet() {
                await syncActiveSheet("create");
            }

            async function openEditSheet(recordId) {
                if (Number.isInteger(Number(recordId)) && Number(recordId) > 0) {
                    await loadRecordDetail(Number(recordId));
                } else if (!state.focusDetail && state.ui.focusedRecordId) {
                    await loadRecordDetail(state.ui.focusedRecordId);
                }
                closeFocusAction();
                closeFocusSheet();
                await syncActiveSheet("edit");
                dispatchUiEvent("kanban-open-edit");
            }

            function hydrateFocusMarkdown() {
                const raw = elements.focusCard.querySelector("[data-focus-body-raw]");
                const preview = elements.focusCard.querySelector("[data-focus-body-preview]");
                if (!raw || !preview) {
                    return;
                }
                preview.innerHTML = renderMarkdown(raw.textContent || "");
            }

            async function submitRecordForm(mode) {
                const root =
                    mode === "edit" ? elements.editSheetBody : elements.createSheetBody;
                const draft = readRecordForm(root);
                const payload =
                    mode === "edit"
                        ? {
                              recordId: Number(state.focusDetail?.record_id || 0),
                              head: draft.head,
                              body: draft.body,
                              quantity: draft.quantity,
                              taskType: draft.taskType || null,
                              categories: draft.categories,
                              startAt: draft.startAt,
                              endAt: draft.endAt,
                              estimateSeconds:
                                  Number.isInteger(draft.estimateSeconds) &&
                                  draft.estimateSeconds >= 0
                                      ? draft.estimateSeconds
                                      : null,
                              assigneeIds: draft.assigneeIds,
                              parentId:
                                  Number.isInteger(draft.parentId) && draft.parentId > 0
                                      ? draft.parentId
                                      : null,
                          }
                        : {
                              record: {
                                  head: draft.head,
                                  body: draft.body,
                                  quantity: draft.quantity,
                              },
                              taskType: draft.taskType || null,
                              categories: draft.categories,
                              startAt: draft.startAt,
                              endAt: draft.endAt,
                              estimateSeconds:
                                  Number.isInteger(draft.estimateSeconds) &&
                                  draft.estimateSeconds >= 0
                                      ? draft.estimateSeconds
                                      : null,
                              assigneeIds: draft.assigneeIds,
                              parentId:
                                  Number.isInteger(draft.parentId) && draft.parentId > 0
                                      ? draft.parentId
                                      : null,
                          };
                const outcome = await postAction(
                    mode === "edit" ? "update-record" : "create-record",
                    payload,
                );
                state.formOptions = null;
                if (mode === "edit") {
                    closeSheet("edit");
                } else {
                    closeSheet("create");
                }
                if (outcome?.record_id) {
                    persistUi({
                        ...state.ui,
                        focusedRecordId: Number(outcome.record_id),
                    });
                }
                await refreshRuntime(true);
                if (mode === "edit") {
                    if (Number(state.focusDetail?.record_id || 0) > 0) {
                        await loadRecordDetail(Number(state.focusDetail.record_id));
                    } else if (state.ui.focusedRecordId) {
                        await loadRecordDetail(state.ui.focusedRecordId);
                    }
                } else if (
                    Number(outcome?.record_id || 0) > 0 &&
                    String(draft.taskType || "").trim()
                ) {
                    await loadRecordDetail(Number(outcome.record_id));
                }
            }

            function findComment(commentId) {
                return Array.isArray(state.focusDetail?.comments)
                    ? state.focusDetail.comments.find(
                          (entry) => Number(entry?.id || 0) === Number(commentId),
                      ) || null
                    : null;
            }

            function findResource(resourceRefId) {
                return Array.isArray(state.focusDetail?.resources)
                    ? state.focusDetail.resources.find(
                          (entry) => Number(entry?.id || 0) === Number(resourceRefId),
                      ) || null
                    : null;
            }

            function closeFocusAction() {
                state.focusAction = null;
                if (elements.focusActionPanel) {
                    elements.focusActionPanel.hidden = true;
                    elements.focusActionPanel.innerHTML = "";
                }
            }

            function focusActionInput(name) {
                return elements.focusActionPanel?.querySelector(`[data-focus-action-field="${name}"]`);
            }

            function quantityLabel(quantity) {
                const lane = columns.find((entry) => Number(entry.value) === Number(quantity));
                return lane ? lane.label : `Quantity ${Number(quantity) || 0}`;
            }

            function compactBodyPreview(body) {
                const trimmed = String(body || "").trim();
                if (!trimmed) {
                    return "";
                }

                const excerpt = [];
                let length = 0;
                for (const line of trimmed.split("\n").map((line) => line.trimEnd())) {
                    excerpt.push(line);
                    length += line.length;
                    if (excerpt.length >= 4 || length >= 220) {
                        break;
                    }
                }

                const compact = excerpt.join("\n").trim();
                if (compact.length < trimmed.length) {
                    return `${compact}\n...`;
                }
                return compact;
            }

            function readCardPreview(recordId) {
                const card = elements.columns.querySelector(
                    `.card[data-record-id="${CSS.escape(String(recordId))}"]`,
                );
                if (!card) {
                    return null;
                }
                return {
                    recordId: Number(recordId),
                    head: String(card.dataset.head || ""),
                    body: String(card.dataset.body || ""),
                    quantity: Number(card.dataset.quantity || 0),
                };
            }

            function renderFocusPreviewHtml(preview) {
                const head = String(preview?.head || "Untitled");
                const body = String(preview?.body || "");
                const quantity = quantityLabel(preview?.quantity);
                const bodyHtml = body.trim()
                    ? `<section class="kanban-focus-card__body-wrap">
                            <div class="sheetHeader">
                                <div class="headerMeta">
                                    <h3>Body</h3>
                                </div>
                                <div class="headerActions">
                                    <span class="small">Loading more detail...</span>
                                </div>
                            </div>
                            <article
                                class="kanban-focus-card__body-preview markdownRender"
                                data-record-id="${escapeHtml(String(preview?.recordId || 0))}"
                                data-markdown-source="${escapeHtml(body)}"
                            >${renderMarkdown(body)}</article>
                        </section>`
                    : `<p class="small">No body available yet.</p>`;
                return `
                    <section class="kanban-focus-card kanban-focus-card--preview">
                        <header class="kanban-focus-card__header">
                            <h2 class="kanban-focus-card__title">${escapeHtml(head)}</h2>
                            <div class="kanban-focus-card__meta">
                                <span>Quantity: ${escapeHtml(quantity)}</span>
                                <span>Record #${escapeHtml(String(preview?.recordId || 0))}</span>
                            </div>
                        </header>
                        ${bodyHtml}
                    </section>
                `;
            }

            function markdownTaskToggleRoot(node) {
                return node?.closest(".card, .kanban-focus-card, #kanban-focus-card") || null;
            }

            function markdownTaskRecordId(node) {
                const recordNode = node?.closest("[data-record-id]");
                const recordId = Number(recordNode?.dataset?.recordId || 0);
                return Number.isInteger(recordId) && recordId > 0
                    ? recordId
                    : Number(state.focusDetail?.record_id || 0) > 0
                      ? Number(state.focusDetail.record_id)
                      : Number(state.focusPreview?.recordId || 0) > 0
                        ? Number(state.focusPreview.recordId)
                        : null;
            }

            function syncMarkdownTaskSources(root, nextSource) {
                if (!root) {
                    return [];
                }
                if (Object.prototype.hasOwnProperty.call(root.dataset || {}, "body")) {
                    root.dataset.body = nextSource;
                }
                const compactSource = Object.prototype.hasOwnProperty.call(root.dataset || {}, "body")
                    ? compactBodyPreview(nextSource)
                    : nextSource;
                const blocks = Array.from(root.querySelectorAll("[data-markdown-source]"));
                for (const block of blocks) {
                    block.dataset.markdownSource = block.classList.contains("is-full")
                        ? nextSource
                        : compactSource;
                    block.dataset.markdownRendered = "";
                }
                const rawBody = root.querySelector("[data-focus-body-raw]");
                if (rawBody) {
                    rawBody.textContent = nextSource;
                }
                hydrateMarkdownBlocks(root);
                return blocks;
            }

            async function persistMarkdownTaskToggle(checkbox) {
                const root = markdownTaskToggleRoot(checkbox);
                if (!root) {
                    return;
                }
                const recordId = markdownTaskRecordId(checkbox);
                if (!recordId) {
                    return;
                }

                const sourceBlock = checkbox.closest("[data-markdown-source]");
                const previousSource = root.dataset.body
                    ? String(root.dataset.body || "")
                    : String(sourceBlock?.dataset.markdownSource || "");
                if (!previousSource) {
                    return;
                }

                const nextSource = toggleMarkdownTaskLine(
                    previousSource,
                    checkbox.dataset.markdownTaskLine,
                    checkbox.checked,
                );
                if (nextSource === previousSource) {
                    return;
                }

                const previousRawBody = root.querySelector("[data-focus-body-raw]")?.textContent || null;

                state.focusDetailGeneration += 1;
                syncMarkdownTaskSources(root, nextSource);
                if (state.focusDetail && Number(state.focusDetail.record_id || 0) === recordId) {
                    state.focusDetail.body = nextSource;
                }
                if (state.focusPreview && Number(state.focusPreview.recordId || 0) === recordId) {
                    state.focusPreview.body = nextSource;
                }

                try {
                    await postAction("update-record-body", {
                        recordId,
                        body: nextSource,
                    });
                    if (
                        isSheetVisible(elements.focusSheet) &&
                        (
                            Number(state.focusDetail?.record_id || 0) === recordId ||
                            Number(state.focusPreview?.recordId || 0) === recordId ||
                            Number(state.ui.focusedRecordId || 0) === recordId
                        )
                    ) {
                        await loadRecordDetail(recordId).catch((reloadError) => {
                            state.transportError =
                                reloadError instanceof Error
                                    ? reloadError.message
                                    : String(reloadError);
                            updateStatus();
                        });
                    }
                } catch (error) {
                    syncMarkdownTaskSources(root, previousSource);
                    const rawBody = root.querySelector("[data-focus-body-raw]");
                    if (rawBody && previousRawBody !== null) {
                        rawBody.textContent = previousRawBody;
                    }
                    if (state.focusDetail && Number(state.focusDetail.record_id || 0) === recordId) {
                        state.focusDetail.body = previousSource;
                    }
                    if (state.focusPreview && Number(state.focusPreview.recordId || 0) === recordId) {
                        state.focusPreview.body = previousSource;
                    }
                    state.transportError =
                        error instanceof Error ? error.message : String(error);
                    updateStatus();
                    if (
                        isSheetVisible(elements.focusSheet) &&
                        (
                            Number(state.focusDetail?.record_id || 0) === recordId ||
                            Number(state.focusPreview?.recordId || 0) === recordId ||
                            Number(state.ui.focusedRecordId || 0) === recordId
                        )
                    ) {
                        await loadRecordDetail(recordId).catch((reloadError) => {
                            state.transportError =
                                reloadError instanceof Error
                                    ? reloadError.message
                                    : String(reloadError);
                            updateStatus();
                        });
                    }
                }
            }

            function renderFocusActionPanel(action) {
                if (!elements.focusActionPanel) {
                    return;
                }

                const current = action || {};
                const kind = String(current.kind || "");
                const header = String(current.header || "Action");
                const description = String(current.description || "");
                const message = String(current.message || "");
                const body = String(current.body || "");
                const resourcePath = String(current.resourcePath || "");
                const resourceKind = String(current.resourceKind || "image").trim() || "image";
                const title = String(current.title || "");
                const note = String(current.note || "");

                let formHtml = "";
                if (kind === "create-comment" || kind === "edit-comment") {
                    formHtml = `
                        <form data-focus-action-form="">
                            <div class="fieldBlock">
                                <label class="fieldLabel" for="focus-action-body">Comment body</label>
                                <textarea class="textarea" id="focus-action-body" data-focus-action-field="body" spellcheck="true" placeholder="Write a comment">${escapeHtml(body)}</textarea>
                            </div>
                            <div class="sheetActions">
                                <button class="toolbarBtn" type="button" data-focus-action-cancel="">Cancel</button>
                                <button class="toolbarBtn toolbarBtn--accent" type="button" data-focus-action-submit="">${kind === "create-comment" ? "Add comment" : "Save comment"}</button>
                            </div>
                        </form>
                    `;
                } else if (kind === "create-resource-ref") {
                    formHtml = `
                        <form data-focus-action-form="">
                            <div class="formGrid">
                                <div class="fieldBlock">
                                    <label class="fieldLabel" for="focus-action-resource-path">Resource path</label>
                                    <input class="field" id="focus-action-resource-path" data-focus-action-field="resourcePath" type="text" placeholder="bucket/path/to/file.png" value="${escapeHtml(resourcePath)}">
                                </div>
                                <div class="fieldBlock">
                                    <label class="fieldLabel" for="focus-action-resource-kind">Resource kind</label>
                                    <input class="field" id="focus-action-resource-kind" data-focus-action-field="resourceKind" type="text" placeholder="image" value="${escapeHtml(resourceKind)}">
                                </div>
                                <div class="fieldBlock">
                                    <label class="fieldLabel" for="focus-action-resource-title">Title</label>
                                    <input class="field" id="focus-action-resource-title" data-focus-action-field="title" type="text" placeholder="Optional title" value="${escapeHtml(title)}">
                                </div>
                            </div>
                            <div class="sheetActions">
                                <button class="toolbarBtn" type="button" data-focus-action-cancel="">Cancel</button>
                                <button class="toolbarBtn toolbarBtn--accent" type="button" data-focus-action-submit="">Link resource</button>
                            </div>
                        </form>
                    `;
                } else if (kind === "start-worklog") {
                    formHtml = `
                        <form data-focus-action-form="">
                            <div class="fieldBlock">
                                <label class="fieldLabel" for="focus-action-note">Worklog note</label>
                                <textarea class="textarea" id="focus-action-note" data-focus-action-field="note" spellcheck="true" placeholder="Optional note">${escapeHtml(note)}</textarea>
                            </div>
                            <div class="sheetActions">
                                <button class="toolbarBtn" type="button" data-focus-action-cancel="">Cancel</button>
                                <button class="toolbarBtn toolbarBtn--accent" type="button" data-focus-action-submit="">Start</button>
                            </div>
                        </form>
                    `;
                } else if (
                    kind === "delete-comment" ||
                    kind === "delete-resource-ref" ||
                    kind === "delete-record"
                ) {
                    formHtml = `
                        <form data-focus-action-form="">
                            <p data-focus-action-body="">${escapeHtml(body)}</p>
                            <div class="sheetActions">
                                <button class="toolbarBtn" type="button" data-focus-action-cancel="">Cancel</button>
                                <button class="toolbarBtn toolbarBtn--danger" type="button" data-focus-action-submit="">${kind === "delete-record" ? "Delete record" : "Remove"}</button>
                            </div>
                        </form>
                    `;
                }

                elements.focusActionPanel.hidden = false;
                elements.focusActionPanel.innerHTML = `
                    <div class="sheetHeader">
                        <div class="headerMeta">
                            <div class="headerTitle">${escapeHtml(header)}</div>
                            <div class="headerSub">${escapeHtml(description)}</div>
                        </div>
                        <div class="headerActions">
                            <button class="toolbarBtn" type="button" data-focus-action-close="">Close</button>
                        </div>
                    </div>
                    <p class="small" data-focus-action-message=""${message ? "" : ' style="display: none"'}>${escapeHtml(message)}</p>
                    ${formHtml}
                `;

                const form = elements.focusActionPanel.querySelector("[data-focus-action-form]");
                if (form) {
                    form.addEventListener("submit", async (event) => {
                        event.preventDefault();
                        await submitFocusAction();
                    });
                }
            }

            function setFocusActionMessage(message) {
                const nextMessage = String(message || "");
                if (state.focusAction) {
                    state.focusAction.message = nextMessage;
                }
                const target = elements.focusActionPanel?.querySelector("[data-focus-action-message]");
                if (target) {
                    target.textContent = nextMessage;
                    target.style.display = nextMessage ? "" : "none";
                }
            }

            function openFocusAction(actionOrKind, options = {}) {
                const baseAction =
                    typeof actionOrKind === "string"
                        ? { kind: actionOrKind, ...options }
                        : { ...(actionOrKind || {}) };
                const kind = String(baseAction.kind || "");
                const recordId = Number(baseAction.recordId || state.focusDetail?.record_id || 0);
                const recordLabel = state.focusDetail?.head?.trim()
                    ? state.focusDetail.head.trim()
                    : `Record #${recordId}`;
                const nextAction = {
                    kind,
                    recordId,
                    commentId: Number(baseAction.commentId || 0) || 0,
                    resourceRefId: Number(baseAction.resourceRefId || 0) || 0,
                    header: String(baseAction.header || ""),
                    description: String(baseAction.description || ""),
                    body: String(baseAction.body || ""),
                    resourcePath: String(baseAction.resourcePath || ""),
                    resourceKind: String(baseAction.resourceKind || "image").trim() || "image",
                    title: String(baseAction.title || ""),
                    note: String(baseAction.note || ""),
                };

                if (kind === "create-comment") {
                    nextAction.header = "Add comment";
                    nextAction.description = recordLabel;
                    nextAction.body = "";
                } else if (kind === "edit-comment") {
                    const comment = nextAction.commentId ? findComment(nextAction.commentId) : null;
                    nextAction.header = "Edit comment";
                    nextAction.description = recordLabel;
                    nextAction.body = String(baseAction.body || comment?.body || "");
                } else if (kind === "create-resource-ref") {
                    nextAction.header = "Link resource";
                    nextAction.description = recordLabel;
                    nextAction.resourceKind = String(baseAction.resourceKind || "image").trim() || "image";
                } else if (kind === "start-worklog") {
                    nextAction.header = "Start worklog";
                    nextAction.description = recordLabel;
                } else if (kind === "delete-comment") {
                    const comment = nextAction.commentId ? findComment(nextAction.commentId) : null;
                    nextAction.header = "Confirm removal";
                    nextAction.description = recordLabel;
                    nextAction.body = String(comment?.body || "This comment will be removed.");
                } else if (kind === "delete-resource-ref") {
                    const resource = nextAction.resourceRefId ? findResource(nextAction.resourceRefId) : null;
                    nextAction.header = "Confirm removal";
                    nextAction.description = recordLabel;
                    nextAction.body = String(
                        resource?.title ||
                        resource?.resource_path ||
                        "This resource link will be removed.",
                    );
                } else if (kind === "delete-record") {
                    nextAction.header = "Delete record";
                    nextAction.description = recordLabel;
                    nextAction.body = `Delete ${recordLabel}?`;
                }

                state.focusAction = nextAction;
                renderFocusActionPanel(nextAction);
                window.requestAnimationFrame(() => {
                    elements.focusActionPanel?.scrollIntoView({
                        block: "nearest",
                        behavior: "smooth",
                    });
                    const firstField =
                        kind === "create-comment" || kind === "edit-comment"
                            ? focusActionInput("body")
                            : kind === "create-resource-ref"
                              ? focusActionInput("resourcePath")
                              : kind === "start-worklog"
                                ? focusActionInput("note")
                                : null;
                    firstField?.focus();
                });
            }

            async function submitFocusAction() {
                const action = state.focusAction;
                if (!action) {
                    return;
                }

                const recordId = Number(action.recordId || state.focusDetail?.record_id || 0);

                try {
                    if (action.kind === "create-comment") {
                        const body = String(focusActionInput("body")?.value || "").trim();
                        if (!body) {
                            setFocusActionMessage("Comentario vazio nao e valido.");
                            focusActionInput("body")?.focus();
                            return;
                        }
                        await postAction("create-comment", {
                            recordId,
                            body,
                        });
                        closeFocusAction();
                        await loadRecordDetail(recordId);
                        return;
                    }

                    if (action.kind === "edit-comment") {
                        const body = String(focusActionInput("body")?.value || "").trim();
                        if (!body) {
                            setFocusActionMessage("Comentario vazio nao e valido.");
                            focusActionInput("body")?.focus();
                            return;
                        }
                        await postAction("update-comment", {
                            commentId: Number(action.commentId || 0),
                            body,
                        });
                        closeFocusAction();
                        if (state.focusDetail?.record_id) {
                            await loadRecordDetail(Number(state.focusDetail.record_id));
                        }
                        return;
                    }

                    if (action.kind === "create-resource-ref") {
                        const resourcePath = String(focusActionInput("resourcePath")?.value || "").trim();
                        if (!resourcePath) {
                            setFocusActionMessage("resource_path vazio nao e valido.");
                            focusActionInput("resourcePath")?.focus();
                            return;
                        }
                        const resourceKind = String(focusActionInput("resourceKind")?.value || "image").trim() || "image";
                        const title = String(focusActionInput("title")?.value || "").trim();
                        await postAction("create-resource-ref", {
                            recordId,
                            provider: "bucket",
                            resourceKind,
                            resourcePath,
                            title: title || null,
                            position: Array.isArray(state.focusDetail?.resources)
                                ? state.focusDetail.resources.length
                                : null,
                        });
                        closeFocusAction();
                        if (state.focusDetail?.record_id) {
                            await loadRecordDetail(Number(state.focusDetail.record_id));
                        }
                        return;
                    }

                    if (action.kind === "start-worklog") {
                        const note = String(focusActionInput("note")?.value || "").trim();
                        const outcome = await postAction("start-worklog", {
                            recordId,
                            note: note || null,
                        });
                        const intervalId = Number(outcome?.detail?.interval?.id || 0);
                        if (intervalId > 0) {
                            upsertActiveWorklogInterval(recordId, intervalId);
                        }
                        closeFocusAction();
                        await refreshRuntime(true);
                        await loadRecordDetail(recordId);
                        return;
                    }

                    if (action.kind === "delete-comment") {
                        await postAction("delete-comment", {
                            commentId: Number(action.commentId || 0),
                        });
                        closeFocusAction();
                        if (state.focusDetail?.record_id) {
                            await loadRecordDetail(Number(state.focusDetail.record_id));
                        }
                        return;
                    }

                    if (action.kind === "delete-resource-ref") {
                        await postAction("delete-resource-ref", {
                            resourceRefId: Number(action.resourceRefId || 0),
                        });
                        closeFocusAction();
                        if (state.focusDetail?.record_id) {
                            await loadRecordDetail(Number(state.focusDetail.record_id));
                        }
                        return;
                    }

                    if (action.kind === "delete-record") {
                        await postAction("delete-record", { recordId });
                        closeFocusAction();
                        closeFocus();
                        state.focusDetail = null;
                        await refreshRuntime(true);
                    }
                } catch (error) {
                    setFocusActionMessage(error instanceof Error ? error.message : String(error));
                }
            }

            async function deleteRecordFromUi(recordId) {
                openFocusSheet();
                openFocusAction({
                    kind: "delete-record",
                    recordId: Number(recordId),
                });
            }

            async function createComment(recordId) {
                openFocusAction({
                    kind: "create-comment",
                    recordId: Number(recordId),
                });
            }

            async function editComment(commentId) {
                openFocusAction({
                    kind: "edit-comment",
                    recordId: Number(state.focusDetail?.record_id || 0),
                    commentId: Number(commentId),
                });
            }

            async function deleteComment(commentId) {
                openFocusAction({
                    kind: "delete-comment",
                    recordId: Number(state.focusDetail?.record_id || 0),
                    commentId: Number(commentId),
                });
            }

            async function createResourceRef(recordId) {
                openFocusAction({
                    kind: "create-resource-ref",
                    recordId: Number(recordId),
                });
            }

            async function deleteResourceRef(resourceRefId) {
                openFocusAction({
                    kind: "delete-resource-ref",
                    recordId: Number(state.focusDetail?.record_id || 0),
                    resourceRefId: Number(resourceRefId),
                });
            }

            async function startWorklog(recordId) {
                openFocusAction({
                    kind: "start-worklog",
                    recordId: Number(recordId),
                });
            }

            function queuePendingStop(recordId, intervalId, endedAt) {
                state.pendingWorklogStops = normalizePendingStops([
                    ...state.pendingWorklogStops,
                    { recordId, intervalId, endedAt },
                ]);
                persistPendingStops();
                removeActiveWorklogInterval(recordId, intervalId);
            }

            function looksOffline(error) {
                const text = String(error instanceof Error ? error.message : error || "");
                return (
                    navigator.onLine === false ||
                    text.includes("Failed to fetch") ||
                    text.includes("NetworkError") ||
                    text.includes("Load failed")
                );
            }

            async function stopWorklog(recordId, intervalId) {
                const endedAt = new Date().toISOString().replace(/\.\d{3}Z$/, "Z");
                try {
                    await postAction("stop-worklog", {
                        recordId: Number(recordId),
                        intervalId: Number(intervalId),
                        endedAt,
                    });
                    removeActiveWorklogInterval(recordId, intervalId);
                    state.pendingWorklogStops = state.pendingWorklogStops.filter(
                        (entry) =>
                            Number(entry.intervalId) !== Number(intervalId),
                    );
                    persistPendingStops();
                    await refreshRuntime(true);
                    await loadRecordDetail(Number(recordId));
                } catch (error) {
                    if (!looksOffline(error)) {
                        throw error;
                    }
                    queuePendingStop(Number(recordId), Number(intervalId), endedAt);
                    if (Number(state.focusDetail?.record_id || 0) === Number(recordId)) {
                        closeFocus();
                    }
                }
            }

            function schedulePersistUi(normalized, nextJson) {
                if (state.persistTimer) {
                    window.clearTimeout(state.persistTimer);
                }

                state.persistTimer = window.setTimeout(() => {
                    state.persistTimer = null;
                    window.LinceWidgetHost?.patchCardState?.({ ui: normalized });
                }, 140);
            }

            function syncUiSignals(nextUi) {
                const normalized = normalizeUi(nextUi);
                state.ui = normalized;
                patchSignals({ ui: normalized });
            }

            function syncUiFromSignals() {
                const nextUi = readSignalPath("ui");
                if (nextUi !== null) {
                    state.ui = normalizeUi(nextUi);
                    return;
                }
                void datastarReady.then(() => {
                    const resolved = readSignalPath("ui");
                    if (resolved !== null) {
                        state.ui = normalizeUi(resolved);
                    }
                });
            }

            function syncSheetFromSignals(patch) {
                if (!patch || typeof patch !== "object") {
                    return;
                }

                if (!Object.prototype.hasOwnProperty.call(patch, "activeSheet")) {
                    return;
                }

                void syncActiveSheet(patch.activeSheet);
            }

            function persistUiFromSignals() {
                const rawUi = readSignalPath("ui");
                if (rawUi === null) {
                    return;
                }
                const normalized = normalizeUi(rawUi);
                const nextJson = serializeUi(normalized);
                state.ui = normalized;

                if (nextJson === state.lastPersistedUiJson) {
                    return;
                }

                state.lastPersistedUiJson = nextJson;
                persistPreviewUi(normalized);
                schedulePersistUi(normalized, nextJson);
            }

            function persistUi(nextUi) {
                const normalized = normalizeUi(nextUi);
                const nextJson = serializeUi(normalized);
                state.ui = normalized;
                syncUiSignals(normalized);

                if (nextJson === state.lastPersistedUiJson) {
                    return;
                }

                state.lastPersistedUiJson = nextJson;
                persistPreviewUi(normalized);
                schedulePersistUi(normalized, nextJson);
            }

            function normalizePendingStops(rawStops) {
                if (!Array.isArray(rawStops)) {
                    return [];
                }
                const normalized = [];
                for (const entry of rawStops) {
                    const recordId = Number(entry?.recordId);
                    const intervalId = Number(entry?.intervalId);
                    const endedAt = String(entry?.endedAt || "").trim();
                    if (!Number.isInteger(recordId) || !Number.isInteger(intervalId) || !endedAt) {
                        continue;
                    }
                    normalized.push({ recordId, intervalId, endedAt });
                }
                return normalized;
            }

            function normalizeActiveWorklogIntervals(rawIntervals) {
                if (!Array.isArray(rawIntervals)) {
                    return [];
                }
                const seen = new Set();
                const normalized = [];
                for (const entry of rawIntervals) {
                    const recordId = Number(entry?.recordId);
                    const intervalId = Number(entry?.intervalId);
                    if (!Number.isInteger(recordId) || !Number.isInteger(intervalId)) {
                        continue;
                    }
                    const key = `${recordId}:${intervalId}`;
                    if (seen.has(key)) {
                        continue;
                    }
                    seen.add(key);
                    normalized.push({ recordId, intervalId });
                }
                return normalized;
            }

            function persistPendingStops() {
                window.LinceWidgetHost?.patchCardState?.({
                    pendingWorklogStops: state.pendingWorklogStops,
                });
            }

            function persistActiveWorklogIntervals() {
                window.LinceWidgetHost?.patchCardState?.({
                    activeWorklogIntervals: state.activeWorklogIntervals,
                });
            }

            function setActiveWorklogIntervals(nextIntervals) {
                state.activeWorklogIntervals = normalizeActiveWorklogIntervals(nextIntervals);
                persistActiveWorklogIntervals();
                syncHeartbeatLoop();
            }

            function upsertActiveWorklogInterval(recordId, intervalId) {
                const next = normalizeActiveWorklogIntervals([
                    ...state.activeWorklogIntervals,
                    { recordId, intervalId },
                ]);
                state.activeWorklogIntervals = next;
                persistActiveWorklogIntervals();
                syncHeartbeatLoop();
            }

            function removeActiveWorklogInterval(recordId, intervalId) {
                const hasIntervalId =
                    intervalId !== null &&
                    intervalId !== undefined &&
                    Number.isInteger(Number(intervalId)) &&
                    Number(intervalId) > 0;
                const hasRecordId =
                    recordId !== null &&
                    recordId !== undefined &&
                    Number.isInteger(Number(recordId)) &&
                    Number(recordId) > 0;
                state.activeWorklogIntervals = state.activeWorklogIntervals.filter((entry) => {
                    if (hasIntervalId) {
                        return Number(entry.intervalId) !== Number(intervalId);
                    }
                    if (hasRecordId) {
                        return Number(entry.recordId) !== Number(recordId);
                    }
                    return true;
                });
                persistActiveWorklogIntervals();
                syncHeartbeatLoop();
            }

            function stopHeartbeatLoop() {
                if (state.heartbeatTimer) {
                    window.clearInterval(state.heartbeatTimer);
                    state.heartbeatTimer = null;
                }
            }

            function startHeartbeatLoop() {
                stopHeartbeatLoop();
                if (!state.activeWorklogIntervals.length) {
                    return;
                }
                state.heartbeatTimer = window.setInterval(() => {
                    for (const interval of state.activeWorklogIntervals) {
                        postAction("heartbeat-worklog", {
                            intervalId: Number(interval.intervalId),
                            recordId: Number(interval.recordId),
                        }).catch(() => {});
                    }
                }, 5 * 60 * 1000);
            }

            function syncHeartbeatLoop() {
                if (state.activeWorklogIntervals.length) {
                    startHeartbeatLoop();
                } else {
                    stopHeartbeatLoop();
                }
            }

            function syncHeartbeatFromDetail() {
                const intervalId = Number(
                    state.focusDetail?.worklog?.current_user_open_interval_id || 0,
                );
                const recordId = Number(state.focusDetail?.record_id || 0);
                if (
                    Number.isInteger(intervalId) &&
                    intervalId > 0 &&
                    Number.isInteger(recordId) &&
                    recordId > 0
                ) {
                    upsertActiveWorklogInterval(recordId, intervalId);
                } else if (Number.isInteger(recordId) && recordId > 0) {
                    removeActiveWorklogInterval(recordId, null);
                }
            }

            async function flushPendingWorklogStops() {
                if (!state.pendingWorklogStops.length) {
                    return;
                }
                const remaining = [];
                let changed = false;
                for (const pending of state.pendingWorklogStops) {
                    try {
                        await postAction("stop-worklog", {
                            recordId: pending.recordId,
                            intervalId: pending.intervalId,
                            endedAt: pending.endedAt,
                        });
                        removeActiveWorklogInterval(pending.recordId, pending.intervalId);
                        changed = true;
                    } catch {
                        remaining.push(pending);
                    }
                }
                if (remaining.length !== state.pendingWorklogStops.length) {
                    state.pendingWorklogStops = remaining;
                    persistPendingStops();
                }
                if (changed && state.ui.focusedRecordId) {
                    loadRecordDetail(state.ui.focusedRecordId).catch(() => {});
                }
            }

            function removeEmptyPlaceholder(list) {
                const empty = list.querySelector(".empty");
                if (empty) {
                    empty.remove();
                }
            }

            function ensureEmptyPlaceholder(list) {
                if (list.querySelector(".card")) {
                    return;
                }
                if (list.querySelector(".empty")) {
                    return;
                }
                const empty = document.createElement("div");
                empty.className = "empty";
                empty.textContent = "Drop records here";
                list.appendChild(empty);
            }

            function updateLaneCounts() {
                for (const column of elements.columns.querySelectorAll(".col")) {
                    const count = column.querySelectorAll(".card").length;
                    const badge = column.querySelector(".count");
                    if (badge) {
                        badge.textContent = String(count);
                    }
                }
            }

            function optimisticMoveCard(recordId, targetLaneKey) {
                const card = elements.columns.querySelector(
                    `.card[data-record-id="${CSS.escape(String(recordId))}"]`,
                );
                const targetList = elements.columns.querySelector(
                    `.col[data-col="${CSS.escape(String(targetLaneKey))}"] .list`,
                );
                if (!card || !targetList) {
                    return null;
                }
                const previousHtml = elements.columns.innerHTML;
                const sourceList = card.closest(".list");
                removeEmptyPlaceholder(targetList);
                targetList.appendChild(card);
                card.dataset.quantity = String(laneToQuantity(targetLaneKey));
                 card.classList.remove("backlog", "next", "wip", "review", "done");
                card.classList.add(targetLaneKey);
                updateLaneCounts();
                ensureEmptyPlaceholder(sourceList);
                return previousHtml;
            }

            function rollbackBoard(previousHtml) {
                if (typeof previousHtml !== "string") {
                    return;
                }
                elements.columns.innerHTML = previousHtml;
                hydrateMarkdownBlocks(elements.columns);
            }

            function handleKanbanSync(payload) {
                state.connected = true;
                state.loadingStream = false;
                state.transportError = "";
                state.reconnectAttempt = 0;
                state.lastUpdate = new Date().toLocaleTimeString();
                state.viewMeta = payload?.view || null;
                if (state.updatesPaused) {
                    updateStatus();
                    return;
                }
                patchHtml(elements.toolbarState, payload?.html?.toolbar_state);
                patchHtml(elements.columns, payload?.html?.columns);
                patchHtml(elements.emptyOrError, payload?.html?.empty_or_error);
                hydrateMarkdownBlocks(elements.columns);
                updateStatus();
                if (state.ui.focusedRecordId && isSheetVisible(elements.focusSheet)) {
                    loadRecordDetail(state.ui.focusedRecordId).catch(() => {
                        closeFocus();
                    });
                }
            }

            function handleKanbanError(payload) {
                state.connected = false;
                state.loadingStream = false;
                state.transportError =
                    payload?.message || "The backend stream reported an error.";
                patchHtml(elements.emptyOrError, payload?.html?.empty_or_error);
                updateStatus();
            }

            async function consumeSseResponse(response, generation, signal) {
                const reader = response.body.getReader();
                const decoder = new TextDecoder();
                let buffer = "";

                while (true) {
                    const { value, done } = await reader.read();
                    if (done || signal.aborted || generation !== state.streamGeneration) {
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

                        let payload = null;
                        try {
                            payload = JSON.parse(event.data);
                        } catch {
                            payload = null;
                        }

                        if (event.event === "kanban-sync" && payload) {
                            handleKanbanSync(payload);
                        } else if (event.event === "kanban-error" && payload) {
                            handleKanbanError(payload);
                        }
                    }
                }
            }

            async function connectStream(reset) {
                stopStream();
                if (!state.contract || !streamEnabled()) {
                    updateStatus();
                    return;
                }
                if (
                    state.contract.source?.requires_auth &&
                    state.contract.source?.authenticated === false
                ) {
                    updateStatus();
                    return;
                }

                if (reset) {
                    state.loadingStream = true;
                    state.transportError = "";
                    updateStatus();
                }

                const generation = ++state.streamGeneration;
                const controller = new AbortController();
                state.streamController = controller;

                try {
                    const response = await fetch(streamUrl(), {
                        headers: { Accept: "text/event-stream" },
                        cache: "no-store",
                        signal: controller.signal,
                    });

                    if (controller.signal.aborted || generation !== state.streamGeneration) {
                        return;
                    }

                    if (response.status === 401) {
                        window.LinceWidgetHost?.invalidateServerAuth?.(
                            state.contract?.source?.server_id || state.hostMeta.serverId || "",
                        );
                        state.transportError =
                            "The host login for this server expired. Reconnect the server in the board.";
                        state.loadingStream = false;
                        updateStatus();
                        await fetchContract();
                        return;
                    }

                    if (!response.ok || !response.body) {
                        const raw = await response.text().catch(() => "");
                        throw new Error(raw || `Unable to open the Kanban stream (${response.status}).`);
                    }

                    state.connected = true;
                    state.loadingStream = true;
                    updateStatus();
                    await consumeSseResponse(response, generation, controller.signal);

                    if (controller.signal.aborted || generation !== state.streamGeneration) {
                        return;
                    }

                    state.connected = false;
                    state.loadingStream = false;
                    state.transportError = "The stream ended. Reconnecting...";
                    updateStatus();
                    scheduleReconnect();
                } catch (error) {
                    if (controller.signal.aborted || generation !== state.streamGeneration) {
                        return;
                    }
                    state.connected = false;
                    state.loadingStream = false;
                    state.transportError =
                        error instanceof Error ? error.message : String(error);
                    updateStatus();
                    scheduleReconnect();
                } finally {
                    if (state.streamController === controller) {
                        state.streamController = null;
                    }
                }
            }

            async function refreshRuntime(resetStream) {
                const contractLoaded = await fetchContract();
                updateStatus();
                if (!contractLoaded) {
                    stopStream();
                    return;
                }
                void ensureFormOptionsLoaded();
                await flushPendingWorklogStops();
                if (streamEnabled()) {
                    await connectStream(resetStream !== false);
                } else {
                    stopStream();
                }
            }

            async function postAction(action, payload) {
                const response = await fetch(actionUrl(action), {
                    method: "POST",
                    headers: { "Content-Type": "application/json" },
                    body: JSON.stringify(payload || {}),
                });
                const body = await response.json().catch(() => null);
                if (response.status === 401) {
                    window.LinceWidgetHost?.invalidateServerAuth?.(
                        state.contract?.source?.server_id || state.hostMeta.serverId || "",
                    );
                }
                if (!response.ok) {
                    throw new Error(body?.error || `Action ${action} failed.`);
                }
                return body;
            }

            async function loadRecordDetail(recordId) {
                const generation = ++state.focusDetailGeneration;
                const preview = readCardPreview(recordId);
                state.focusPreview = preview;
                patchHtml(
                    elements.focusCard,
                    preview
                        ? renderFocusPreviewHtml(preview)
                        : `<section class="kanban-focus-card"><p class="small">Loading task detail...</p></section>`,
                );
                closeFocusAction();
                openFocusSheet();
                const payload = await postAction("load-record-detail", {
                    recordId: Number(recordId),
                });
                if (generation !== state.focusDetailGeneration) {
                    return;
                }
                patchHtml(elements.focusCard, payload?.html);
                state.focusDetail = payload?.detail || null;
                state.focusPreview = null;
                hydrateFocusMarkdown();
                syncHeartbeatFromDetail();
                if (isSheetVisible(elements.editSheet)) {
                    renderEditSheet();
                }
                persistUi({
                    ...state.ui,
                    focusedRecordId: Number(recordId),
                });
            }

            function closeFocus() {
                state.focusDetailGeneration += 1;
                closeFocusSheet();
                elements.focusCard.innerHTML = "";
                closeFocusAction();
                closeSheet("edit");
                state.focusDetail = null;
                state.focusPreview = null;
                persistUi({
                    ...state.ui,
                    focusedRecordId: null,
                });
            }

            async function moveRecord(recordId, nextQuantity, targetLaneKey) {
                const rollbackHtml = optimisticMoveCard(recordId, targetLaneKey);
                try {
                    const outcome = await postAction("move-record", {
                        recordId: Number(recordId),
                        quantity: Number(nextQuantity),
                    });
                    state.transportError = "";
                    state.lastUpdate = new Date().toLocaleTimeString();
                    updateStatus();
                    if (outcome?.await_stream_refresh) {
                        return;
                    }
                } catch (error) {
                    rollbackBoard(rollbackHtml);
                    state.transportError =
                        error instanceof Error ? error.message : String(error);
                    updateStatus();
                }
            }

            function toggleWidgetStream() {
                const nextEnabled = !(state.hostMeta.streams.cardEnabled !== false);
                state.hostMeta = {
                    ...state.hostMeta,
                    streams: {
                        ...state.hostMeta.streams,
                        cardEnabled: nextEnabled,
                        enabled:
                            state.hostMeta.streams.globalEnabled !== false &&
                            nextEnabled,
                    },
                };
                window.LinceWidgetHost?.setStreamsEnabled?.(nextEnabled);
                updateStatus();
                if (streamEnabled()) {
                    refreshRuntime(true).catch((error) => {
                        state.transportError =
                            error instanceof Error ? error.message : String(error);
                        updateStatus();
                    });
                } else {
                    stopStream();
                }
            }

            function togglePausedUpdates() {
                state.updatesPaused = !state.updatesPaused;
                updateStatus();
                if (!state.updatesPaused && streamEnabled()) {
                    refreshRuntime(true).catch((error) => {
                        state.transportError =
                            error instanceof Error ? error.message : String(error);
                        updateStatus();
                    });
                }
            }

            function applyHostMeta(rawMeta) {
                const previousEnabled = state.hostMeta.streams.enabled !== false;
                const nextMeta = normalizeHostMeta(rawMeta);
                const nextUi = normalizeUi(nextMeta.cardState?.ui);
                const nextUiJson = serializeUi(nextUi);
                const uiChanged =
                    !state.hasHostState || nextUiJson !== state.lastPersistedUiJson;

                state.hostMeta = nextMeta;
                state.hasHostState = true;
                if (
                    Object.prototype.hasOwnProperty.call(
                        nextMeta.cardState || {},
                        "pendingWorklogStops",
                    )
                ) {
                    state.pendingWorklogStops = normalizePendingStops(
                        nextMeta.cardState?.pendingWorklogStops,
                    );
                }
                if (
                    Object.prototype.hasOwnProperty.call(
                        nextMeta.cardState || {},
                        "activeWorklogIntervals",
                    )
                ) {
                    state.activeWorklogIntervals = normalizeActiveWorklogIntervals(
                        nextMeta.cardState?.activeWorklogIntervals,
                    );
                    syncHeartbeatLoop();
                }

                if (uiChanged) {
                    state.ui = nextUi;
                    state.lastPersistedUiJson = nextUiJson;
                    persistPreviewUi(nextUi);
                    syncUiSignals(nextUi);
                }

                if (
                    Object.prototype.hasOwnProperty.call(
                        nextMeta.cardState || {},
                        "kanban_settings",
                    )
                ) {
                    syncKanbanSettings(nextMeta.cardState?.kanban_settings || null);
                    if (state.activeSheet === "view") {
                        renderViewSheet();
                    }
                } else if (state.settings.viewName) {
                    persistKanbanSettingsToHost(state.settings);
                }

                updateStatus();
                const nextEnabled = nextMeta.streams.enabled !== false;
                if (previousEnabled !== nextEnabled) {
                    if (nextEnabled) {
                        refreshRuntime(true).catch((error) => {
                            state.transportError =
                                error instanceof Error ? error.message : String(error);
                            updateStatus();
                        });
                    } else {
                        stopStream();
                    }
                }
            }

            app.addEventListener("click", async (event) => {
                try {
                    const reconnectButton = event.target.closest("#kanban-reconnect");
                    if (reconnectButton) {
                        event.preventDefault();
                        await refreshRuntime(true);
                        return;
                    }

                    const toggleUpdatesButton = event.target.closest("#kanban-toggle-updates");
                    if (toggleUpdatesButton) {
                        event.preventDefault();
                        togglePausedUpdates();
                        return;
                    }

                    const toggleButton = event.target.closest("#kanban-toggle-stream");
                    if (toggleButton) {
                        event.preventDefault();
                        toggleWidgetStream();
                        return;
                    }

                    const openFiltersFromViewButton = event.target.closest(
                        "[data-open-filters-from-view]",
                    );
                    if (openFiltersFromViewButton) {
                        event.preventDefault();
                        await openFilterSheet();
                        return;
                    }

                    const setDefaultBodyModeButton = event.target.closest(
                        "[data-set-default-body-mode]",
                    );
                    if (setDefaultBodyModeButton) {
                        event.preventDefault();
                        const nextMode = String(
                            setDefaultBodyModeButton.dataset.setDefaultBodyMode || "",
                        );
                        if (isBodyMode(nextMode)) {
                            persistUi({
                                ...state.ui,
                                defaultBodyMode: nextMode,
                                cardModes: {},
                            });
                            if (isSheetVisible(elements.viewSheet)) {
                                renderViewSheet();
                            }
                        }
                        return;
                    }

                    const saveViewConfigButton = event.target.closest(
                        "[data-save-view-config]",
                    );
                    if (saveViewConfigButton) {
                        event.preventDefault();
                        await saveViewConfig();
                        if (isSheetVisible(elements.viewSheet)) {
                            renderViewSheet();
                        }
                        return;
                    }

                    const sourceViewChoiceButton = event.target.closest(
                        "[data-source-view-id]",
                    );
                    if (sourceViewChoiceButton) {
                        event.preventDefault();
                        const input = elements.viewSheetBody?.querySelector("#kanban-source-view");
                        if (input) {
                            const id = Number(sourceViewChoiceButton.dataset.sourceViewId || 0);
                            const view = findViewById(id) || {
                                id,
                                name: String(sourceViewChoiceButton.dataset.sourceViewName || ""),
                            };
                            const label = viewLabel(view);
                            input.value = label;
                            state.sourceViewQuery = label;
                            updateSourceViewSummary();
                            hideSourceViewSuggestions();
                        }
                        return;
                    }

                    const toggleUpdatesFromViewButton = event.target.closest(
                        "[data-toggle-updates-from-view]",
                    );
                    if (toggleUpdatesFromViewButton) {
                        event.preventDefault();
                        togglePausedUpdates();
                        if (isSheetVisible(elements.viewSheet)) {
                            renderViewSheet();
                        }
                        return;
                    }

                    const toggleStreamFromViewButton = event.target.closest(
                        "[data-toggle-stream-from-view]",
                    );
                    if (toggleStreamFromViewButton) {
                        event.preventDefault();
                        toggleWidgetStream();
                        if (isSheetVisible(elements.viewSheet)) {
                            renderViewSheet();
                        }
                        return;
                    }

                    const reconnectFromViewButton = event.target.closest(
                        "[data-reconnect-from-view]",
                    );
                    if (reconnectFromViewButton) {
                        event.preventDefault();
                        await refreshRuntime(true);
                        if (isSheetVisible(elements.viewSheet)) {
                            renderViewSheet();
                        }
                        return;
                    }

                    const clearFiltersButton = event.target.closest("[data-clear-filters]");
                    if (clearFiltersButton) {
                        event.preventDefault();
                        state.draftFilters = emptyFilterState();
                        renderActiveFilters();
                        if (isSheetVisible(elements.filterSheet)) {
                            renderFilterSheet();
                        }
                        await applyFiltersAndRefresh();
                        return;
                    }

                    const clearFilterButton = event.target.closest("[data-clear-filter]");
                    if (clearFilterButton) {
                        event.preventDefault();
                        clearFilterKey(String(clearFilterButton.dataset.clearFilter || ""));
                        await applyFiltersAndRefresh();
                        return;
                    }

                    const applyFiltersButton = event.target.closest("[data-apply-filters]");
                    if (applyFiltersButton) {
                        event.preventDefault();
                        state.draftFilters = readFilterSheet();
                        await applyFiltersAndRefresh();
                        closeSheet("filter");
                        return;
                    }

                    const parentChoiceButton = event.target.closest("[data-parent-choice]");
                    if (parentChoiceButton) {
                        event.preventDefault();
                        const root = parentChoiceButton.closest("[data-record-form-root]");
                        if (root) {
                            const hiddenParent = root.querySelector("[name='parentId']");
                            if (hiddenParent) {
                                hiddenParent.value = String(
                                    parentChoiceButton.dataset.parentChoice || "",
                                );
                            }
                            root.dataset.parentHead = String(
                                parentChoiceButton.dataset.parentHead || "",
                            );
                            renderParentChoices(root);
                        }
                        return;
                    }

                    const submitRecordButton = event.target.closest("[data-submit-record]");
                    if (submitRecordButton) {
                        event.preventDefault();
                        await submitRecordForm(
                            String(submitRecordButton.dataset.submitRecord || "create"),
                        );
                        return;
                    }

                    const focusActionSubmitButton = event.target.closest("[data-focus-action-submit]");
                    if (focusActionSubmitButton) {
                        event.preventDefault();
                        event.stopPropagation();
                        await submitFocusAction();
                        return;
                    }

                    const focusActionCancelButton = event.target.closest(
                        "[data-focus-action-cancel],[data-focus-action-close]",
                    );
                    if (focusActionCancelButton) {
                        event.preventDefault();
                        event.stopPropagation();
                        closeFocusAction();
                        return;
                    }

                    const submitDeleteButton = event.target.closest("[data-submit-delete]");
                    if (submitDeleteButton) {
                        event.preventDefault();
                        await deleteRecordFromUi(Number(submitDeleteButton.dataset.submitDelete));
                        return;
                    }

                    if (!event.target.closest(".autocompleteHost")) {
                        hideSourceViewSuggestions();
                    }

                } catch (error) {
                    state.transportError =
                        error instanceof Error ? error.message : String(error);
                    updateStatus();
                }
            });

            app.addEventListener("input", (event) => {
                const sourceViewInput = event.target.closest("#kanban-source-view");
                if (sourceViewInput) {
                    state.sourceViewQuery = String(sourceViewInput.value || "");
                    updateSourceViewSummary();
                    renderSourceViewSuggestions();
                    return;
                }

                const viewNameInput = event.target.closest("#kanban-view-name");
                if (viewNameInput) {
                    updateSourceViewSummary();
                    return;
                }

                const parentSearchInput = event.target.closest(
                    "[data-parent-search-head],[data-parent-search-categories]",
                );
                if (!parentSearchInput) {
                    return;
                }

                const root = parentSearchInput.closest("[data-record-form-root]");
                if (!root) {
                    return;
                }

                renderParentChoices(root);
            });

            app.addEventListener("keydown", (event) => {
                const sourceViewInput = event.target.closest("#kanban-source-view");
                if (!sourceViewInput) {
                    return;
                }

                if (event.key === "Escape") {
                    hideSourceViewSuggestions();
                    return;
                }

                if (event.key !== "Enter") {
                    return;
                }

                event.preventDefault();
                const selected = resolveSourceViewFromInput(sourceViewInput.value);
                if (!selected) {
                    return;
                }
                const label = viewLabel(selected);
                sourceViewInput.value = label;
                state.sourceViewQuery = label;
                updateSourceViewSummary();
                hideSourceViewSuggestions();
            });

            app.addEventListener("change", async (event) => {
                const showParentToggle = event.target.closest("[data-setting-show-parent]");
                if (showParentToggle) {
                    try {
                        await updateKanbanSettings({
                            showParentContext: showParentToggle.checked === true,
                        });
                    } catch (error) {
                        state.transportError =
                            error instanceof Error ? error.message : String(error);
                        updateStatus();
                    }
                    return;
                }

                const viewNameInput = event.target.closest("#kanban-view-name");
                if (viewNameInput) {
                    return;
                }

                const markdownTaskCheckbox = event.target.closest("input[data-markdown-task-line]");
                if (!markdownTaskCheckbox) {
                    return;
                }

                await persistMarkdownTaskToggle(markdownTaskCheckbox);
            });

            app.addEventListener("pointerdown", (event) => {
                const resizeHandle = event.target.closest("[data-resize-handle]");
                if (!resizeHandle) {
                    return;
                }
                event.preventDefault();
                const key = String(resizeHandle.dataset.resizeHandle || "");
                const lane = state.ui.lanes[key];
                if (!lane) {
                    return;
                }
                state.resize = {
                    key,
                    side: String(resizeHandle.dataset.resizeSide || "right"),
                    startX: event.clientX,
                    startWidth: clampWidth(lane.width || DEFAULT_WIDTH),
                };
                resizeHandle.classList.add("is-resizing");
                document.body.style.cursor = "ew-resize";
            });

            app.addEventListener("dragstart", (event) => {
                const card = event.target.closest(".card");
                if (!card) {
                    return;
                }
                state.dragRecordId = Number(card.dataset.recordId || 0) || null;
                event.dataTransfer?.setData(
                    "text/plain",
                    String(state.dragRecordId || ""),
                );
            });

            app.addEventListener("dragend", () => {
                state.dragRecordId = null;
                for (const column of elements.columns.querySelectorAll(".col")) {
                    column.classList.remove("dragOver");
                }
            });

            app.addEventListener("dragover", (event) => {
                const list = event.target.closest("[data-dropzone]");
                if (!list) {
                    return;
                }
                event.preventDefault();
            });

            app.addEventListener("dragenter", (event) => {
                const column = event.target.closest(".col");
                if (column) {
                    column.classList.add("dragOver");
                }
            });

            app.addEventListener("dragleave", (event) => {
                const column = event.target.closest(".col");
                if (column && !column.contains(event.relatedTarget)) {
                    column.classList.remove("dragOver");
                }
            });

            app.addEventListener("drop", (event) => {
                const list = event.target.closest("[data-dropzone]");
                if (!list) {
                    return;
                }
                event.preventDefault();
                for (const column of elements.columns.querySelectorAll(".col")) {
                    column.classList.remove("dragOver");
                }
                const laneKey = String(list.dataset.dropzone || "");
                const recordId =
                    state.dragRecordId ||
                    Number(event.dataTransfer?.getData("text/plain") || 0);
                if (!Number.isInteger(recordId) || !laneKey) {
                    return;
                }
                moveRecord(recordId, laneToQuantity(laneKey), laneKey).catch((error) => {
                    state.transportError =
                        error instanceof Error ? error.message : String(error);
                    updateStatus();
                });
            });

            app.addEventListener("lince-bridge-state", (event) => {
                if (!event.detail || typeof event.detail !== "object") {
                    return;
                }
                applyHostMeta(event.detail.meta || null);
            });

            window.addEventListener("pointermove", (event) => {
                if (!state.resize) {
                    return;
                }
                const { key, side, startX, startWidth } = state.resize;
                const delta = event.clientX - startX;
                const signedDelta = side === "left" ? -delta : delta;
                const nextWidth = clampWidth(startWidth + signedDelta);
                persistUi({
                    ...state.ui,
                    lanes: {
                        ...state.ui.lanes,
                        [key]: {
                            ...state.ui.lanes[key],
                            collapsed: false,
                            width: nextWidth,
                        },
                    },
                });
            });

            window.addEventListener("pointerup", () => {
                if (!state.resize) {
                    return;
                }
                const handle = app.querySelector(
                    `[data-resize-handle="${CSS.escape(String(state.resize.key))}"]`,
                );
                handle?.classList.remove("is-resizing");
                document.body.style.cursor = "";
                state.resize = null;
            });

            window.addEventListener("online", () => {
                flushPendingWorklogStops().catch(() => {});
                if (!state.updatesPaused && streamEnabled()) {
                    refreshRuntime(true).catch(() => {});
                }
            });

            window.KanbanWidget = {
                refreshRuntime,
                loadRecordDetail,
                syncActiveSheet,
                syncSheetFromSignals,
                syncUiFromSignals,
                persistUiFromSignals,
                openEditSheet,
                openFocusAction,
                closeFocusAction,
                createComment,
                editComment,
                deleteComment,
                createResourceRef,
                deleteResourceRef,
                startWorklog,
                deleteRecordFromUi,
                stopWorklog,
                closeFocus,
            };

            (async () => {
                try {
                    const hostSettings = state.hostMeta?.cardState?.kanban_settings || null;
                    const storedSettings = readStoredKanbanSettings();
                    const nextSettings = hostSettings || storedSettings;

                    if (nextSettings) {
                        syncKanbanSettings(nextSettings);
                        if (!hostSettings) {
                            await postAction("update-settings", kanbanSettingsPayload(nextSettings));
                            persistKanbanSettingsToHost(nextSettings);
                        }
                    }

                    syncUiSignals(state.ui);
                    updateStatus();
                    await refreshRuntime(true);
                    if (state.ui.focusedRecordId) {
                        loadRecordDetail(state.ui.focusedRecordId).catch(() => {});
                    }
                } catch (error) {
                    state.transportError =
                        error instanceof Error ? error.message : String(error);
                    updateStatus();
                }
            })();
    "##,
    );
    script
}
