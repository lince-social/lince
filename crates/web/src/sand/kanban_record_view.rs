use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, html};

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "kanban-record-view.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "▥".into(),
            title: "Kanban Record View".into(),
            author: "Lince Labs".into(),
            version: "1.0.0".into(),
            description: "Kanban oficial para acompanhar uma view SSE da tabela record.".into(),
            details: "Lê server_id e view_id do host, organiza records por quantity em colunas de backlog, next, wip, review e done, e persiste o estado da interface no card.".into(),
            initial_width: 6,
            initial_height: 5,
            permissions: vec![
                "bridge_state".into(),
                "read_view_stream".into(),
                "write_records".into(),
            ],
        },
        head_links: vec![],
        inline_styles: vec![style()],
        body: body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script())],
    }
}

fn body() -> Markup {
    html! {
        div id="app" data-lince-bridge-root {}
    }
}

fn style() -> &'static str {
    r#"
            :root {
                color-scheme: dark;
                --bg: #0e1013;
                --panel: #14181d;
                --panel-alt: #1a1f26;
                --line: #2a313a;
                --line-strong: #394451;
                --text: #e7edf4;
                --muted: #91a0b1;
                --soft: #c1ccd9;
                --accent: #7aa2f7;
                --done: #4cc58b;
                --backlog: #8e9cae;
                --next: #aa86ff;
                --wip: #d1ae58;
                --review: #df748a;
                --warn-bg: #23191b;
                --warn-line: #4f2e33;
                --warn-text: #ffd9de;
            }

            * {
                box-sizing: border-box;
            }

            html,
            body {
                height: 100%;
                margin: 0;
                background: var(--bg);
                color: var(--text);
                font-family:
                    ui-sans-serif,
                    system-ui,
                    -apple-system,
                    BlinkMacSystemFont,
                    "Segoe UI",
                    sans-serif;
            }

            body {
                overflow: hidden;
            }

            #app {
                height: 100%;
            }

            .widget {
                height: 100%;
                padding: 10px;
                display: flex;
                flex-direction: column;
                gap: 10px;
                background:
                    radial-gradient(
                        circle at top right,
                        rgba(122, 162, 247, 0.08),
                        transparent 30%
                    ),
                    linear-gradient(
                        180deg,
                        rgba(16, 18, 23, 0.98),
                        rgba(11, 13, 17, 0.98)
                    );
            }

            .widgetSurface {
                flex: 1;
                min-height: 0;
                display: flex;
                flex-direction: column;
                gap: 10px;
            }

            .panel,
            .warn {
                background: var(--panel);
                border: 1px solid var(--line);
                border-radius: 14px;
                padding: 12px;
            }

            .panel {
                display: grid;
                gap: 10px;
            }

            .warn {
                background: var(--warn-bg);
                border-color: var(--warn-line);
                color: var(--warn-text);
                overflow: auto;
            }

            .header {
                display: flex;
                align-items: flex-start;
                justify-content: space-between;
                gap: 10px;
                min-width: 0;
            }

            .headerTitle {
                font-size: 14px;
                font-weight: 700;
                letter-spacing: 0.02em;
                white-space: nowrap;
                overflow: hidden;
                text-overflow: ellipsis;
            }

            .headerSub {
                margin-top: 4px;
                color: var(--muted);
                font-size: 11px;
                line-height: 1.4;
            }

            .headerMeta {
                min-width: 0;
            }

            .headerActions {
                display: flex;
                align-items: center;
                gap: 8px;
                flex-wrap: wrap;
                justify-content: flex-end;
                flex: 0 0 auto;
            }

            .status {
                display: inline-flex;
                align-items: center;
                gap: 6px;
                padding: 5px 9px;
                border-radius: 999px;
                background: rgba(255, 255, 255, 0.04);
                border: 1px solid var(--line);
                color: var(--muted);
                font-size: 10px;
                letter-spacing: 0.06em;
                text-transform: uppercase;
            }

            .status.is-live {
                color: #d8f5e8;
                border-color: rgba(76, 197, 139, 0.3);
                background: rgba(23, 53, 39, 0.72);
            }

            .status.is-paused {
                color: #f3d6ac;
                border-color: rgba(228, 181, 107, 0.26);
                background: rgba(57, 40, 16, 0.74);
            }

            .status.is-error {
                color: #ffd9de;
                border-color: rgba(223, 116, 138, 0.28);
                background: rgba(69, 25, 34, 0.72);
            }

            .pill {
                display: inline-flex;
                align-items: center;
                gap: 6px;
                padding: 4px 8px;
                border-radius: 999px;
                background: rgba(255, 255, 255, 0.04);
                border: 1px solid var(--line);
                font-size: 10px;
                color: var(--muted);
            }

            button {
                appearance: none;
                border: 1px solid var(--line);
                border-radius: 10px;
                background: var(--panel-alt);
                color: var(--text);
                font: inherit;
                cursor: pointer;
                transition:
                    border-color 120ms ease,
                    background 120ms ease,
                    color 120ms ease,
                    transform 120ms ease;
            }

            button:hover {
                border-color: var(--line-strong);
                background: #202733;
            }

            button:disabled {
                opacity: 0.55;
                cursor: default;
                transform: none;
            }

            .toolbarBtn {
                min-height: 32px;
                padding: 0 11px;
                font-size: 11px;
            }

            .toolbarBtn--accent {
                border-color: rgba(122, 162, 247, 0.28);
                color: #d9e5ff;
            }

            .toolbarBtn--paused {
                border-color: rgba(228, 181, 107, 0.26);
                color: #f3d6ac;
            }

            .boardWrap {
                flex: 1;
                min-height: 0;
                overflow: auto;
                padding-bottom: 2px;
            }

            .board {
                min-height: min-content;
                display: flex;
                align-items: flex-start;
                gap: 10px;
            }

            .col {
                flex: 0 0 auto;
                width: 260px;
                min-width: 260px;
                display: flex;
                flex-direction: column;
                align-self: flex-start;
                min-height: 0;
                border: 1px solid var(--line);
                border-radius: 14px;
                background: rgba(20, 24, 29, 0.96);
                overflow: hidden;
            }

            .col.is-collapsed {
                width: 64px;
                min-width: 64px;
            }

            .colHead {
                display: flex;
                align-items: flex-start;
                justify-content: space-between;
                gap: 8px;
                padding: 10px;
                border-bottom: 1px solid var(--line);
                background: rgba(255, 255, 255, 0.02);
            }

            .col.is-collapsed .colHead {
                align-items: center;
                justify-content: flex-start;
                padding: 12px 8px;
                border-bottom: 0;
            }

            .colHeadMain {
                min-width: 0;
                display: flex;
                align-items: center;
                gap: 8px;
            }

            .col.is-collapsed .colHeadMain {
                flex-direction: column;
                justify-content: center;
                gap: 10px;
                width: 100%;
                writing-mode: vertical-rl;
                transform: rotate(180deg);
            }

            .laneToggle {
                width: 24px;
                height: 24px;
                min-width: 24px;
                padding: 0;
                display: inline-grid;
                place-items: center;
                border-radius: 8px;
                color: var(--muted);
            }

            .col.is-collapsed .laneToggle {
                transform: rotate(180deg);
            }

            .colName {
                font-size: 12px;
                font-weight: 700;
                letter-spacing: 0.02em;
            }

            .count {
                display: inline-flex;
                align-items: center;
                justify-content: center;
                min-width: 22px;
                padding: 2px 6px;
                border-radius: 999px;
                background: rgba(255, 255, 255, 0.04);
                border: 1px solid rgba(255, 255, 255, 0.06);
                color: var(--muted);
                font-size: 10px;
            }

            .colTools {
                display: flex;
                align-items: center;
                gap: 6px;
            }

            .colToolBtn {
                width: 26px;
                height: 26px;
                min-width: 26px;
                padding: 0;
                border-radius: 8px;
                font-size: 12px;
            }

            .list {
                display: flex;
                flex-direction: column;
                gap: 8px;
                padding: 10px;
                max-height: min(70vh, calc(100vh - 220px));
                min-height: 0;
                overflow: auto;
            }

            .empty {
                padding: 12px 10px;
                color: var(--muted);
                font-size: 11px;
                line-height: 1.4;
                border: 1px dashed rgba(255, 255, 255, 0.08);
                border-radius: 12px;
            }

            .card {
                position: relative;
                display: grid;
                gap: 6px;
                padding: 10px;
                border-radius: 12px;
                border: 1px solid var(--line);
                background: var(--panel-alt);
                cursor: grab;
                user-select: none;
                transition:
                    border-color 120ms ease,
                    transform 120ms ease;
            }

            .card:active {
                cursor: grabbing;
            }

            .card:hover {
                border-color: var(--line-strong);
            }

            .card.backlog {
                border-left: 3px solid var(--backlog);
            }

            .card.next {
                border-left: 3px solid var(--next);
            }

            .card.wip {
                border-left: 3px solid var(--wip);
            }

            .card.review {
                border-left: 3px solid var(--review);
            }

            .card.done {
                border-left: 3px solid var(--done);
            }

            .cardActions {
                position: absolute;
                top: 8px;
                right: 8px;
                display: flex;
                align-items: center;
                gap: 4px;
                opacity: 0;
                pointer-events: none;
                transition: opacity 120ms ease;
            }

            .card:hover .cardActions,
            .card:focus-within .cardActions {
                opacity: 1;
                pointer-events: auto;
            }

            .cardAction {
                width: 24px;
                height: 24px;
                min-width: 24px;
                padding: 0;
                border-radius: 7px;
                font-size: 11px;
                color: var(--muted);
            }

            .cardAction.is-active {
                border-color: rgba(122, 162, 247, 0.3);
                color: #d9e5ff;
            }

            .head {
                padding-right: 84px;
                font-size: 12px;
                font-weight: 680;
                line-height: 1.32;
                word-break: break-word;
            }

            .meta {
                display: flex;
                align-items: center;
                justify-content: space-between;
                gap: 8px;
                font-size: 10px;
                color: var(--muted);
            }

            .body {
                color: var(--soft);
                font-size: 11px;
                line-height: 1.45;
                white-space: pre-wrap;
                overflow: hidden;
            }

            .body.is-full {
                max-height: none;
            }

            .dragOver {
                outline: 1px solid var(--accent);
                box-shadow: inset 0 0 0 1px rgba(122, 162, 247, 0.16);
            }

            .warnTitle {
                margin: 0;
                font-size: 13px;
                font-weight: 700;
            }

            .small {
                color: var(--muted);
                font-size: 10px;
                line-height: 1.45;
            }

            .warn .small {
                color: #e8bbc2;
            }

            code,
            pre {
                font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
            }

            pre {
                margin: 8px 0 0;
                white-space: pre-wrap;
                word-break: break-word;
                font-size: 10px;
                line-height: 1.35;
            }

            @media (max-width: 720px) {
                .widget {
                    padding: 8px;
                }

                .header {
                    flex-direction: column;
                    align-items: flex-start;
                }

                .headerActions {
                    justify-content: flex-start;
                }
            }
    "#
}

fn script() -> &'static str {
    r#"
            const app = document.getElementById("app");
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
            const MIN_WIDTH = 180;
            const MAX_WIDTH = 420;
            const DEFAULT_BODY_MODE = "compact";
            const BODY_MODES = new Set(["head", "compact", "full"]);

            const state = {
                hostMeta: normalizeHostMeta(null),
                hasHostState: false,
                ui: loadPreviewUi(),
                lastPersistedUiJson: "",
                data: null,
                mismatch: null,
                transportError: "",
                loading: false,
                connected: false,
                dragId: null,
                lastUpdate: "",
                reconnectAttempt: 0,
                reconnectTimer: null,
                streamController: null,
                streamGeneration: 0,
                persistTimer: null,
            };

            state.lastPersistedUiJson = serializeUi(state.ui);

            function normalizeHostMeta(rawMeta) {
                const frameServerId = String(
                    frame?.dataset?.linceServerId || "",
                ).trim();
                const frameViewIdRaw = String(
                    frame?.dataset?.linceViewId || "",
                ).trim();
                const frameViewId = Number(frameViewIdRaw);
                const rawStreams = rawMeta?.streams || {};
                const globalEnabled = rawStreams.globalEnabled !== false;
                const cardEnabled = rawStreams.cardEnabled !== false;

                return {
                    mode: rawMeta?.mode === "edit" ? "edit" : "view",
                    serverId: String(rawMeta?.serverId || frameServerId).trim(),
                    viewId:
                        rawMeta?.viewId == null
                            ? Number.isInteger(frameViewId) && frameViewId > 0
                                ? frameViewId
                                : null
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

            function clampWidth(value) {
                const parsed = Number(value);
                if (!Number.isFinite(parsed)) {
                    return DEFAULT_WIDTH;
                }

                return Math.max(
                    MIN_WIDTH,
                    Math.min(MAX_WIDTH, Math.round(parsed)),
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
                    for (const [key, value] of Object.entries(
                        rawUi.cardModes,
                    )) {
                        if (isBodyMode(value)) {
                            cardModes[String(key)] = String(value);
                        }
                    }
                }

                return {
                    lanes: nextLanes,
                    defaultBodyMode: isBodyMode(rawUi?.defaultBodyMode)
                        ? String(rawUi.defaultBodyMode)
                        : DEFAULT_BODY_MODE,
                    cardModes,
                };
            }

            function storageKey() {
                const instanceId =
                    String(
                        frame?.dataset?.packageInstanceId || "preview",
                    ).trim() || "preview";
                return "lince.widget.kanban." + instanceId + ".ui";
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

            function serializeUi(ui) {
                return JSON.stringify(normalizeUi(ui));
            }

            function hasValidConfig() {
                return (
                    Boolean(state.hostMeta.serverId) &&
                    Number.isInteger(state.hostMeta.viewId)
                );
            }

            function streamEnabled() {
                return state.hostMeta.streams.enabled !== false;
            }

            function buildStreamUrl() {
                if (!hasValidConfig()) {
                    return "";
                }

                return (
                    "/host/integrations/servers/" +
                    encodeURIComponent(state.hostMeta.serverId) +
                    "/views/" +
                    encodeURIComponent(String(state.hostMeta.viewId)) +
                    "/stream"
                );
            }

            function buildRecordUrl(id) {
                if (!state.hostMeta.serverId) {
                    return "";
                }

                return (
                    "/host/integrations/servers/" +
                    encodeURIComponent(state.hostMeta.serverId) +
                    "/table/record/" +
                    encodeURIComponent(String(id))
                );
            }

            function qtyToLane(quantity) {
                const value = Number(quantity);
                if (value > 0) {
                    return "done";
                }
                if (value === 0) {
                    return "backlog";
                }
                if (value === -1) {
                    return "next";
                }
                if (value === -2) {
                    return "wip";
                }
                if (value === -3) {
                    return "review";
                }
                return value < -3 ? "review" : "backlog";
            }

            function laneToQuantity(key) {
                const column = columns.find((entry) => entry.key === key);
                return column ? column.value : 0;
            }

            function expectedShort() {
                return {
                    view_id: "number",
                    name: "string",
                    query: "string",
                    columns: ["id", "quantity", "head", "body"],
                    rows: [
                        {
                            id: "number|string",
                            quantity: "number|string",
                            head: "string",
                            body: "string",
                        },
                    ],
                };
            }

            function receivedShort(value) {
                if (!value || typeof value !== "object") {
                    return { type: typeof value };
                }

                return {
                    keys: Object.keys(value).slice(0, 12),
                    columns: Array.isArray(value.columns)
                        ? value.columns.slice(0, 8)
                        : typeof value.columns,
                    rows_count: Array.isArray(value.rows)
                        ? value.rows.length
                        : null,
                    first_row:
                        Array.isArray(value.rows) &&
                        value.rows[0] &&
                        typeof value.rows[0] === "object"
                            ? Object.fromEntries(
                                  Object.keys(value.rows[0])
                                      .slice(0, 8)
                                      .map((key) => [
                                          key,
                                          typeof value.rows[0][key],
                                      ]),
                              )
                            : null,
                };
            }

            function validateSnapshot(value) {
                if (!value || typeof value !== "object") {
                    return false;
                }
                if (
                    !("view_id" in value) ||
                    !("name" in value) ||
                    !("query" in value)
                ) {
                    return false;
                }
                if (
                    !Array.isArray(value.columns) ||
                    !Array.isArray(value.rows)
                ) {
                    return false;
                }

                return ["id", "quantity", "head", "body"].every((key) =>
                    value.columns.includes(key),
                );
            }

            function normalizeRows(rows) {
                if (!Array.isArray(rows)) {
                    return [];
                }

                return rows.map((row) => ({
                    id: row?.id,
                    quantity: Number(row?.quantity),
                    head: row?.head == null ? "" : String(row.head),
                    body: row?.body == null ? "" : String(row.body),
                }));
            }

            function rowById(rowId) {
                return (
                    normalizeRows(state.data?.rows).find(
                        (row) => String(row.id) === String(rowId),
                    ) || null
                );
            }

            function bodyModeFor(rowId) {
                return (
                    state.ui.cardModes[String(rowId)] ||
                    state.ui.defaultBodyMode
                );
            }

            function compactBody(text) {
                const trimmed = String(text || "").trim();
                if (!trimmed) {
                    return "";
                }

                const lines = trimmed
                    .split(/\r?\n/)
                    .map((line) => line.trimEnd());
                const excerpt = [];
                let length = 0;
                for (const line of lines) {
                    excerpt.push(line);
                    length += line.length;
                    if (excerpt.length >= 4 || length >= 220) {
                        break;
                    }
                }

                const compact = excerpt.join("\n").trim();
                return compact.length < trimmed.length
                    ? compact + "\n..."
                    : compact;
            }

            function bodyFor(rowId, mode) {
                const row = rowById(rowId);
                if (!row) {
                    return "";
                }

                if (mode === "head") {
                    return "";
                }

                if (mode === "full") {
                    return row.body || "";
                }

                return compactBody(row.body || "");
            }

            function rememberMismatch(payload) {
                const expected = expectedShort();
                const arrived = receivedShort(payload);
                console.warn("[kanban-lince] snapshot mismatch", {
                    expected,
                    arrived,
                });
                state.mismatch = { payload, expected, arrived };
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
            }

            function scheduleReconnect() {
                clearReconnectTimer();
                if (!hasValidConfig() || !streamEnabled()) {
                    return;
                }

                const delay = Math.min(
                    15000,
                    1500 * Math.max(1, state.reconnectAttempt + 1),
                );
                state.reconnectAttempt += 1;
                state.reconnectTimer = window.setTimeout(
                    () => connectStream(false),
                    delay,
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

            function handleSnapshot(payload) {
                state.connected = true;
                state.loading = false;
                state.lastUpdate = new Date().toLocaleTimeString();
                state.reconnectAttempt = 0;

                if (validateSnapshot(payload)) {
                    state.data = payload;
                    state.mismatch = null;
                    state.transportError = "";
                } else {
                    rememberMismatch(payload);
                    state.transportError =
                        "The stream delivered data, but the payload shape does not match the expected Record view snapshot.";
                }

                render();
            }

            function handleStreamErrorPayload(payload) {
                const message =
                    payload && typeof payload === "object" && payload.error
                        ? String(payload.error)
                        : "The backend stream reported an error.";
                state.connected = false;
                state.loading = false;
                state.transportError = message;
                render();
            }

            async function consumeSseResponse(response, generation, signal) {
                const reader = response.body.getReader();
                const decoder = new TextDecoder();
                let buffer = "";

                while (true) {
                    const { value, done } = await reader.read();
                    if (
                        done ||
                        signal.aborted ||
                        generation !== state.streamGeneration
                    ) {
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
                            payload = event.data;
                        }

                        if (event.event === "snapshot") {
                            handleSnapshot(payload);
                        } else if (event.event === "error") {
                            handleStreamErrorPayload(payload);
                        }
                    }
                }
            }

            async function connectStream(reset) {
                stopStream();

                if (!hasValidConfig() || !streamEnabled()) {
                    state.loading = false;
                    render();
                    return;
                }

                if (reset) {
                    state.loading = true;
                    state.connected = false;
                    state.mismatch = null;
                    state.transportError = "";
                }

                render();

                const generation = ++state.streamGeneration;
                const controller = new AbortController();
                state.streamController = controller;

                try {
                    const response = await fetch(buildStreamUrl(), {
                        headers: { Accept: "text/event-stream" },
                        cache: "no-store",
                        signal: controller.signal,
                    });

                    if (
                        controller.signal.aborted ||
                        generation !== state.streamGeneration
                    ) {
                        return;
                    }

                    if (response.status === 401) {
                        state.loading = false;
                        state.connected = false;
                        state.transportError =
                            "Servidor bloqueado. Entre com suas credenciais no host para liberar esse widget.";
                        render();
                        return;
                    }

                    if (!response.ok || !response.body) {
                        const raw = await response.text().catch(() => "");
                        throw new Error(
                            raw ||
                                `Nao foi possivel abrir o stream (${response.status}).`,
                        );
                    }

                    state.connected = true;
                    state.loading = !state.data;
                    state.transportError = "";
                    render();

                    await consumeSseResponse(
                        response,
                        generation,
                        controller.signal,
                    );

                    if (
                        controller.signal.aborted ||
                        generation !== state.streamGeneration
                    ) {
                        return;
                    }

                    state.connected = false;
                    state.loading = !state.data;
                    state.transportError = "The stream ended. Reconnecting...";
                    render();
                    scheduleReconnect();
                } catch (error) {
                    if (
                        controller.signal.aborted ||
                        generation !== state.streamGeneration
                    ) {
                        return;
                    }

                    state.connected = false;
                    state.loading = !state.data;
                    state.transportError = String(
                        error instanceof Error ? error.message : error,
                    );
                    render();
                    scheduleReconnect();
                } finally {
                    if (state.streamController === controller) {
                        state.streamController = null;
                    }
                }
            }

            function applyHostMeta(rawMeta) {
                const previousMeta = state.hostMeta;
                const nextMeta = normalizeHostMeta(rawMeta);
                const previousKey = `${previousMeta.serverId}:${previousMeta.viewId || ""}`;
                const nextKey = `${nextMeta.serverId}:${nextMeta.viewId || ""}`;
                const previousEnabled = previousMeta.streams.enabled !== false;
                const nextEnabled = nextMeta.streams.enabled !== false;
                const nextUi = normalizeUi(nextMeta.cardState?.ui);
                const nextUiJson = serializeUi(nextUi);
                const uiChanged =
                    !state.hasHostState ||
                    nextUiJson !== state.lastPersistedUiJson;

                state.hostMeta = nextMeta;
                state.hasHostState = true;

                if (uiChanged) {
                    state.ui = nextUi;
                    state.lastPersistedUiJson = nextUiJson;
                    persistPreviewUi(nextUi);
                }

                if (!hasValidConfig() || !nextEnabled) {
                    stopStream();
                    state.loading = false;
                    state.connected = false;
                    render();
                    return;
                }

                if (
                    previousKey !== nextKey ||
                    previousEnabled !== nextEnabled
                ) {
                    connectStream(true);
                    return;
                }

                render();
            }

            function persistUi(nextUi) {
                const normalized = normalizeUi(nextUi);
                const nextJson = serializeUi(normalized);
                state.ui = normalized;

                if (nextJson === state.lastPersistedUiJson) {
                    return;
                }

                state.lastPersistedUiJson = nextJson;
                persistPreviewUi(normalized);

                if (state.persistTimer) {
                    window.clearTimeout(state.persistTimer);
                }

                state.persistTimer = window.setTimeout(() => {
                    state.persistTimer = null;
                    window.LinceWidgetHost?.patchCardState?.({
                        ui: normalized,
                    });
                }, 140);
            }

            function toggleWidgetStream() {
                const nextEnabled = !(
                    state.hostMeta.streams.cardEnabled !== false
                );
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

                if (streamEnabled()) {
                    connectStream(true);
                    return;
                }

                stopStream();
                state.loading = false;
                render();
            }

            function reconnect() {
                if (!hasValidConfig() || !streamEnabled()) {
                    render();
                    return;
                }

                connectStream(true);
            }

            function currentStreamLabel() {
                if (state.hostMeta.streams.globalEnabled === false) {
                    return "Paused globally";
                }
                if (state.hostMeta.streams.cardEnabled === false) {
                    return "Paused";
                }
                if (state.connected) {
                    return "Live";
                }
                if (state.transportError) {
                    return "Offline";
                }
                return "Waiting";
            }

            function currentStreamClass() {
                if (
                    state.hostMeta.streams.globalEnabled === false ||
                    state.hostMeta.streams.cardEnabled === false
                ) {
                    return "is-paused";
                }
                if (state.connected) {
                    return "is-live";
                }
                if (state.transportError) {
                    return "is-error";
                }
                return "";
            }

            function groupedRows() {
                const grouped = {
                    backlog: [],
                    next: [],
                    wip: [],
                    review: [],
                    done: [],
                };

                for (const row of normalizeRows(state.data?.rows)) {
                    grouped[qtyToLane(row.quantity)].push(row);
                }

                for (const key of Object.keys(grouped)) {
                    grouped[key].sort((left, right) =>
                        left.head.localeCompare(right.head),
                    );
                }

                return grouped;
            }

            function laneExpression(key) {
                return `$ui.lanes.${key}`;
            }

            function jsString(value) {
                return String(value)
                    .replaceAll("\\", "\\\\")
                    .replaceAll("'", "\\'");
            }

            function escapeHtml(value) {
                return String(value).replace(
                    /[&<>\"']/g,
                    (char) =>
                        ({
                            "&": "&amp;",
                            "<": "&lt;",
                            ">": "&gt;",
                            '"': "&quot;",
                            "'": "&#39;",
                        })[char],
                );
            }

            function jsonAttr(value) {
                return escapeHtml(JSON.stringify(value));
            }

            function renderMissingConfig() {
                return `
        <div class="widget">
          <div class="panel">
            <div class="header">
              <div class="headerMeta">
                <div class="headerTitle">Kanban Record View</div>
                <div class="headerSub">Use the board Configure action to pick a server and view.</div>
              </div>
              <div class="headerActions">
                <span class="pill">Host config required</span>
              </div>
            </div>
            <div class="small">
              This widget reads <code>server_id</code> and <code>view_id</code> from the host card configuration.
            </div>
            <div class="small">
              Current values: server <strong>${escapeHtml(state.hostMeta.serverId || "unset")}</strong> ·
              view <strong>${escapeHtml(state.hostMeta.viewId == null ? "unset" : String(state.hostMeta.viewId))}</strong>
            </div>
          </div>
        </div>
      `;
            }

            function renderLoading() {
                return `
        <div class="widget">
          <div class="panel">
            <div class="header">
              <div class="headerMeta">
                <div class="headerTitle">Kanban Record View</div>
                <div class="headerSub">server ${escapeHtml(state.hostMeta.serverId)} · view ${escapeHtml(String(state.hostMeta.viewId))}</div>
              </div>
              <div class="headerActions">
                <span class="status ${currentStreamClass()}">${escapeHtml(currentStreamLabel())}</span>
                <button class="toolbarBtn" type="button" data-widget-action="reconnect">Retry</button>
              </div>
            </div>
            <div class="small">Waiting for authenticated SSE snapshot...</div>
          </div>
        </div>
      `;
            }

            function renderPausedEmpty() {
                return `
        <div class="widget">
          <div class="panel">
            <div class="header">
              <div class="headerMeta">
                <div class="headerTitle">Kanban Record View</div>
                <div class="headerSub">server ${escapeHtml(state.hostMeta.serverId)} · view ${escapeHtml(String(state.hostMeta.viewId))}</div>
              </div>
              <div class="headerActions">
                <span class="status ${currentStreamClass()}">${escapeHtml(currentStreamLabel())}</span>
                <button
                  class="toolbarBtn ${state.hostMeta.streams.cardEnabled === false ? "toolbarBtn--accent" : "toolbarBtn--paused"}"
                  type="button"
                  data-widget-action="toggle-stream"
                >
                  ${state.hostMeta.streams.cardEnabled === false ? "Resume widget" : "Pause widget"}
                </button>
              </div>
            </div>
            <div class="small">
              The stream is paused. Resume this widget or turn streams back on in the board to receive snapshots.
            </div>
          </div>
        </div>
      `;
            }

            function renderWarning(title, copy, expected, arrived) {
                return `
        <div class="widget">
          <div class="warn">
            <div class="header">
              <div class="headerMeta">
                <h2 class="warnTitle">${escapeHtml(title)}</h2>
                <div class="small">${escapeHtml(copy)}</div>
              </div>
              <div class="headerActions">
                <button class="toolbarBtn" type="button" data-widget-action="reconnect">Retry</button>
              </div>
            </div>
            ${expected ? `<div class="small" style="margin-top:10px">Expected</div><pre>${escapeHtml(JSON.stringify(expected, null, 2))}</pre>` : ""}
            ${arrived ? `<div class="small" style="margin-top:10px">Arrived</div><pre>${escapeHtml(JSON.stringify(arrived, null, 2))}</pre>` : ""}
          </div>
        </div>
      `;
            }

            function renderModeButton(rowId, mode, activeMode, label, title) {
                const rowKey = jsString(String(rowId));
                return `
        <button
          type="button"
          draggable="false"
          class="cardAction${activeMode === mode ? " is-active" : ""}"
          title="${escapeHtml(title)}"
          data-class:is-active="($ui.cardModes['${rowKey}'] || $ui.defaultBodyMode) === '${mode}'"
          data-on:click="evt.stopPropagation(); $ui.cardModes['${rowKey}'] = '${mode}'"
        >
          ${escapeHtml(label)}
        </button>
      `;
            }

            function renderCard(row) {
                const mode = bodyModeFor(row.id);
                const rowKey = jsString(String(row.id));
                const lane = qtyToLane(row.quantity);

                return `
        <div
          class="card ${lane}"
          draggable="true"
          data-id="${escapeHtml(String(row.id))}"
          data-qty="${escapeHtml(String(row.quantity))}"
        >
          <div class="cardActions">
            ${renderModeButton(row.id, "head", mode, "_", "Show only the head")}
            ${renderModeButton(row.id, "compact", mode, "=", "Show compact body")}
            ${renderModeButton(row.id, "full", mode, "+", "Show full body")}
          </div>
          <div class="head">${escapeHtml(row.head || "(no head)")}</div>
          <div class="meta">
            <span>#${escapeHtml(String(row.id))}</span>
            <span>qty ${escapeHtml(String(row.quantity))}</span>
          </div>
          <div
            class="body${mode === "full" ? " is-full" : ""}"
            ${mode === "head" ? 'style="display:none"' : ""}
            data-show="($ui.cardModes['${rowKey}'] || $ui.defaultBodyMode) !== 'head'"
            data-class:is-full="($ui.cardModes['${rowKey}'] || $ui.defaultBodyMode) === 'full'"
            data-text="window.KanbanWidget.bodyFor('${rowKey}', ($ui.cardModes['${rowKey}'] || $ui.defaultBodyMode))"
          >${escapeHtml(bodyFor(row.id, mode))}</div>
        </div>
      `;
            }

            function renderColumn(column, rows) {
                const lane = state.ui.lanes[column.key];
                const laneExpr = laneExpression(column.key);

                return `
        <section
          class="col${lane.collapsed ? " is-collapsed" : ""}"
          data-col="${escapeHtml(column.key)}"
          data-class:is-collapsed="${laneExpr}.collapsed"
          data-style:width="${laneExpr}.collapsed ? '${COLLAPSED_WIDTH}px' : (${laneExpr}.width + 'px')"
          data-style:flex-basis="${laneExpr}.collapsed ? '${COLLAPSED_WIDTH}px' : (${laneExpr}.width + 'px')"
          data-style:min-width="${laneExpr}.collapsed ? '${COLLAPSED_WIDTH}px' : (${laneExpr}.width + 'px')"
        >
          <header class="colHead">
            <div class="colHeadMain">
              <button
                type="button"
                class="laneToggle"
                title="Toggle ${escapeHtml(column.label)}"
                data-on:click="evt.stopPropagation(); ${laneExpr}.collapsed = !${laneExpr}.collapsed"
              >
                <span data-show="${laneExpr}.collapsed" ${lane.collapsed ? "" : 'style="display:none"'}>+</span>
                <span data-show="!${laneExpr}.collapsed" ${lane.collapsed ? 'style="display:none"' : ""}>-</span>
              </button>
              <div class="colName">${escapeHtml(column.label)}</div>
              <div class="count">${rows.length}</div>
            </div>
            <div
              class="colTools"
              ${lane.collapsed ? 'style="display:none"' : ""}
              data-show="!${laneExpr}.collapsed"
            >
              <button
                type="button"
                class="colToolBtn"
                title="Narrower"
                data-on:click="evt.stopPropagation(); ${laneExpr}.width = Math.max(${MIN_WIDTH}, (${laneExpr}.width || ${DEFAULT_WIDTH}) - 32)"
              >-</button>
              <button
                type="button"
                class="colToolBtn"
                title="Wider"
                data-on:click="evt.stopPropagation(); ${laneExpr}.width = Math.min(${MAX_WIDTH}, (${laneExpr}.width || ${DEFAULT_WIDTH}) + 32)"
              >+</button>
            </div>
          </header>
          <div
            class="list"
            data-dropzone="${escapeHtml(column.key)}"
            ${lane.collapsed ? 'style="display:none"' : ""}
            data-show="!${laneExpr}.collapsed"
          >
            ${
                rows.length
                    ? rows.map(renderCard).join("")
                    : '<div class="empty">Drop records here</div>'
            }
          </div>
        </section>
      `;
            }

            function renderBoard() {
                const grouped = groupedRows();
                const query = String(
                    state.data?.query || "SELECT * FROM record",
                );
                const signals = { ui: state.ui };

                return `
        <div class="widget">
          <div
            class="widgetSurface"
            data-signals="${jsonAttr(signals)}"
            data-on-signal-patch="window.KanbanWidget.persistUi($ui)"
            data-on-signal-patch-filter="{include: /^ui/}"
          >
            <div class="panel">
              <div class="header">
                <div class="headerMeta">
                  <div class="headerTitle">${escapeHtml(state.data?.name || "Record")}</div>
                  <div class="headerSub">
                    server ${escapeHtml(state.hostMeta.serverId)} ·
                    view ${escapeHtml(String(state.data?.view_id ?? state.hostMeta.viewId ?? ""))} ·
                    ${escapeHtml(query)}
                  </div>
                </div>
                <div class="headerActions">
                  <span class="status ${currentStreamClass()}">${escapeHtml(currentStreamLabel())}</span>
                  ${
                      state.hostMeta.streams.globalEnabled === false
                          ? '<span class="pill">Board pause</span>'
                          : ""
                  }
                  <button
                    class="toolbarBtn ${state.hostMeta.streams.cardEnabled === false ? "toolbarBtn--accent" : "toolbarBtn--paused"}"
                    type="button"
                    data-widget-action="toggle-stream"
                  >
                    ${state.hostMeta.streams.cardEnabled === false ? "Resume widget" : "Pause widget"}
                  </button>
                  <button
                    class="toolbarBtn toolbarBtn--accent"
                    type="button"
                    data-widget-action="reconnect"
                    ${!streamEnabled() ? "disabled" : ""}
                  >
                    Reconnect
                  </button>
                </div>
              </div>
              <div class="small">
                Columns can collapse into narrow rails, widths are persistent, and card body mode is saved per record for this widget instance.
              </div>
            </div>
            <div class="boardWrap">
              <div class="board">
                ${columns.map((column) => renderColumn(column, grouped[column.key])).join("")}
              </div>
            </div>
          </div>
        </div>
      `;
            }

            function render() {
                if (!hasValidConfig()) {
                    app.innerHTML = renderMissingConfig();
                    bindCommonActions();
                    return;
                }

                if (!streamEnabled() && !state.data) {
                    app.innerHTML = renderPausedEmpty();
                    bindCommonActions();
                    return;
                }

                if (
                    state.loading &&
                    !state.data &&
                    !state.mismatch &&
                    !state.transportError
                ) {
                    app.innerHTML = renderLoading();
                    bindCommonActions();
                    return;
                }

                if (state.mismatch && !state.data) {
                    app.innerHTML = renderWarning(
                        "Snapshot mismatch",
                        "The widget assumes a Record-like SSE payload. The incoming snapshot did not match the expected short form.",
                        state.mismatch.expected,
                        state.mismatch.arrived,
                    );
                    bindCommonActions();
                    return;
                }

                if (state.transportError && !state.data && streamEnabled()) {
                    app.innerHTML = renderWarning(
                        "Stream unavailable",
                        state.transportError,
                        null,
                        null,
                    );
                    bindCommonActions();
                    return;
                }

                app.innerHTML = renderBoard();
                bindCommonActions();
                bindBoard();
            }

            function bindCommonActions() {
                app.querySelectorAll(
                    "[data-widget-action='reconnect']",
                ).forEach((button) => {
                    button.addEventListener("click", () => reconnect());
                });

                app.querySelectorAll(
                    "[data-widget-action='toggle-stream']",
                ).forEach((button) => {
                    button.addEventListener("click", () =>
                        toggleWidgetStream(),
                    );
                });
            }

            function bindBoard() {
                app.querySelectorAll(".card").forEach((element) => {
                    element.addEventListener("dragstart", (event) => {
                        state.dragId = element.dataset.id;
                        event.dataTransfer?.setData(
                            "text/plain",
                            state.dragId || "",
                        );
                    });

                    element.addEventListener("dragend", () => {
                        state.dragId = null;
                        app.querySelectorAll(".col").forEach((column) =>
                            column.classList.remove("dragOver"),
                        );
                    });
                });

                app.querySelectorAll(".col").forEach((column) => {
                    column.addEventListener("dragover", (event) =>
                        event.preventDefault(),
                    );
                    column.addEventListener("dragenter", () =>
                        column.classList.add("dragOver"),
                    );
                    column.addEventListener("dragleave", () =>
                        column.classList.remove("dragOver"),
                    );
                    column.addEventListener("drop", (event) =>
                        handleDrop(event, column.dataset.col),
                    );
                });
            }

            async function handleDrop(event, colKey) {
                event.preventDefault();
                const rawId =
                    state.dragId ||
                    event.dataTransfer?.getData("text/plain") ||
                    "";
                const recordId = Number(rawId);
                const nextQuantity = laneToQuantity(colKey);
                app.querySelectorAll(".col").forEach((column) =>
                    column.classList.remove("dragOver"),
                );

                if (!Number.isInteger(recordId) || !hasValidConfig()) {
                    return;
                }

                const previousRows = Array.isArray(state.data?.rows)
                    ? state.data.rows
                    : [];
                const previousIndex = previousRows.findIndex(
                    (row) => String(row?.id) === String(recordId),
                );
                const previousQuantity =
                    previousIndex >= 0
                        ? previousRows[previousIndex]?.quantity
                        : null;

                if (previousIndex >= 0) {
                    state.data = {
                        ...state.data,
                        rows: previousRows.map((row, index) =>
                            index === previousIndex
                                ? { ...row, quantity: String(nextQuantity) }
                                : row,
                        ),
                    };
                    state.transportError = "";
                    render();
                }

                try {
                    const response = await fetch(buildRecordUrl(recordId), {
                        method: "PATCH",
                        headers: { "Content-Type": "application/json" },
                        body: JSON.stringify({ quantity: nextQuantity }),
                    });
                    const raw = await response.text().catch(() => "");

                    if (response.status === 401) {
                        throw new Error(
                            "Servidor bloqueado. Entre com suas credenciais no host para liberar esse widget.",
                        );
                    }

                    if (!response.ok) {
                        throw new Error(
                            raw || `Falha ao atualizar record ${recordId}.`,
                        );
                    }

                    state.lastUpdate = new Date().toLocaleTimeString();
                    state.transportError = "";
                } catch (error) {
                    if (previousIndex >= 0) {
                        state.data = {
                            ...state.data,
                            rows: previousRows.map((row, index) =>
                                index === previousIndex
                                    ? { ...row, quantity: previousQuantity }
                                    : row,
                            ),
                        };
                    }
                    state.transportError = String(
                        error instanceof Error ? error.message : error,
                    );
                } finally {
                    render();
                }
            }

            app.addEventListener("lince-bridge-state", (event) => {
                if (!event.detail || typeof event.detail !== "object") {
                    return;
                }

                applyHostMeta(event.detail.meta || null);
            });

            window.KanbanWidget = {
                persistUi,
                bodyFor,
                reconnect,
                toggleStream: toggleWidgetStream,
            };

            render();
            if (hasValidConfig() && streamEnabled()) {
                connectStream(true);
            }
    "#
}
