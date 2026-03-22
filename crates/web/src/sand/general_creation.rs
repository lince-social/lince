use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, html};

struct TableOverview {
    key: &'static str,
    label: &'static str,
    auth: &'static str,
    field_preview: &'static [&'static str],
    notes: &'static [&'static str],
}

const TABLES: [TableOverview; 9] = [
    TableOverview {
        key: "record",
        label: "record",
        auth: "Authenticated server user",
        field_preview: &["quantity:number", "head:string|null", "body:string|null"],
        notes: &["Empty object works", "id is forbidden"],
    },
    TableOverview {
        key: "view",
        label: "view",
        auth: "Authenticated server user",
        field_preview: &["name:string", "query:string"],
        notes: &[
            "Send name on create",
            "query defaults to SELECT * FROM record",
        ],
    },
    TableOverview {
        key: "frequency",
        label: "frequency",
        auth: "Authenticated server user",
        field_preview: &[
            "quantity:number",
            "name:string",
            "day_week:int|null",
            "months:number",
            "days:number",
            "seconds:number",
            "next_date:string",
            "finish_date:string|null",
            "catch_up_sum:int",
        ],
        notes: &["Empty object works", "day_week and finish_date can be null"],
    },
    TableOverview {
        key: "karma_condition",
        label: "karma_condition",
        auth: "Authenticated server user",
        field_preview: &["quantity:int", "name:string", "condition:string"],
        notes: &["condition is required in practice", "id is forbidden"],
    },
    TableOverview {
        key: "karma_consequence",
        label: "karma_consequence",
        auth: "Authenticated server user",
        field_preview: &["quantity:int", "name:string", "consequence:string"],
        notes: &["consequence is required in practice", "id is forbidden"],
    },
    TableOverview {
        key: "karma",
        label: "karma",
        auth: "Authenticated server user",
        field_preview: &[
            "quantity:int",
            "name:string",
            "condition_id:int",
            "operator:string",
            "consequence_id:int",
        ],
        notes: &[
            "condition_id and consequence_id must exist",
            "operator has no frontend enum guard",
        ],
    },
    TableOverview {
        key: "configuration",
        label: "configuration",
        auth: "Authenticated server user",
        field_preview: &[
            "quantity:int|null",
            "name:string",
            "language:string|null",
            "timezone:int|null",
            "style:string|null",
            "show_command_notifications:bool|0|1",
            "command_notification_seconds:number",
            "delete_confirmation:bool|0|1",
            "error_toast_seconds:number",
            "keybinding_mode:int",
        ],
        notes: &[
            "Send name on create",
            "boolean fields accept true/false or 0/1",
        ],
    },
    TableOverview {
        key: "app_user",
        label: "app_user",
        auth: "Admin only on create",
        field_preview: &[
            "name:string",
            "username:string",
            "password:string",
            "role_id:int?",
        ],
        notes: &[
            "password_hash and role are forbidden",
            "role_id defaults to the lince role",
        ],
    },
    TableOverview {
        key: "role",
        label: "role",
        auth: "Admin only",
        field_preview: &["name:string"],
        notes: &["Empty object is rejected", "name must be unique"],
    },
];

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: "general-creation.html",
        lang: "en",
        manifest: PackageManifest {
            icon: "⊕".into(),
            title: "General Creation".into(),
            author: "Lince Labs".into(),
            version: "0.1.0".into(),
            description: "Official creator for the backend API tables exposed through the host.".into(),
            details: "Choose a server, pick one of the API-exposed tables, inspect the writable schema and exceptions, then POST a JSON object through the host-mediated backend route.".into(),
            initial_width: 6,
            initial_height: 5,
            permissions: vec!["bridge_state".into(), "write_table".into()],
        },
        head_links: vec![],
        inline_styles: vec![r#"
      :root {
        color-scheme: dark;
        --bg: #0f1216;
        --panel: #171c23;
        --panel-soft: #1d242d;
        --panel-alt: #131820;
        --line: rgba(255, 255, 255, 0.08);
        --line-strong: rgba(255, 255, 255, 0.14);
        --text: #edf2f8;
        --muted: #95a2b1;
        --accent: #89b4ff;
        --accent-soft: rgba(137, 180, 255, 0.12);
        --ok: #88efbb;
        --warn: #f0c67f;
        --danger: #ff9ead;
        --mono: "IBM Plex Mono", "SFMono-Regular", monospace;
      }

      * { box-sizing: border-box; }

      html, body {
        min-height: 100%;
        margin: 0;
        background: transparent;
      }

      body {
        min-height: 100vh;
        padding: 14px;
        color: var(--text);
        background:
          radial-gradient(circle at top right, rgba(137, 180, 255, 0.08), transparent 32%),
          linear-gradient(180deg, rgba(16, 18, 23, 0.98), rgba(11, 13, 17, 0.98));
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
      }

      .app {
        min-height: calc(100vh - 28px);
        display: grid;
        grid-template-rows: auto auto auto auto 1fr auto;
        gap: 12px;
      }

      .panel {
        display: grid;
        gap: 10px;
        padding: 12px;
        border: 1px solid var(--line);
        border-radius: 16px;
        background: var(--panel);
      }

      .header {
        display: flex;
        justify-content: space-between;
        align-items: flex-start;
        gap: 12px;
      }

      .eyebrow {
        color: var(--muted);
        font-size: 0.68rem;
        font-weight: 600;
        letter-spacing: 0.16em;
        text-transform: uppercase;
      }

      .title {
        margin: 4px 0 0;
        font-size: 1.05rem;
        font-weight: 700;
        letter-spacing: -0.02em;
      }

      .copy {
        margin: 6px 0 0;
        color: var(--muted);
        font-size: 0.8rem;
        line-height: 1.5;
      }

      .status {
        display: inline-flex;
        align-items: center;
        gap: 7px;
        min-height: 34px;
        padding: 0 12px;
        border: 1px solid var(--line);
        border-radius: 999px;
        color: var(--muted);
        background: rgba(255, 255, 255, 0.04);
        font-size: 0.72rem;
        letter-spacing: 0.05em;
        text-transform: uppercase;
      }

      .status::before {
        content: "";
        width: 8px;
        height: 8px;
        border-radius: 999px;
        background: var(--warn);
        box-shadow: 0 0 0 6px rgba(240, 198, 127, 0.08);
      }

      .status[data-tone="ok"]::before {
        background: var(--ok);
        box-shadow: 0 0 0 6px rgba(136, 239, 187, 0.08);
      }

      .status[data-tone="error"]::before {
        background: var(--danger);
        box-shadow: 0 0 0 6px rgba(255, 158, 173, 0.08);
      }

      .meta {
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
      }

      .pill {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        min-height: 28px;
        padding: 0 10px;
        border-radius: 999px;
        border: 1px solid var(--line);
        color: var(--muted);
        background: rgba(255, 255, 255, 0.04);
        font-size: 0.7rem;
      }

      .controls {
        display: grid;
        grid-template-columns: minmax(0, 1fr) auto auto auto;
        gap: 10px;
        align-items: end;
      }

      .fieldGroup {
        display: grid;
        gap: 6px;
      }

      .label {
        color: var(--muted);
        font-family: var(--mono);
        font-size: 0.68rem;
        font-weight: 600;
        letter-spacing: 0.12em;
        text-transform: uppercase;
      }

      .select,
      .button,
      .textarea {
        width: 100%;
        border: 1px solid var(--line);
        border-radius: 12px;
        background: var(--panel-soft);
        color: var(--text);
        font: inherit;
      }

      .select,
      .textarea {
        padding: 11px 12px;
      }

      .textarea {
        min-height: 220px;
        resize: vertical;
        font-family: var(--mono);
        font-size: 0.74rem;
        line-height: 1.58;
      }

      .button {
        min-height: 40px;
        padding: 0 12px;
        cursor: pointer;
        transition: border-color 160ms ease, background 160ms ease, color 160ms ease;
      }

      .button:hover {
        border-color: var(--line-strong);
        background: #242d38;
      }

      .button--accent {
        border-color: rgba(137, 180, 255, 0.26);
        background: var(--accent-soft);
        color: #dce9ff;
        font-weight: 600;
      }

      .button--ghost {
        color: var(--muted);
      }

      .overviewGrid,
      .detailGrid {
        display: grid;
        gap: 10px;
      }

      .overviewGrid {
        grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
      }

      .overviewCard,
      .detailCard {
        display: grid;
        gap: 8px;
        padding: 10px;
        border: 1px solid var(--line);
        border-radius: 14px;
        background: var(--panel-alt);
      }

      .overviewCardHeader {
        display: flex;
        justify-content: space-between;
        gap: 10px;
        align-items: baseline;
      }

      .overviewCardTitle,
      .detailCardTitle {
        font-size: 0.8rem;
        font-weight: 700;
        letter-spacing: 0.01em;
      }

      .tag {
        display: inline-flex;
        align-items: center;
        min-height: 22px;
        padding: 0 8px;
        border-radius: 999px;
        border: 1px solid var(--line);
        color: var(--muted);
        font-size: 0.66rem;
        white-space: nowrap;
      }

      .overviewList,
      .detailList {
        margin: 0;
        padding-left: 1rem;
        color: var(--soft, #d3dbe5);
        font-size: 0.74rem;
        line-height: 1.5;
      }

      .overviewList li + li,
      .detailList li + li {
        margin-top: 4px;
      }

      .detailGrid {
        grid-template-columns: repeat(2, minmax(0, 1fr));
      }

      .fieldRow {
        display: grid;
        gap: 4px;
        padding: 8px 0;
        border-top: 1px solid rgba(255, 255, 255, 0.05);
      }

      .fieldRow:first-child {
        border-top: 0;
        padding-top: 0;
      }

      .fieldName {
        font-family: var(--mono);
        font-size: 0.72rem;
        font-weight: 600;
      }

      .fieldMeta {
        color: var(--muted);
        font-size: 0.72rem;
        line-height: 1.45;
      }

      .requestMeta {
        display: grid;
        grid-template-columns: minmax(0, 1fr) auto;
        gap: 10px;
        align-items: center;
      }

      .endpoint {
        min-height: 42px;
        padding: 10px 12px;
        border: 1px solid var(--line);
        border-radius: 12px;
        background: rgba(255, 255, 255, 0.03);
        color: var(--muted);
        font-family: var(--mono);
        font-size: 0.72rem;
        overflow: auto;
      }

      pre {
        margin: 0;
        min-height: 0;
        overflow: auto;
        padding: 12px;
        border: 1px solid var(--line);
        border-radius: 14px;
        background: rgba(10, 12, 15, 0.66);
        color: var(--text);
        font-family: var(--mono);
        font-size: 0.72rem;
        line-height: 1.55;
        white-space: pre-wrap;
        word-break: break-word;
      }

      .hint {
        color: var(--muted);
        font-size: 0.75rem;
        line-height: 1.5;
      }

      @media (max-width: 860px) {
        .controls,
        .detailGrid,
        .requestMeta {
          grid-template-columns: 1fr;
        }
      }
    "#],
        body: body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(script())],
    }
}

fn body() -> Markup {
    let header_pills = [
        "All writes go through /host/integrations/servers/{server_id}/table/{table}.",
        "Unknown or non-writable keys are rejected by the backend.",
        "This widget only creates rows. It does not patch or delete them.",
    ];

    html! {
        div id="app" class="app" data-lince-bridge-root {
            section class="panel" {
                div class="header" {
                    div {
                        div class="eyebrow" { "Official Sand" }
                        h1 class="title" { "General Creation" }
                        p class="copy" {
                            "Create rows on the backend API tables that Lince actually exposes. "
                            "The schema panel reflects writable fields and the exception panel lists the backend rules you should expect."
                        }
                    }
                    div id="status" class="status" data-tone="idle" { "Waiting for server" }
                }
                div class="meta" {
                    @for pill in header_pills {
                        span class="pill" { (pill) }
                    }
                }
            }

            section class="panel" {
                div class="controls" {
                    div class="fieldGroup" {
                        label class="label" for="table-select" { "Table" }
                        select id="table-select" class="select" {
                            @for table in TABLES {
                                option value=(table.key) { (table.label) }
                            }
                        }
                    }
                    button id="example-button" class="button button--ghost" type="button" { "Use example" }
                    button id="clear-button" class="button button--ghost" type="button" { "Clear draft" }
                    button id="create-button" class="button button--accent" type="button" { "Create row" }
                }
                div class="requestMeta" {
                    div class="fieldGroup" {
                        div class="label" { "Endpoint" }
                        div id="endpoint" class="endpoint" { "Configure a server in the host first." }
                    }
                    div class="hint" id="server-hint" {
                        "This widget uses the card's configured server_id. No view_id is required."
                    }
                }
            }

            section class="panel" {
                div class="label" { "Backend Tables Exposed Here" }
                div class="overviewGrid" {
                    @for table in TABLES {
                        article class="overviewCard" {
                            div class="overviewCardHeader" {
                                div class="overviewCardTitle" { (table.label) }
                                span class="tag" { (table.auth) }
                            }
                            ul class="overviewList" {
                                @for field in table.field_preview {
                                    li { (field) }
                                }
                            }
                            ul class="overviewList" {
                                @for note in table.notes {
                                    li { (note) }
                                }
                            }
                        }
                    }
                }
            }

            section class="panel" {
                div class="detailGrid" {
                    article class="detailCard" {
                        div class="detailCardTitle" { "Writable fields" }
                        div id="fields-panel" {}
                    }
                    article class="detailCard" {
                        div class="detailCardTitle" { "Exceptions and backend rules" }
                        ul class="detailList" id="exceptions-panel" {}
                    }
                }
            }

            section class="panel" {
                div class="fieldGroup" {
                    label class="label" for="payload-input" { "JSON payload" }
                    textarea id="payload-input" class="textarea" spellcheck="false" {}
                }
            }

            section class="panel" {
                div class="detailCardTitle" { "Response" }
                pre id="result-output" { "-- waiting" }
            }
        }
    }
}

fn script() -> &'static str {
    r#"
      const TABLES = {
        record: {
          label: "record",
          auth: "Authenticated server user",
          fields: [
            { name: "quantity", kind: "number", required: false, nullable: false, note: "Defaults to 1." },
            { name: "head", kind: "string", required: false, nullable: true, note: "Null is accepted." },
            { name: "body", kind: "string", required: false, nullable: true, note: "Null is accepted." }
          ],
          exceptions: [
            "The id field is not writable.",
            "Unknown or non-writable fields are rejected.",
            "Empty object is valid here and becomes INSERT DEFAULT VALUES."
          ],
          example: {
            quantity: -1,
            head: "Need ride to clinic",
            body: "Need transport on Monday morning"
          }
        },
        view: {
          label: "view",
          auth: "Authenticated server user",
          fields: [
            { name: "name", kind: "string", required: true, nullable: false, note: "Send this on create. The database requires it." },
            { name: "query", kind: "string", required: false, nullable: false, note: "Defaults to SELECT * FROM record if omitted." }
          ],
          exceptions: [
            "The id field is not writable.",
            "Unknown or non-writable fields are rejected.",
            "Avoid empty object here because name has no database default."
          ],
          example: {
            name: "Open needs",
            query: "SELECT * FROM record WHERE quantity < 0 ORDER BY id DESC"
          }
        },
        frequency: {
          label: "frequency",
          auth: "Authenticated server user",
          fields: [
            { name: "quantity", kind: "number", required: false, nullable: false, note: "Defaults to 1." },
            { name: "name", kind: "string", required: false, nullable: false, note: "Defaults to Frequency." },
            { name: "day_week", kind: "integer", required: false, nullable: true, note: "Null is accepted." },
            { name: "months", kind: "number", required: false, nullable: false, note: "Defaults to 0." },
            { name: "days", kind: "number", required: false, nullable: false, note: "Defaults to 0." },
            { name: "seconds", kind: "number", required: false, nullable: false, note: "Defaults to 0." },
            { name: "next_date", kind: "string", required: false, nullable: false, note: "Defaults to CURRENT_TIMESTAMP." },
            { name: "finish_date", kind: "string", required: false, nullable: true, note: "Null is accepted." },
            { name: "catch_up_sum", kind: "integer", required: false, nullable: false, note: "Defaults to 0." }
          ],
          exceptions: [
            "The id field is not writable.",
            "Unknown or non-writable fields are rejected.",
            "Empty object is valid here because the database supplies defaults."
          ],
          example: {
            name: "Weekly review",
            day_week: 5,
            days: 7,
            catch_up_sum: 1
          }
        },
        karma_condition: {
          label: "karma_condition",
          auth: "Authenticated server user",
          fields: [
            { name: "quantity", kind: "integer", required: false, nullable: false, note: "Defaults to 1." },
            { name: "name", kind: "string", required: false, nullable: false, note: "Defaults to Condition." },
            { name: "condition", kind: "string", required: true, nullable: false, note: "No database default exists." }
          ],
          exceptions: [
            "The id field is not writable.",
            "Unknown or non-writable fields are rejected.",
            "In practice you must send condition."
          ],
          example: {
            name: "High stock",
            condition: "rq1 > 10"
          }
        },
        karma_consequence: {
          label: "karma_consequence",
          auth: "Authenticated server user",
          fields: [
            { name: "quantity", kind: "integer", required: false, nullable: false, note: "Defaults to 1." },
            { name: "name", kind: "string", required: false, nullable: false, note: "Defaults to Consequence." },
            { name: "consequence", kind: "string", required: true, nullable: false, note: "No database default exists." }
          ],
          exceptions: [
            "The id field is not writable.",
            "Unknown or non-writable fields are rejected.",
            "In practice you must send consequence."
          ],
          example: {
            name: "Refill warning",
            consequence: "rq1=-1"
          }
        },
        karma: {
          label: "karma",
          auth: "Authenticated server user",
          fields: [
            { name: "quantity", kind: "integer", required: false, nullable: false, note: "Defaults to 1." },
            { name: "name", kind: "string", required: false, nullable: false, note: "Defaults to Karma." },
            { name: "condition_id", kind: "integer", required: true, nullable: false, note: "No database default exists." },
            { name: "operator", kind: "string", required: true, nullable: false, note: "The backend does not validate against a fixed enum here." },
            { name: "consequence_id", kind: "integer", required: true, nullable: false, note: "No database default exists." }
          ],
          exceptions: [
            "The id field is not writable.",
            "Unknown or non-writable fields are rejected.",
            "condition_id and consequence_id should point at existing rows."
          ],
          example: {
            name: "Buy more apples",
            condition_id: 1,
            operator: "and",
            consequence_id: 1
          }
        },
        configuration: {
          label: "configuration",
          auth: "Authenticated server user",
          fields: [
            { name: "quantity", kind: "integer", required: false, nullable: true, note: "Null is accepted." },
            { name: "name", kind: "string", required: true, nullable: false, note: "Send this on create. The database requires it." },
            { name: "language", kind: "string", required: false, nullable: true, note: "Null is accepted." },
            { name: "timezone", kind: "integer", required: false, nullable: true, note: "Null is accepted." },
            { name: "style", kind: "string", required: false, nullable: true, note: "Null is accepted." },
            { name: "show_command_notifications", kind: "boolean or 0/1", required: false, nullable: false, note: "Defaults to 0." },
            { name: "command_notification_seconds", kind: "number", required: false, nullable: false, note: "Defaults to -1." },
            { name: "delete_confirmation", kind: "boolean or 0/1", required: false, nullable: false, note: "Defaults to 1." },
            { name: "error_toast_seconds", kind: "number", required: false, nullable: false, note: "Defaults to 5." },
            { name: "keybinding_mode", kind: "integer", required: false, nullable: false, note: "Defaults to 0." }
          ],
          exceptions: [
            "The id field is not writable.",
            "Unknown or non-writable fields are rejected.",
            "Boolean fields accept true or false, and also 0 or 1."
          ],
          example: {
            name: "Web defaults",
            language: "en",
            show_command_notifications: true,
            delete_confirmation: true,
            error_toast_seconds: 5,
            keybinding_mode: 0
          }
        },
        app_user: {
          label: "app_user",
          auth: "Admin only on create",
          fields: [
            { name: "name", kind: "string", required: true, nullable: false, note: "Required by the backend." },
            { name: "username", kind: "string", required: true, nullable: false, note: "Required and must be unique." },
            { name: "password", kind: "string", required: true, nullable: false, note: "The backend hashes this; do not send password_hash." },
            { name: "role_id", kind: "integer", required: false, nullable: false, note: "Defaults to the lince role if omitted." }
          ],
          exceptions: [
            "The id field is not writable.",
            "The password_hash field is not writable.",
            "The role field is not writable.",
            "Only admin users can create app_user rows."
          ],
          example: {
            name: "Alex",
            username: "alex",
            password: "change-me",
            role_id: 2
          }
        },
        role: {
          label: "role",
          auth: "Admin only",
          fields: [
            { name: "name", kind: "string", required: true, nullable: false, note: "Required and should be unique." }
          ],
          exceptions: [
            "The id field is not writable.",
            "Unknown or non-writable fields are rejected.",
            "Only admin users can create role rows.",
            "Empty object is rejected because at least one writable field is required."
          ],
          example: {
            name: "operator"
          }
        }
      };

      const app = document.getElementById("app");
      const statusEl = document.getElementById("status");
      const tableSelect = document.getElementById("table-select");
      const endpointEl = document.getElementById("endpoint");
      const serverHintEl = document.getElementById("server-hint");
      const fieldsPanel = document.getElementById("fields-panel");
      const exceptionsPanel = document.getElementById("exceptions-panel");
      const payloadInput = document.getElementById("payload-input");
      const resultOutput = document.getElementById("result-output");
      const exampleButton = document.getElementById("example-button");
      const clearButton = document.getElementById("clear-button");
      const createButton = document.getElementById("create-button");

      const DEFAULT_TABLE = "record";
      const CARD_STATE_KEY = "generalCreation";
      let hostMeta = normalizeMeta(null);
      let cardState = normalizeCardState(null);
      let persistTimer = null;
      let bridgeBound = false;

      function normalizeMeta(rawMeta) {
        return {
          serverId: String(rawMeta?.serverId || "").trim(),
          mode: rawMeta?.mode === "edit" ? "edit" : "view"
        };
      }

      function normalizeCardState(rawState) {
        const scoped = rawState && typeof rawState === "object" ? rawState[CARD_STATE_KEY] : null;
        const rawDrafts = scoped?.drafts && typeof scoped.drafts === "object" ? scoped.drafts : {};
        const drafts = {};
        for (const [table, value] of Object.entries(rawDrafts)) {
          if (!TABLES[table]) {
            continue;
          }
          drafts[table] = typeof value === "string" ? value : JSON.stringify(value, null, 2);
        }
        return {
          selectedTable: TABLES[scoped?.selectedTable] ? scoped.selectedTable : DEFAULT_TABLE,
          drafts
        };
      }

      function activeTableKey() {
        return TABLES[tableSelect.value] ? tableSelect.value : DEFAULT_TABLE;
      }

      function activeTable() {
        return TABLES[activeTableKey()];
      }

      function getDraft(tableKey) {
        const raw = cardState.drafts[tableKey];
        if (typeof raw === "string" && raw.trim()) {
          return raw;
        }
        return JSON.stringify(TABLES[tableKey].example, null, 2);
      }

      function saveCardStateSoon() {
        if (persistTimer) {
          clearTimeout(persistTimer);
        }
        persistTimer = window.setTimeout(() => {
          persistTimer = null;
          window.LinceWidgetHost?.patchCardState?.({
            [CARD_STATE_KEY]: {
              selectedTable: cardState.selectedTable,
              drafts: cardState.drafts
            }
          });
        }, 120);
      }

      function applyBridgeDetail(detail) {
        hostMeta = normalizeMeta(detail?.meta || null);
        const nextCardState = normalizeCardState(detail?.meta?.cardState || null);
        const previousTable = cardState.selectedTable;
        cardState = nextCardState;
        tableSelect.value = cardState.selectedTable;
        if (previousTable !== cardState.selectedTable || payloadInput.dataset.table !== cardState.selectedTable) {
          ensureDraftLoaded(cardState.selectedTable);
        }
        syncUI();
      }

      function setStatus(text, tone) {
        statusEl.textContent = text;
        statusEl.dataset.tone = tone || "idle";
      }

      function escapeHtml(value) {
        return String(value)
          .replaceAll("&", "&amp;")
          .replaceAll("<", "&lt;")
          .replaceAll(">", "&gt;")
          .replaceAll('\"', "&quot;")
          .replaceAll("'", "&#39;");
      }

      function currentEndpoint() {
        if (!hostMeta.serverId) {
          return "Configure a server in the host first.";
        }
        return "/host/integrations/servers/" +
          encodeURIComponent(hostMeta.serverId) +
          "/table/" +
          encodeURIComponent(activeTableKey());
      }

      function renderFields() {
        const table = activeTable();
        fieldsPanel.innerHTML = table.fields.map((field) => `
          <div class="fieldRow">
            <div class="fieldName">${escapeHtml(field.name)}</div>
            <div class="fieldMeta">
              ${escapeHtml(field.kind)}
              ${field.required ? " · required on create" : " · optional"}
              ${field.nullable ? " · nullable" : ""}
            </div>
            <div class="fieldMeta">${escapeHtml(field.note)}</div>
          </div>
        `).join("");
      }

      function renderExceptions() {
        const table = activeTable();
        exceptionsPanel.innerHTML = [
          "Route: POST /api/table/" + table.label + " through the host proxy.",
          "Auth profile: " + table.auth + ".",
          ...table.exceptions
        ].map((line) => `<li>${escapeHtml(line)}</li>`).join("");
      }

      function syncUI() {
        const table = activeTable();
        endpointEl.textContent = currentEndpoint();
        serverHintEl.textContent = hostMeta.serverId
          ? `Using server_id ${hostMeta.serverId}. The backend enforces the real permissions and writable fields.`
          : "This widget needs a configured server. Use Configure on the card shell.";
        renderFields();
        renderExceptions();
        createButton.disabled = !hostMeta.serverId;
        if (!hostMeta.serverId) {
          setStatus("Waiting for server", "idle");
        } else {
          setStatus("Ready to create " + table.label, "ok");
        }
      }

      function ensureDraftLoaded(tableKey) {
        if (payloadInput.dataset.table === tableKey) {
          return;
        }
        payloadInput.dataset.table = tableKey;
        payloadInput.value = getDraft(tableKey);
      }

      function persistCurrentDraft() {
        const tableKey = activeTableKey();
        cardState.selectedTable = tableKey;
        cardState.drafts[tableKey] = payloadInput.value;
        saveCardStateSoon();
      }

      function parsePayload() {
        const raw = payloadInput.value.trim();
        if (!raw) {
          return {};
        }
        const parsed = JSON.parse(raw);
        if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
          throw new Error("Expected a JSON object payload.");
        }
        return parsed;
      }

      async function submitCreate() {
        if (!hostMeta.serverId) {
          setStatus("Configure server", "error");
          resultOutput.textContent = "Choose a server in the host configuration first.";
          return;
        }

        let payload;
        try {
          payload = parsePayload();
        } catch (error) {
          setStatus("Invalid JSON", "error");
          resultOutput.textContent = error instanceof Error ? error.message : "Invalid payload.";
          return;
        }

        const tableKey = activeTableKey();
        persistCurrentDraft();
        setStatus("Creating row", "idle");
        resultOutput.textContent = JSON.stringify({
          phase: "request",
          table: tableKey,
          endpoint: currentEndpoint(),
          payload
        }, null, 2);

        try {
          const response = await fetch(currentEndpoint(), {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(payload)
          });

          const raw = await response.text().catch(() => "");
          let parsed = raw;
          try {
            parsed = raw ? JSON.parse(raw) : null;
          } catch {
            // keep raw
          }

          if (response.status === 401) {
            throw new Error("Server locked. Authenticate that server in the host first.");
          }

          if (!response.ok) {
            throw new Error(typeof parsed === "string" ? parsed : JSON.stringify(parsed, null, 2));
          }

          setStatus("Create succeeded", "ok");
          resultOutput.textContent = JSON.stringify({
            phase: "response",
            table: tableKey,
            status: response.status,
            payload: parsed
          }, null, 2);
        } catch (error) {
          setStatus("Create failed", "error");
          resultOutput.textContent = error instanceof Error ? error.message : "Create failed.";
        }
      }

      tableSelect.addEventListener("change", () => {
        cardState.selectedTable = activeTableKey();
        ensureDraftLoaded(cardState.selectedTable);
        saveCardStateSoon();
        syncUI();
      });

      payloadInput.addEventListener("input", () => {
        persistCurrentDraft();
      });

      exampleButton.addEventListener("click", () => {
        const nextValue = JSON.stringify(activeTable().example, null, 2);
        payloadInput.value = nextValue;
        persistCurrentDraft();
      });

      clearButton.addEventListener("click", () => {
        payloadInput.value = "{}";
        persistCurrentDraft();
      });

      createButton.addEventListener("click", () => {
        void submitCreate();
      });

      app.addEventListener("lince-bridge-state", (event) => {
        if (!event.detail || typeof event.detail !== "object") {
          return;
        }

        applyBridgeDetail(event.detail);
      });

      function bindBridgeWhenReady() {
        if (bridgeBound || !window.LinceWidgetHost || typeof window.LinceWidgetHost.subscribe !== "function") {
          return false;
        }

        bridgeBound = true;
        window.LinceWidgetHost.subscribe((detail) => {
          applyBridgeDetail(detail);
        });
        window.LinceWidgetHost.requestState?.();
        return true;
      }

      tableSelect.value = cardState.selectedTable;
      ensureDraftLoaded(cardState.selectedTable);
      syncUI();
      resultOutput.textContent = JSON.stringify({
        selected_table: cardState.selectedTable,
        draft_source: "per-card state",
        server_id: hostMeta.serverId || null
      }, null, 2);
      if (!bindBridgeWhenReady()) {
        window.setTimeout(bindBridgeWhenReady, 0);
      }
    "#
}
