pub(super) const INLINE_STYLES: [&str; 2] = [STYLE, crate::sand::shared_markdown::PREVIEW_STYLES];

const STYLE: &str =
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
                position: relative;
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

            .headerSubButton {
                min-height: 0;
                padding: 0;
                margin-top: 4px;
                border: 0;
                background: transparent;
                color: var(--muted);
                font-size: 11px;
                line-height: 1.4;
                text-align: left;
            }

            .headerSubButton:hover {
                background: transparent;
                color: var(--soft);
            }

            .headerQuery {
                margin: 8px 0 0;
                padding: 10px 12px;
                border: 1px solid var(--line);
                border-radius: 10px;
                background: rgba(255, 255, 255, 0.02);
                color: var(--muted);
                font-size: 10px;
                line-height: 1.45;
                white-space: pre-wrap;
                word-break: break-word;
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
                position: relative;
                flex: 0 0 auto;
                width: 260px;
                min-width: 260px;
                display: flex;
                flex-direction: column;
                align-self: flex-start;
                min-height: 0;
                border: 0;
                border-radius: 0;
                background: transparent;
                overflow: visible;
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
                margin-bottom: 8px;
                border: 1px solid var(--line);
                border-radius: 14px;
                background: rgba(20, 24, 29, 0.96);
            }

            .col.is-collapsed .colHead {
                align-items: center;
                justify-content: flex-start;
                padding: 12px 8px;
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

            .colResizeEdge {
                position: absolute;
                top: 0;
                bottom: 0;
                width: 10px;
                min-width: 10px;
                padding: 0;
                border: 0;
                border-radius: 0;
                background: transparent;
                cursor: ew-resize;
                z-index: 3;
            }

            .colResizeEdge--left {
                left: -5px;
            }

            .colResizeEdge--right {
                right: -5px;
            }

            .colResizeEdge.is-resizing,
            .colResizeEdge:hover {
                background: rgba(122, 162, 247, 0.12);
            }

            .list {
                display: flex;
                flex-direction: column;
                gap: 8px;
                padding: 0;
                min-height: 0;
                overflow: visible;
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
                overflow: hidden;
            }

            .body.is-full {
                max-height: none;
            }

            .body :first-child,
            .kanban-focus-card__body-preview :first-child {
                margin-top: 0;
            }

            .body :last-child,
            .kanban-focus-card__body-preview :last-child {
                margin-bottom: 0;
            }

            .body h1,
            .body h2,
            .body h3,
            .body h4,
            .kanban-focus-card__body-preview h1,
            .kanban-focus-card__body-preview h2,
            .kanban-focus-card__body-preview h3,
            .kanban-focus-card__body-preview h4 {
                margin: 0.95em 0 0.38em;
                line-height: 1.2;
                font-size: 1em;
            }

            .body p,
            .body ul,
            .body ol,
            .body blockquote,
            .body pre,
            .kanban-focus-card__body-preview p,
            .kanban-focus-card__body-preview ul,
            .kanban-focus-card__body-preview ol,
            .kanban-focus-card__body-preview blockquote,
            .kanban-focus-card__body-preview pre {
                margin: 0 0 0.65em;
            }

            .body ul,
            .body ol,
            .kanban-focus-card__body-preview ul,
            .kanban-focus-card__body-preview ol {
                padding-left: 1.1rem;
            }

            .body blockquote,
            .kanban-focus-card__body-preview blockquote {
                padding-left: 0.8rem;
                color: var(--muted);
                border-left: 2px solid rgba(255, 255, 255, 0.12);
            }

            .body code,
            .kanban-focus-card__body-preview code {
                padding: 0.08rem 0.28rem;
                border-radius: 6px;
                background: rgba(255, 255, 255, 0.06);
                font-family: "IBM Plex Mono", "SFMono-Regular", monospace;
                font-size: 0.92em;
            }

            .body pre,
            .kanban-focus-card__body-preview pre {
                overflow: auto;
            }

            .body pre code,
            .kanban-focus-card__body-preview pre code {
                display: block;
                padding: 0;
                background: transparent;
            }

            .body hr,
            .kanban-focus-card__body-preview hr {
                border: 0;
                border-top: 1px solid var(--line);
                margin: 0.9em 0;
            }

            .body a,
            .kanban-focus-card__body-preview a {
                color: var(--accent);
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

            #kanban-active-filters:empty,
            #kanban-toolbar-state:empty,
            #kanban-empty-or-error:empty {
                display: none;
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

            .kanban-focus-card__body-wrap {
                display: grid;
                gap: 8px;
                padding: 12px;
                border-radius: 14px;
                border: 1px solid var(--line);
                background: rgba(255, 255, 255, 0.03);
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

            [hidden] {
                display: none !important;
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
;
