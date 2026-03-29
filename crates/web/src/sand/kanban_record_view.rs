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
            details: "Resolve o contrato do widget pela instancia do host, consome o stream oficial filtrado do Kanban e persiste ergonomia local do board no card.".into(),
            initial_width: 6,
            initial_height: 5,
            permissions: vec![
                "bridge_state".into(),
                "read_view_stream".into(),
                "write_records".into(),
                "write_table".into(),
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
        .widget id="app" data-lince-bridge-root {
            .widgetSurface {
                .panel {
                    .header {
                        #kanban-header-meta.headerMeta {
                            .headerTitle id="kanban-header-title" { "Kanban Record View" }
                            .headerSub id="kanban-header-sub" { "Waiting for widget contract..." }
                        }
                        .headerActions {
                            span.status id="kanban-connection-status" { "Waiting" }
                            #kanban-toolbar-state {}
                            .toolbarGroup {
                                button.toolbarBtn type="button" data-set-default-body-mode="head" { "All head" }
                                button.toolbarBtn type="button" data-set-default-body-mode="compact" { "All compact" }
                                button.toolbarBtn type="button" data-set-default-body-mode="full" { "All full" }
                            }
                            button.toolbarBtn type="button" id="kanban-toggle-updates" { "Pause updates" }
                            button.toolbarBtn type="button" id="kanban-open-filters" { "Filters" }
                            button.toolbarBtn.toolbarBtn--accent type="button" id="kanban-open-create" { "New task" }
                            button.toolbarBtn.toolbarBtn--paused type="button" id="kanban-toggle-stream" { "Disconnect widget" }
                            button.toolbarBtn.toolbarBtn--accent type="button" id="kanban-reconnect" { "Reconnect" }
                        }
                    }
                    p.small id="kanban-status-copy" {
                        "Waiting for the instance-aware contract and stream."
                    }
                }
                #kanban-active-filters {}
                #kanban-state-panel.panel hidden {
                    .header {
                        .headerMeta {
                            h2.warnTitle id="kanban-state-title" { "" }
                            p.small id="kanban-state-copy" { "" }
                        }
                    }
                    p.small id="kanban-state-detail" { "" }
                }
                #kanban-empty-or-error {}
                .boardWrap {
                    #kanban-columns.board {}
                }
            }
            .sheetOverlay id="kanban-filter-sheet" hidden {
                button.sheetBackdrop type="button" data-close-sheet="filter" {}
                .sheetPanel {
                    .sheetHeader {
                        .headerMeta {
                            .headerTitle { "Filters" }
                            .headerSub { "Each active row is AND. Multi-value rows use OR." }
                        }
                        .headerActions {
                            button.toolbarBtn type="button" data-clear-filters="true" { "Clear all" }
                            button.toolbarBtn type="button" data-close-sheet="filter" { "Close" }
                        }
                    }
                    #kanban-filter-sheet-body {}
                }
            }
            .sheetOverlay id="kanban-create-sheet" hidden {
                button.sheetBackdrop type="button" data-close-sheet="create" {}
                .sheetPanel {
                    .sheetHeader {
                        .headerMeta {
                            .headerTitle { "Create task" }
                            .headerSub { "A record becomes Kanban-ready when task_type is set." }
                        }
                        .headerActions {
                            button.toolbarBtn type="button" data-close-sheet="create" { "Close" }
                        }
                    }
                    #kanban-create-sheet-body {}
                }
            }
            .sheetOverlay id="kanban-edit-sheet" hidden {
                button.sheetBackdrop type="button" data-close-sheet="edit" {}
                .sheetPanel {
                    .sheetHeader {
                        .headerMeta {
                            .headerTitle { "Edit task" }
                            .headerSub { "Task metadata and relations are saved through the Kanban actions." }
                        }
                        .headerActions {
                            button.toolbarBtn type="button" data-close-sheet="edit" { "Close" }
                        }
                    }
                    #kanban-edit-sheet-body {}
                }
            }
            .sheetOverlay id="kanban-focus-sheet" hidden {
                button.sheetBackdrop type="button" data-close-focus="true" {}
                .sheetPanel {
                    .sheetHeader {
                        .headerMeta {
                            .headerTitle { "Task detail" }
                            .headerSub { "Focused record detail loaded from the host service." }
                        }
                        .headerActions {
                            button.toolbarBtn type="button" data-close-focus="true" { "Close" }
                        }
                    }
                    #kanban-focus-card {}
                }
            }
        }
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

            .toolbarGroup {
                display: inline-flex;
                align-items: center;
                gap: 6px;
                flex-wrap: wrap;
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

            .colResizeHandle {
                width: 14px;
                min-width: 14px;
                height: 26px;
                padding: 0;
                border-radius: 8px;
                color: var(--muted);
                cursor: ew-resize;
                touch-action: none;
            }

            .colResizeHandle.is-resizing {
                border-color: rgba(122, 162, 247, 0.4);
                color: #d9e5ff;
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

            .headButton {
                padding: 0;
                border: 0;
                background: transparent;
                color: inherit;
                text-align: left;
                min-height: 0;
            }

            .headButton:hover {
                background: transparent;
                border-color: transparent;
                color: var(--accent);
            }

            .tagRow {
                display: flex;
                flex-wrap: wrap;
                gap: 6px;
            }

            .parentLink {
                font-size: 10px;
                color: var(--muted);
            }

            .parentLink a,
            .kanban-focus-card a {
                color: var(--accent);
                text-decoration: none;
            }

            .parentLink a:hover,
            .kanban-focus-card a:hover {
                text-decoration: underline;
            }

            .sheetOverlay {
                position: absolute;
                inset: 0;
                display: flex;
                align-items: stretch;
                justify-content: flex-end;
                pointer-events: none;
            }

            .sheetOverlay[hidden] {
                display: none;
            }

            .sheetBackdrop {
                flex: 1 1 auto;
                border: 0;
                border-radius: 0;
                background: rgba(4, 7, 12, 0.58);
                pointer-events: auto;
            }

            .sheetPanel {
                position: relative;
                z-index: 1;
                width: min(560px, 100%);
                max-width: 100%;
                height: 100%;
                overflow: auto;
                display: flex;
                flex-direction: column;
                gap: 10px;
                padding: 12px;
                border-left: 1px solid var(--line);
                background:
                    linear-gradient(
                        180deg,
                        rgba(20, 24, 29, 0.99),
                        rgba(11, 13, 17, 0.99)
                    );
                pointer-events: auto;
            }

            .sheetHeader {
                display: flex;
                align-items: flex-start;
                justify-content: space-between;
                gap: 10px;
            }

            .sheetBody,
            .formGrid {
                display: grid;
                gap: 12px;
            }

            .formGrid {
                grid-template-columns: minmax(0, 1fr);
            }

            .fieldBlock {
                display: grid;
                gap: 6px;
            }

            .fieldLabel {
                color: var(--muted);
                font-size: 11px;
                font-weight: 600;
            }

            .field,
            .textarea,
            .select {
                width: 100%;
                min-height: 36px;
                padding: 8px 10px;
                border: 1px solid var(--line);
                border-radius: 10px;
                background: var(--panel-alt);
                color: var(--text);
                font: inherit;
            }

            .textarea {
                min-height: 104px;
                resize: vertical;
            }

            .chipBar {
                display: flex;
                flex-wrap: wrap;
                gap: 6px;
            }

            .chip {
                display: inline-flex;
                align-items: center;
                gap: 6px;
                padding: 5px 9px;
                border-radius: 999px;
                border: 1px solid var(--line);
                background: rgba(255, 255, 255, 0.04);
                font-size: 11px;
            }

            .chip button {
                min-height: 0;
                padding: 0;
                border: 0;
                background: transparent;
            }

            .checkGrid {
                display: grid;
                gap: 8px;
            }

            .checkRow {
                display: flex;
                align-items: center;
                gap: 8px;
                color: var(--soft);
                font-size: 12px;
            }

            .sheetActions {
                display: flex;
                flex-wrap: wrap;
                justify-content: flex-end;
                gap: 8px;
            }

            #kanban-active-filters {
                display: flex;
                flex-wrap: wrap;
                gap: 8px;
            }

            .kanban-focus-card {
                display: grid;
                gap: 14px;
            }

            .kanban-focus-card__header,
            .kanban-focus-card__children,
            .kanban-focus-card__comments,
            .kanban-focus-card__resources,
            .kanban-focus-card__worklog {
                display: grid;
                gap: 8px;
                padding: 12px;
                border-radius: 14px;
                border: 1px solid var(--line);
                background: rgba(255, 255, 255, 0.03);
            }

            .kanban-focus-card__title {
                margin: 0;
                font-size: 16px;
            }

            .kanban-focus-card__meta {
                display: flex;
                flex-wrap: wrap;
                gap: 8px;
                color: var(--muted);
                font-size: 11px;
            }

            .kanban-focus-card__body,
            .kanban-focus-card__comment p {
                margin: 0;
                white-space: pre-wrap;
            }

            .kanban-focus-card__image {
                display: block;
                width: 100%;
                max-height: 220px;
                object-fit: cover;
                border-radius: 12px;
                border: 1px solid var(--line);
                margin-bottom: 8px;
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
            const MIN_WIDTH = 180;
            const MAX_WIDTH = 420;
            const DEFAULT_BODY_MODE = "compact";
            const BODY_MODES = new Set(["head", "compact", "full"]);
            const app = document.getElementById("app");

            const elements = {
                headerMeta: document.getElementById("kanban-header-meta"),
                headerTitle: document.getElementById("kanban-header-title"),
                headerSub: document.getElementById("kanban-header-sub"),
                status: document.getElementById("kanban-connection-status"),
                toolbarState: document.getElementById("kanban-toolbar-state"),
                statusCopy: document.getElementById("kanban-status-copy"),
                statePanel: document.getElementById("kanban-state-panel"),
                stateTitle: document.getElementById("kanban-state-title"),
                stateCopy: document.getElementById("kanban-state-copy"),
                stateDetail: document.getElementById("kanban-state-detail"),
                emptyOrError: document.getElementById("kanban-empty-or-error"),
                activeFilters: document.getElementById("kanban-active-filters"),
                columns: document.getElementById("kanban-columns"),
                toggleUpdates: document.getElementById("kanban-toggle-updates"),
                openFilters: document.getElementById("kanban-open-filters"),
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
                formOptions: null,
                formOptionsPromise: null,
                focusDetail: null,
                pendingWorklogStops: [],
                activeWorklogIntervals: [],
                heartbeatTimer: null,
                draftFilters: emptyFilterState(),
                updatesPaused: false,
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
                    for (const [key, value] of Object.entries(rawUi.cardModes)) {
                        if (isBodyMode(value)) {
                            cardModes[String(key)] = String(value);
                        }
                    }
                }

                const focusedRecordId = Number(rawUi?.focusedRecordId);
                return {
                    lanes: nextLanes,
                    defaultBodyMode: isBodyMode(rawUi?.defaultBodyMode)
                        ? String(rawUi.defaultBodyMode)
                        : DEFAULT_BODY_MODE,
                    cardModes,
                    focusedRecordId:
                        Number.isInteger(focusedRecordId) && focusedRecordId > 0
                            ? focusedRecordId
                            : null,
                };
            }

            function storageKey() {
                return "lince.widget.kanban." + instanceId() + ".ui";
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

            function bodyModeFor(recordId) {
                return (
                    state.ui.cardModes[String(recordId)] ||
                    state.ui.defaultBodyMode
                );
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
                if (!state.contract || !streamEnabled()) {
                    return;
                }
                const delay = Math.min(15000, 1500 * Math.max(1, state.reconnectAttempt + 1));
                state.reconnectAttempt += 1;
                state.reconnectTimer = window.setTimeout(() => {
                    connectStream(false);
                }, delay);
            }

            function setShellState(title, copy, detail) {
                elements.stateTitle.textContent = title || "";
                elements.stateCopy.textContent = copy || "";
                elements.stateDetail.textContent = detail || "";
                elements.statePanel.hidden = false;
            }

            function clearShellState() {
                elements.statePanel.hidden = true;
                elements.stateTitle.textContent = "";
                elements.stateCopy.textContent = "";
                elements.stateDetail.textContent = "";
            }

            function setHeaderMetaFromContract() {
                const title =
                    state.viewMeta?.name ||
                    state.contract?.widget?.title ||
                    "Kanban Record View";
                const source = state.contract?.source || {};
                const viewId =
                    state.viewMeta?.view_id ??
                    state.contract?.source?.view_id ??
                    state.hostMeta.viewId;
                const query =
                    state.viewMeta?.query ||
                    (source.server_name
                        ? `server ${source.server_name} · view ${String(viewId || "")}`
                        : `view ${String(viewId || "")}`);

                elements.headerTitle.textContent = title;
                elements.headerSub.textContent = query;
            }

            function updateStatus() {
                const source = state.contract?.source || {};
                let label = "Waiting";
                let className = "status";
                let copy = "Waiting for the instance-aware Kanban stream.";

                if (!state.contract && state.loadingContract) {
                    label = "Loading";
                    copy = "Resolving the Kanban contract from the host.";
                } else if (source.requires_auth && source.authenticated === false) {
                    label = "Locked";
                    className += " is-error";
                    copy = "This widget needs the host login to reconnect the configured server.";
                } else if (state.hostMeta.streams.globalEnabled === false) {
                    label = "Paused globally";
                    className += " is-paused";
                    copy = "The board disabled streams globally for this workspace.";
                } else if (state.hostMeta.streams.cardEnabled === false) {
                    label = "Disconnected";
                    className += " is-paused";
                    copy = "This widget disconnected its live stream.";
                } else if (state.updatesPaused) {
                    label = "Paused updates";
                    className += " is-paused";
                    copy = "The connection is live, but incoming merges are paused locally.";
                } else if (state.connected) {
                    label = "Live";
                    className += " is-live";
                    copy = state.lastUpdate
                        ? "Live update received at " + state.lastUpdate + "."
                        : "Connected to the filtered Kanban stream.";
                } else if (state.transportError) {
                    label = "Offline";
                    className += " is-error";
                    copy = state.transportError;
                } else if (state.loadingStream) {
                    label = "Connecting";
                    copy = "Opening the instance-aware filtered stream.";
                }

                elements.status.className = className;
                elements.status.textContent = label;
                elements.statusCopy.textContent = copy;
                elements.toggleUpdates.textContent = state.updatesPaused
                    ? "Resume updates"
                    : "Pause updates";
                elements.toggleStream.textContent =
                    state.hostMeta.streams.cardEnabled === false
                        ? "Connect widget"
                        : "Disconnect widget";
                elements.toggleStream.classList.toggle(
                    "toolbarBtn--accent",
                    state.hostMeta.streams.cardEnabled === false,
                );
                elements.toggleStream.classList.toggle(
                    "toolbarBtn--paused",
                    state.hostMeta.streams.cardEnabled !== false,
                );
                elements.reconnect.disabled =
                    !state.contract ||
                    state.hostMeta.streams.enabled === false ||
                    (source.requires_auth && source.authenticated === false);
            }

            async function fetchContract() {
                state.loadingContract = true;
                updateStatus();
                try {
                    const response = await fetch(contractUrl(), {
                        cache: "no-store",
                    });
                    const payload = await response.json().catch(() => null);
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
                        return false;
                    }

                    state.contract = payload;
                    state.draftFilters = parseContractFilters(payload?.filters?.rows);
                    renderActiveFilters();
                    clearShellState();
                    setHeaderMetaFromContract();
                    if (
                        Array.isArray(payload?.filters?.rows) &&
                        payload.filters.rows.length
                    ) {
                        elements.toolbarState.dataset.filtersVersion = String(
                            payload.filters.filters_version || 0,
                        );
                    }
                    return true;
                } catch (error) {
                    state.contract = null;
                    setShellState(
                        "Kanban unavailable",
                        "The widget could not load its host contract.",
                        error instanceof Error ? error.message : String(error),
                    );
                    return false;
                } finally {
                    state.loadingContract = false;
                    updateStatus();
                }
            }

            async function loadFormOptions() {
                try {
                    state.formOptions = await postAction("load-form-options", {});
                    return state.formOptions;
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
                state.formOptionsPromise = loadFormOptions()
                    .catch(() => null)
                    .finally(() => {
                        state.formOptionsPromise = null;
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

            function applyLaneLayout() {
                for (const column of elements.columns.querySelectorAll(".col")) {
                    const key = String(column.dataset.col || "");
                    const lane = state.ui.lanes[key] || {
                        collapsed: false,
                        width: DEFAULT_WIDTH,
                    };
                    const width = lane.collapsed ? COLLAPSED_WIDTH : clampWidth(lane.width);
                    column.classList.toggle("is-collapsed", lane.collapsed);
                    column.style.width = width + "px";
                    column.style.minWidth = width + "px";
                    column.style.flexBasis = width + "px";
                    const list = column.querySelector(".list");
                    const tools = column.querySelector(".colTools");
                    const toggle = column.querySelector(".laneToggle");
                    if (list) {
                        list.style.display = lane.collapsed ? "none" : "";
                    }
                    if (tools) {
                        tools.style.display = lane.collapsed ? "none" : "";
                    }
                    if (toggle) {
                        toggle.textContent = lane.collapsed ? "+" : "-";
                    }
                }
            }

            function applyCardModes() {
                for (const card of elements.columns.querySelectorAll(".card")) {
                    const recordId = String(card.dataset.recordId || "");
                    const mode = bodyModeFor(recordId);
                    const body = card.querySelector("[data-card-body]");
                    const full = String(card.dataset.bodyFull || "");
                    const compact = String(card.dataset.bodyCompact || "");

                    for (const button of card.querySelectorAll("[data-card-body-mode]")) {
                        button.classList.toggle(
                            "is-active",
                            String(button.dataset.cardBodyMode || "") === mode,
                        );
                    }

                    if (!body) {
                        continue;
                    }

                    if (mode === "head") {
                        body.textContent = "";
                        body.style.display = "none";
                        body.classList.remove("is-full");
                    } else if (mode === "full") {
                        body.textContent = full;
                        body.style.display = "";
                        body.classList.add("is-full");
                    } else {
                        body.textContent = compact;
                        body.style.display = compact ? "" : "none";
                        body.classList.remove("is-full");
                    }
                }
            }

            function applyUiToDom() {
                applyLaneLayout();
                applyCardModes();
            }

            function escapeHtml(value) {
                return String(value == null ? "" : value).replace(
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
                const map = {
                    filter: elements.filterSheet,
                    create: elements.createSheet,
                    edit: elements.editSheet,
                };
                const sheet = map[name];
                if (sheet) {
                    sheet.hidden = false;
                }
            }

            function closeSheet(name) {
                const map = {
                    filter: elements.filterSheet,
                    create: elements.createSheet,
                    edit: elements.editSheet,
                };
                const sheet = map[name];
                if (sheet) {
                    sheet.hidden = true;
                }
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
                renderActiveFilters();
                await refreshRuntime(true);
            }

            function assigneeOptions() {
                return Array.isArray(state.formOptions?.assignees)
                    ? state.formOptions.assignees
                    : [];
            }

            function parentOptions() {
                return Array.isArray(state.formOptions?.parentRecords)
                    ? state.formOptions.parentRecords
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

            function formOptionsReady() {
                return state.formOptions || { assignees: [], categories: [], parentRecords: [] };
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
                const parentChoices = [
                    `<option value="">(no parent)</option>`,
                    ...options.parentRecords
                        .filter((record) => Number(record.id) !== Number(draft.recordId || 0))
                        .map(
                            (record) =>
                                `<option value="${record.id}" ${String(record.id) === currentParentId ? "selected" : ""}>#${record.id} ${escapeHtml(record.head || "Untitled")}</option>`,
                        ),
                ].join("");
                const warning =
                    mode === "edit" && !draft.taskType
                        ? `<p class="small">Saving without task_type will remove this record from the Kanban after refresh.</p>`
                        : "";

                return `
                    <div class="sheetBody">
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
                                <label class="fieldLabel" for="${mode}-parent-id">Parent</label>
                                <select class="select" id="${mode}-parent-id" name="parentId">${parentChoices}</select>
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
                });
            }

            async function openFilterSheet() {
                await ensureFormOptionsLoaded();
                renderFilterSheet();
                openSheet("filter");
            }

            async function openCreateSheet() {
                await ensureFormOptionsLoaded();
                renderCreateSheet();
                openSheet("create");
            }

            async function openEditSheet(recordId) {
                if (Number.isInteger(Number(recordId)) && Number(recordId) > 0) {
                    await loadRecordDetail(Number(recordId));
                } else if (!state.focusDetail && state.ui.focusedRecordId) {
                    await loadRecordDetail(state.ui.focusedRecordId);
                }
                await ensureFormOptionsLoaded();
                renderEditSheet();
                elements.focusSheet.hidden = true;
                openSheet("edit");
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

            async function deleteRecordFromUi(recordId) {
                if (!window.confirm("Delete this record?")) {
                    return;
                }
                await postAction("delete-record", { recordId: Number(recordId) });
                closeSheet("edit");
                closeFocus();
                state.focusDetail = null;
                await refreshRuntime(true);
            }

            function findComment(commentId) {
                return Array.isArray(state.focusDetail?.comments)
                    ? state.focusDetail.comments.find(
                          (entry) => Number(entry?.id || 0) === Number(commentId),
                      ) || null
                    : null;
            }

            async function createComment(recordId) {
                const body = window.prompt("Comment body", "");
                if (body == null) {
                    return;
                }
                await postAction("create-comment", {
                    recordId: Number(recordId),
                    body,
                });
                await loadRecordDetail(Number(recordId));
            }

            async function editComment(commentId) {
                const comment = findComment(commentId);
                const body = window.prompt(
                    "Edit comment",
                    String(comment?.body || ""),
                );
                if (body == null) {
                    return;
                }
                await postAction("update-comment", {
                    commentId: Number(commentId),
                    body,
                });
                if (state.focusDetail?.record_id) {
                    await loadRecordDetail(Number(state.focusDetail.record_id));
                }
            }

            async function deleteComment(commentId) {
                if (!window.confirm("Delete this comment?")) {
                    return;
                }
                await postAction("delete-comment", {
                    commentId: Number(commentId),
                });
                if (state.focusDetail?.record_id) {
                    await loadRecordDetail(Number(state.focusDetail.record_id));
                }
            }

            async function createResourceRef(recordId) {
                const resourcePath = window.prompt("Resource path", "");
                if (resourcePath == null || !String(resourcePath).trim()) {
                    return;
                }
                const resourceKind =
                    window.prompt("Resource kind", "image") || "image";
                const title = window.prompt("Title (optional)", "") || null;
                await postAction("create-resource-ref", {
                    recordId: Number(recordId),
                    provider: "bucket",
                    resourceKind: String(resourceKind).trim() || "image",
                    resourcePath: String(resourcePath).trim(),
                    title: title && title.trim() ? title.trim() : null,
                    position: Array.isArray(state.focusDetail?.resources)
                        ? state.focusDetail.resources.length
                        : null,
                });
                if (state.focusDetail?.record_id) {
                    await loadRecordDetail(Number(state.focusDetail.record_id));
                }
            }

            async function deleteResourceRef(resourceRefId) {
                if (!window.confirm("Remove this resource link?")) {
                    return;
                }
                await postAction("delete-resource-ref", {
                    resourceRefId: Number(resourceRefId),
                });
                if (state.focusDetail?.record_id) {
                    await loadRecordDetail(Number(state.focusDetail.record_id));
                }
            }

            async function startWorklog(recordId) {
                const note = window.prompt("Worklog note (optional)", "") || null;
                const outcome = await postAction("start-worklog", {
                    recordId: Number(recordId),
                    note: note && note.trim() ? note.trim() : null,
                });
                const intervalId = Number(outcome?.detail?.interval?.id || 0);
                if (intervalId > 0) {
                    upsertActiveWorklogInterval(Number(recordId), intervalId);
                }
                await refreshRuntime(true);
                await loadRecordDetail(Number(recordId));
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

            function persistUi(nextUi) {
                const normalized = normalizeUi(nextUi);
                const nextJson = serializeUi(normalized);
                state.ui = normalized;
                applyUiToDom();

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
                    window.LinceWidgetHost?.patchCardState?.({ ui: normalized });
                }, 140);
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
                applyUiToDom();
                return previousHtml;
            }

            function rollbackBoard(previousHtml) {
                if (typeof previousHtml !== "string") {
                    return;
                }
                elements.columns.innerHTML = previousHtml;
                applyUiToDom();
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
                patchHtml(elements.headerMeta, payload?.html?.header_meta);
                patchHtml(elements.toolbarState, payload?.html?.toolbar_state);
                patchHtml(elements.columns, payload?.html?.columns);
                patchHtml(elements.emptyOrError, payload?.html?.empty_or_error);
                applyUiToDom();
                updateStatus();
                if (state.ui.focusedRecordId && elements.focusSheet.hidden === false) {
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
                setHeaderMetaFromContract();
                updateStatus();
                if (!contractLoaded) {
                    stopStream();
                    return;
                }
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
                const payload = await postAction("load-record-detail", {
                    recordId: Number(recordId),
                });
                patchHtml(elements.focusCard, payload?.html);
                state.focusDetail = payload?.detail || null;
                elements.focusSheet.hidden = false;
                syncHeartbeatFromDetail();
                if (elements.editSheet.hidden === false) {
                    renderEditSheet();
                }
                persistUi({
                    ...state.ui,
                    focusedRecordId: Number(recordId),
                });
            }

            function closeFocus() {
                elements.focusSheet.hidden = true;
                elements.focusCard.innerHTML = "";
                closeSheet("edit");
                state.focusDetail = null;
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
                    applyUiToDom();
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

                    const openFiltersButton = event.target.closest("#kanban-open-filters");
                    if (openFiltersButton) {
                        event.preventDefault();
                        await openFilterSheet();
                        return;
                    }

                    const openCreateButton = event.target.closest("#kanban-open-create");
                    if (openCreateButton) {
                        event.preventDefault();
                        await openCreateSheet();
                        return;
                    }

                    const closeSheetButton = event.target.closest("[data-close-sheet]");
                    if (closeSheetButton) {
                        event.preventDefault();
                        closeSheet(String(closeSheetButton.dataset.closeSheet || ""));
                        return;
                    }

                    const clearFiltersButton = event.target.closest("[data-clear-filters]");
                    if (clearFiltersButton) {
                        event.preventDefault();
                        state.draftFilters = emptyFilterState();
                        renderActiveFilters();
                        if (elements.filterSheet.hidden === false) {
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

                    const submitRecordButton = event.target.closest("[data-submit-record]");
                    if (submitRecordButton) {
                        event.preventDefault();
                        await submitRecordForm(
                            String(submitRecordButton.dataset.submitRecord || "create"),
                        );
                        return;
                    }

                    const submitDeleteButton = event.target.closest("[data-submit-delete]");
                    if (submitDeleteButton) {
                        event.preventDefault();
                        await deleteRecordFromUi(Number(submitDeleteButton.dataset.submitDelete));
                        return;
                    }

                    const openEditButton = event.target.closest("[data-open-edit]");
                    if (openEditButton) {
                        event.preventDefault();
                        await openEditSheet(Number(openEditButton.dataset.openEdit));
                        return;
                    }

                    const deleteRecordButton = event.target.closest("[data-delete-record]");
                    if (deleteRecordButton) {
                        event.preventDefault();
                        await deleteRecordFromUi(Number(deleteRecordButton.dataset.deleteRecord));
                        return;
                    }

                    const createCommentButton = event.target.closest("[data-create-comment]");
                    if (createCommentButton) {
                        event.preventDefault();
                        await createComment(Number(createCommentButton.dataset.createComment));
                        return;
                    }

                    const editCommentButton = event.target.closest("[data-edit-comment]");
                    if (editCommentButton) {
                        event.preventDefault();
                        await editComment(Number(editCommentButton.dataset.editComment));
                        return;
                    }

                    const deleteCommentButton = event.target.closest("[data-delete-comment]");
                    if (deleteCommentButton) {
                        event.preventDefault();
                        await deleteComment(Number(deleteCommentButton.dataset.deleteComment));
                        return;
                    }

                    const addResourceButton = event.target.closest("[data-add-resource]");
                    if (addResourceButton) {
                        event.preventDefault();
                        await createResourceRef(Number(addResourceButton.dataset.addResource));
                        return;
                    }

                    const deleteResourceButton = event.target.closest("[data-delete-resource]");
                    if (deleteResourceButton) {
                        event.preventDefault();
                        await deleteResourceRef(
                            Number(deleteResourceButton.dataset.deleteResource),
                        );
                        return;
                    }

                    const startWorklogButton = event.target.closest("[data-start-worklog]");
                    if (startWorklogButton) {
                        event.preventDefault();
                        await startWorklog(Number(startWorklogButton.dataset.startWorklog));
                        return;
                    }

                    const stopWorklogButton = event.target.closest("[data-stop-worklog]");
                    if (stopWorklogButton) {
                        event.preventDefault();
                        await stopWorklog(
                            Number(stopWorklogButton.dataset.recordId),
                            Number(stopWorklogButton.dataset.stopWorklog),
                        );
                        return;
                    }

                    const closeButton = event.target.closest("[data-close-focus]");
                    if (closeButton) {
                        event.preventDefault();
                        closeFocus();
                        return;
                    }

                    const globalBodyModeButton = event.target.closest("[data-set-default-body-mode]");
                    if (globalBodyModeButton) {
                        event.preventDefault();
                        const mode = String(globalBodyModeButton.dataset.setDefaultBodyMode || "");
                        if (isBodyMode(mode)) {
                            persistUi({
                                ...state.ui,
                                defaultBodyMode: mode,
                                cardModes: {},
                            });
                        }
                        return;
                    }

                    const laneToggle = event.target.closest("[data-lane-toggle]");
                    if (laneToggle) {
                        event.preventDefault();
                        const key = String(laneToggle.dataset.laneToggle || "");
                        const lane = state.ui.lanes[key];
                        if (lane) {
                            persistUi({
                                ...state.ui,
                                lanes: {
                                    ...state.ui.lanes,
                                    [key]: { ...lane, collapsed: !lane.collapsed },
                                },
                            });
                        }
                        return;
                    }

                    const widthButton = event.target.closest("[data-col-width]");
                    if (widthButton) {
                        event.preventDefault();
                        const key = String(widthButton.dataset.colWidth || "");
                        const delta = Number(widthButton.dataset.widthDelta || 0);
                        const lane = state.ui.lanes[key];
                        if (lane) {
                            persistUi({
                                ...state.ui,
                                lanes: {
                                    ...state.ui.lanes,
                                    [key]: {
                                        ...lane,
                                        collapsed: false,
                                        width: clampWidth((lane.width || DEFAULT_WIDTH) + delta),
                                    },
                                },
                            });
                        }
                        return;
                    }

                    const bodyModeButton = event.target.closest("[data-card-body-mode]");
                    if (bodyModeButton) {
                        event.preventDefault();
                        const card = bodyModeButton.closest(".card");
                        const recordId = String(card?.dataset.recordId || "");
                        const mode = String(bodyModeButton.dataset.cardBodyMode || "");
                        if (recordId && isBodyMode(mode)) {
                            persistUi({
                                ...state.ui,
                                cardModes: {
                                    ...state.ui.cardModes,
                                    [recordId]: mode,
                                },
                            });
                        }
                        return;
                    }

                    const openFocus = event.target.closest("[data-open-focus]");
                    if (openFocus) {
                        event.preventDefault();
                        const recordId = Number(openFocus.dataset.openFocus);
                        if (Number.isInteger(recordId) && recordId > 0) {
                            await loadRecordDetail(recordId);
                        }
                        return;
                    }

                    const recordLink = event.target.closest("[data-record-link]");
                    if (recordLink) {
                        event.preventDefault();
                        const recordId = Number(recordLink.dataset.recordLink);
                        if (Number.isInteger(recordId) && recordId > 0) {
                            await loadRecordDetail(recordId);
                        }
                    }
                } catch (error) {
                    state.transportError =
                        error instanceof Error ? error.message : String(error);
                    updateStatus();
                }
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
                const { key, startX, startWidth } = state.resize;
                const nextWidth = clampWidth(startWidth + (event.clientX - startX));
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
                closeFocus,
            };

            updateStatus();
            setHeaderMetaFromContract();
            refreshRuntime(true).then(() => {
                if (state.ui.focusedRecordId) {
                    loadRecordDetail(state.ui.focusedRecordId).catch(() => {});
                }
            });
    "##
}
