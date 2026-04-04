use crate::{domain::lince_package::PackageManifest, sand::SandWidgetSource};
use maud::{Markup, html};

pub(crate) fn source() -> SandWidgetSource {
    SandWidgetSource {
        filename: r#"organ-management.html"#,
        lang: r#"en"#,
        manifest: PackageManifest {
            icon: r#"◎"#.into(),
            title: r#"Organ Management"#.into(),
            author: r#"Lince Labs"#.into(),
            version: r#"0.1.0"#.into(),
            description: r#"Hyper-specific control surface for organ profiles and connection state."#.into(),
            details: r#"Lists organs from /host/servers, surfaces auth and session metadata, and performs create, update, and delete through the host CRUD routes while persisting UI context per card."#.into(),
            initial_width: 6,
            initial_height: 6,
            requires_server: false,
            permissions: vec![r#"bridge_state"#.into()],
        },
        head_links: vec![],
        inline_styles: vec![r#"
      :root {
        color-scheme: dark;
        --bg: #0d1014;
        --panel: #141922;
        --panel-soft: #1a202a;
        --panel-strong: #202836;
        --line: rgba(255, 255, 255, 0.08);
        --line-strong: rgba(255, 255, 255, 0.16);
        --text: #eef3f8;
        --muted: #93a0b0;
        --accent: #b8d78c;
        --accent-soft: rgba(184, 215, 140, 0.12);
        --warn: #efc77d;
        --danger: #ff97a6;
        --ok: #82efb3;
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
          linear-gradient(180deg, rgba(11, 14, 18, 0.98), rgba(8, 10, 13, 0.98)),
          radial-gradient(circle at top right, rgba(184, 215, 140, 0.08), transparent 28%);
        font-family: "IBM Plex Sans", "Segoe UI", sans-serif;
      }

      button, input {
        font: inherit;
      }

      .app {
        min-height: calc(100vh - 28px);
        display: grid;
        grid-template-rows: auto auto minmax(0, 1fr) auto;
        gap: 12px;
      }

      .panel {
        border: 1px solid var(--line);
        border-radius: 18px;
        background: linear-gradient(180deg, rgba(20, 25, 34, 0.98), rgba(15, 19, 26, 0.98));
        box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.02);
      }

      .header {
        display: grid;
        gap: 10px;
        padding: 14px;
      }

      .headerRow, .toolbar, .summary, .actions, .footerMeta {
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: 8px;
      }

      .headerRow {
        justify-content: space-between;
        align-items: flex-start;
      }

      .eyebrow, .label, .microLabel {
        color: var(--muted);
        font-family: var(--mono);
        font-size: 0.67rem;
        font-weight: 600;
        letter-spacing: 0.14em;
        text-transform: uppercase;
      }

      .title {
        margin: 4px 0 0;
        font-size: 1.05rem;
        font-weight: 700;
        letter-spacing: -0.03em;
      }

      .copy {
        margin: 6px 0 0;
        color: var(--muted);
        font-size: 0.79rem;
        line-height: 1.5;
        max-width: 68ch;
      }

      .summary {
        justify-content: flex-end;
      }

      .metric {
        min-width: 88px;
        padding: 10px 12px;
        border: 1px solid var(--line);
        border-radius: 14px;
        background: rgba(255, 255, 255, 0.02);
      }

      .metricValue {
        display: block;
        margin-top: 3px;
        font-size: 1rem;
        font-weight: 700;
        letter-spacing: -0.03em;
      }

      .toolbar {
        padding: 0 14px 14px;
      }

      .search,
      .field,
      .button {
        min-height: 38px;
        border: 1px solid var(--line);
        border-radius: 12px;
        background: var(--panel-soft);
        color: var(--text);
      }

      .search,
      .field {
        width: 100%;
        padding: 0 12px;
      }

      .button {
        padding: 0 12px;
        cursor: pointer;
        transition: background 150ms ease, border-color 150ms ease, color 150ms ease;
      }

      .button:hover {
        background: var(--panel-strong);
        border-color: var(--line-strong);
      }

      .button--primary {
        border-color: rgba(184, 215, 140, 0.28);
        background: var(--accent-soft);
        color: var(--accent);
        font-weight: 700;
      }

      .button--danger {
        color: var(--danger);
      }

      .toolbarGrow {
        flex: 1 1 220px;
      }

      .workspace {
        min-height: 0;
        display: grid;
        grid-template-columns: minmax(240px, 1.05fr) minmax(280px, 1.4fr);
        gap: 12px;
      }

      .column {
        min-height: 0;
        display: grid;
      }

      .roster {
        min-height: 0;
        display: grid;
        grid-template-rows: auto minmax(0, 1fr);
      }

      .sectionHeader {
        display: flex;
        justify-content: space-between;
        align-items: center;
        gap: 10px;
        padding: 12px 14px;
        border-bottom: 1px solid var(--line);
      }

      .sectionTitle {
        margin: 0;
        font-size: 0.84rem;
        font-weight: 700;
        letter-spacing: -0.01em;
      }

      .sectionCopy {
        color: var(--muted);
        font-size: 0.74rem;
      }

      .list {
        min-height: 0;
        overflow: auto;
        padding: 10px;
        display: grid;
        gap: 8px;
      }

      .organCard {
        width: 100%;
        padding: 12px;
        text-align: left;
        display: grid;
        gap: 8px;
        border: 1px solid var(--line);
        border-radius: 16px;
        background: rgba(255, 255, 255, 0.02);
        color: inherit;
        cursor: pointer;
      }

      .organCard:hover,
      .organCard[data-active="true"] {
        border-color: var(--line-strong);
        background: rgba(255, 255, 255, 0.04);
      }

      .organCard[data-active="true"] {
        box-shadow: inset 0 0 0 1px rgba(184, 215, 140, 0.18);
      }

      .organTop {
        display: flex;
        justify-content: space-between;
        align-items: flex-start;
        gap: 10px;
      }

      .organName {
        font-size: 0.88rem;
        font-weight: 700;
        letter-spacing: -0.02em;
      }

      .organId, .organUrl, .mutedText, .emptyCopy {
        color: var(--muted);
        font-size: 0.74rem;
        line-height: 1.45;
      }

      .chips, .detailStats {
        display: flex;
        flex-wrap: wrap;
        gap: 6px;
      }

      .chip {
        display: inline-flex;
        align-items: center;
        gap: 6px;
        min-height: 24px;
        padding: 0 9px;
        border-radius: 999px;
        border: 1px solid var(--line);
        background: rgba(255, 255, 255, 0.03);
        color: var(--text);
        font-size: 0.69rem;
        white-space: nowrap;
      }

      .chip::before {
        content: "";
        width: 7px;
        height: 7px;
        border-radius: 999px;
        background: var(--muted);
      }

      .chip[data-tone="ok"]::before { background: var(--ok); }
      .chip[data-tone="warn"]::before { background: var(--warn); }
      .chip[data-tone="danger"]::before { background: var(--danger); }
      .chip[data-tone="accent"]::before { background: var(--accent); }

      .detail {
        min-height: 0;
        display: grid;
        grid-template-rows: auto auto auto auto minmax(0, 1fr);
      }

      .detailBody {
        min-height: 0;
        overflow: auto;
        padding: 14px;
        display: grid;
        gap: 14px;
      }

      .hero {
        display: grid;
        gap: 8px;
      }

      .heroTitle {
        display: flex;
        justify-content: space-between;
        align-items: flex-start;
        gap: 12px;
      }

      .heroName {
        margin: 0;
        font-size: 1rem;
        font-weight: 700;
        letter-spacing: -0.03em;
      }

      .mono {
        font-family: var(--mono);
      }

      .statsGrid {
        display: grid;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        gap: 10px;
      }

      .statCard {
        padding: 11px 12px;
        border: 1px solid var(--line);
        border-radius: 14px;
        background: rgba(255, 255, 255, 0.02);
      }

      .statValue {
        display: block;
        margin-top: 4px;
        font-size: 0.84rem;
        font-weight: 700;
      }

      .formGrid {
        display: grid;
        gap: 10px;
      }

      .fieldWrap {
        display: grid;
        gap: 6px;
      }

      .fieldHint {
        color: var(--muted);
        font-size: 0.71rem;
        line-height: 1.4;
      }

      .diagnostics {
        display: grid;
        gap: 10px;
      }

      pre {
        margin: 0;
        min-height: 120px;
        max-height: 240px;
        overflow: auto;
        padding: 12px;
        border: 1px solid var(--line);
        border-radius: 14px;
        background: rgba(8, 10, 14, 0.78);
        color: var(--text);
        font-family: var(--mono);
        font-size: 0.72rem;
        line-height: 1.5;
        white-space: pre-wrap;
        word-break: break-word;
      }

      .statusBar {
        display: flex;
        justify-content: space-between;
        gap: 10px;
        align-items: center;
        padding: 10px 14px 14px;
      }

      .statusText {
        min-height: 36px;
        display: inline-flex;
        align-items: center;
        padding: 0 12px;
        border-radius: 12px;
        border: 1px solid var(--line);
        background: rgba(255, 255, 255, 0.03);
        color: var(--muted);
        font-size: 0.75rem;
      }

      .statusText[data-tone="ok"] { color: var(--ok); }
      .statusText[data-tone="warn"] { color: var(--warn); }
      .statusText[data-tone="error"] { color: var(--danger); }

      .footerMeta {
        color: var(--muted);
        font-size: 0.72rem;
      }

      .emptyState {
        padding: 18px 16px;
        border: 1px dashed var(--line);
        border-radius: 16px;
        color: var(--muted);
      }

      @media (max-width: 880px) {
        .workspace {
          grid-template-columns: 1fr;
        }

        .summary {
          justify-content: flex-start;
        }
      }
    "#],
        body: body(),
        body_scripts: vec![crate::sand::WidgetScript::inline(r#"
      const CARD_STATE_KEY = "organManagement";
      const DEFAULT_FORM = { id: "", name: "", baseUrl: "" };
      const app = document.getElementById("app");
      const searchInput = document.getElementById("search-input");
      const refreshButton = document.getElementById("refresh-button");
      const newButton = document.getElementById("new-button");
      const saveButton = document.getElementById("save-button");
      const deleteButton = document.getElementById("delete-button");
      const resetButton = document.getElementById("reset-button");
      const rosterEl = document.getElementById("roster");
      const detailNameEl = document.getElementById("detail-name");
      const detailIdEl = document.getElementById("detail-id");
      const detailUrlEl = document.getElementById("detail-url");
      const detailChipsEl = document.getElementById("detail-chips");
      const detailAuthEl = document.getElementById("detail-auth");
      const detailSessionEl = document.getElementById("detail-session");
      const detailUserEl = document.getElementById("detail-user");
      const detailConnectedEl = document.getElementById("detail-connected");
      const detailErrorEl = document.getElementById("detail-error");
      const statusEl = document.getElementById("status");
      const requestEl = document.getElementById("request-preview");
      const responseEl = document.getElementById("response-preview");
      const totalEl = document.getElementById("metric-total");
      const connectedEl = document.getElementById("metric-connected");
      const lockedEl = document.getElementById("metric-locked");
      const formModeEl = document.getElementById("form-mode");
      const formIdEl = document.getElementById("field-id");
      const formNameEl = document.getElementById("field-name");
      const formBaseUrlEl = document.getElementById("field-base-url");
      const footerEl = document.getElementById("footer-meta");

      let bridgeBound = false;
      let persistTimer = null;
      let state = {
        hostMeta: normalizeMeta(null),
        ui: normalizeCardState(null),
        organs: [],
        loading: false,
        lastLoadedAt: null,
        lastResponse: "Waiting for the first refresh.",
      };

      function normalizeMeta(rawMeta) {
        return {
          mode: rawMeta?.mode === "edit" ? "edit" : "view",
          cardId: String(rawMeta?.cardId || "").trim(),
        };
      }

      function normalizeCardState(rawState) {
        const scoped = rawState && typeof rawState === "object" ? rawState[CARD_STATE_KEY] : null;
        return {
          search: typeof scoped?.search === "string" ? scoped.search : "",
          selectedId: typeof scoped?.selectedId === "string" ? scoped.selectedId : "",
          draft: {
            id: typeof scoped?.draft?.id === "string" ? scoped.draft.id : "",
            name: typeof scoped?.draft?.name === "string" ? scoped.draft.name : "",
            baseUrl: typeof scoped?.draft?.baseUrl === "string" ? scoped.draft.baseUrl : "",
          },
          draftMode: scoped?.draftMode === "create" ? "create" : "edit",
        };
      }

      function persistUiSoon() {
        if (persistTimer) {
          clearTimeout(persistTimer);
        }
        persistTimer = window.setTimeout(() => {
          persistTimer = null;
          window.LinceWidgetHost?.patchCardState?.({
            [CARD_STATE_KEY]: {
              search: state.ui.search,
              selectedId: state.ui.selectedId,
              draft: state.ui.draft,
              draftMode: state.ui.draftMode,
            },
          });
        }, 120);
      }

      function applyBridgeDetail(detail) {
        state.hostMeta = normalizeMeta(detail?.meta || null);
        const nextUi = normalizeCardState(detail?.meta?.cardState || null);
        const keepDraft = state.ui.draftMode === "create" && hasDraftContent(state.ui.draft);
        state.ui = keepDraft ? { ...nextUi, draft: state.ui.draft, draftMode: state.ui.draftMode } : nextUi;
        searchInput.value = state.ui.search;
        render();
      }

      function hasDraftContent(draft) {
        return Boolean(String(draft?.id || "").trim() || String(draft?.name || "").trim() || String(draft?.baseUrl || "").trim());
      }

      function formatSessionState(value) {
        if (!value) {
          return "No session metadata";
        }
        return String(value).replaceAll("_", " ");
      }

      function formatTimestamp(unixValue) {
        if (!Number.isFinite(unixValue)) {
          return "Never connected";
        }
        return new Date(unixValue * 1000).toLocaleString();
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

      function sortedOrgans() {
        const search = state.ui.search.trim().toLowerCase();
        const items = [...state.organs].sort((left, right) => left.name.localeCompare(right.name) || left.id.localeCompare(right.id));
        if (!search) {
          return items;
        }
        return items.filter((organ) => {
          const haystack = [organ.id, organ.name, organ.baseUrl, organ.sessionState, organ.usernameHint, organ.lastError]
            .filter(Boolean)
            .join(" ")
            .toLowerCase();
          return haystack.includes(search);
        });
      }

      function selectedOrgan() {
        return state.organs.find((organ) => organ.id === state.ui.selectedId) || null;
      }

      function ensureSelection() {
        if (state.ui.draftMode === "create") {
          return;
        }
        const found = selectedOrgan();
        if (found) {
          return;
        }
        const first = sortedOrgans()[0] || state.organs[0] || null;
        state.ui.selectedId = first ? first.id : "";
      }

      function syncDraftWithSelection() {
        if (state.ui.draftMode === "create" && hasDraftContent(state.ui.draft)) {
          return;
        }
        const organ = selectedOrgan();
        if (!organ) {
          state.ui.draftMode = "create";
          state.ui.draft = { ...DEFAULT_FORM };
          return;
        }
        state.ui.draftMode = "edit";
        state.ui.draft = {
          id: organ.id,
          name: organ.name,
          baseUrl: organ.baseUrl,
        };
      }

      function summaryMetrics() {
        const total = state.organs.length;
        let connected = 0;
        let locked = 0;
        for (const organ of state.organs) {
          if (organ.authenticated) {
            connected += 1;
          }
          if (organ.requiresAuth && !organ.authenticated) {
            locked += 1;
          }
        }
        return { total, connected, locked };
      }

      function chip(label, tone) {
        return `<span class="chip" data-tone="${escapeHtml(tone || "accent")}">${escapeHtml(label)}</span>`;
      }

      function organChips(organ) {
        const chips = [];
        chips.push(chip(organ.requiresAuth ? "Auth required" : "Open", organ.requiresAuth ? "warn" : "ok"));
        chips.push(chip(organ.authenticated ? "Connected" : "Disconnected", organ.authenticated ? "ok" : "danger"));
        chips.push(chip(formatSessionState(organ.sessionState), organ.sessionState === "connected" ? "ok" : organ.sessionState ? "warn" : "accent"));
        if (organ.usernameHint) {
          chips.push(chip("User " + organ.usernameHint, "accent"));
        }
        if (organ.lastError) {
          chips.push(chip("Error", "danger"));
        }
        return chips.join("");
      }

      function renderRoster() {
        const items = sortedOrgans();
        if (!items.length) {
          rosterEl.innerHTML = `
            <div class="emptyState">
              <div class="emptyCopy">No organs match the current filter. Clear the search or create a new organ profile.</div>
            </div>
          `;
          return;
        }

        rosterEl.innerHTML = items.map((organ) => `
          <button class="organCard" data-id="${escapeHtml(organ.id)}" data-active="${organ.id === state.ui.selectedId ? "true" : "false"}">
            <div class="organTop">
              <div>
                <div class="organName">${escapeHtml(organ.name)}</div>
                <div class="organId mono">${escapeHtml(organ.id)}</div>
              </div>
              <div class="microLabel">${organ.authenticated ? "live" : "staged"}</div>
            </div>
            <div class="organUrl">${escapeHtml(organ.baseUrl)}</div>
            <div class="chips">${organChips(organ)}</div>
          </button>
        `).join("");
      }

      function renderDetail() {
        const organ = selectedOrgan();
        const draft = state.ui.draft;
        const isCreate = state.ui.draftMode === "create";

        if (isCreate && !hasDraftContent(draft)) {
          detailNameEl.textContent = "New organ";
          detailIdEl.textContent = "ID will be slugified from the name when empty.";
          detailUrlEl.textContent = "Prepare a new organ profile and save through the host.";
          detailChipsEl.innerHTML = chip("Create mode", "accent");
          detailAuthEl.textContent = "Unknown until saved";
          detailSessionEl.textContent = "No session yet";
          detailUserEl.textContent = "No username";
          detailConnectedEl.textContent = "Never connected";
          detailErrorEl.textContent = "No transport errors reported.";
        } else if (organ) {
          detailNameEl.textContent = organ.name;
          detailIdEl.textContent = organ.id;
          detailUrlEl.textContent = organ.baseUrl;
          detailChipsEl.innerHTML = organChips(organ);
          detailAuthEl.textContent = organ.requiresAuth ? "Requires host-managed auth" : "No auth needed for current profile";
          detailSessionEl.textContent = formatSessionState(organ.sessionState);
          detailUserEl.textContent = organ.usernameHint || "No username hint";
          detailConnectedEl.textContent = formatTimestamp(organ.connectedAtUnix);
          detailErrorEl.textContent = organ.lastError || "No backend error recorded.";
        } else {
          detailNameEl.textContent = "No organ selected";
          detailIdEl.textContent = "Select an organ from the roster.";
          detailUrlEl.textContent = "Refresh to load the current host state.";
          detailChipsEl.innerHTML = chip("Idle", "accent");
          detailAuthEl.textContent = "No selection";
          detailSessionEl.textContent = "No selection";
          detailUserEl.textContent = "No selection";
          detailConnectedEl.textContent = "No selection";
          detailErrorEl.textContent = "No selection";
        }

        formModeEl.textContent = isCreate ? "Create organ" : "Edit organ";
        formIdEl.value = draft.id || "";
        formNameEl.value = draft.name || "";
        formBaseUrlEl.value = draft.baseUrl || "";
        deleteButton.disabled = isCreate || !organ || state.loading;
        saveButton.disabled = state.loading;
        resetButton.disabled = state.loading;
      }

      function renderMetrics() {
        const metrics = summaryMetrics();
        totalEl.textContent = String(metrics.total);
        connectedEl.textContent = String(metrics.connected);
        lockedEl.textContent = String(metrics.locked);
      }

      function renderFooter() {
        const bits = [
          state.loading ? "refreshing" : "steady",
          state.hostMeta.mode === "edit" ? "board edit mode" : "board view mode",
        ];
        if (state.lastLoadedAt) {
          bits.push("loaded " + state.lastLoadedAt.toLocaleTimeString());
        }
        footerEl.textContent = bits.join(" · ");
      }

      function render() {
        ensureSelection();
        syncDraftWithSelection();
        renderMetrics();
        renderRoster();
        renderDetail();
        renderFooter();
      }

      function requestPreviewFor(method, url, payload) {
        requestEl.textContent = JSON.stringify({ method, url, payload }, null, 2);
      }

      function patchDraft(nextDraft) {
        state.ui.draft = {
          id: String(nextDraft.id || ""),
          name: String(nextDraft.name || ""),
          baseUrl: String(nextDraft.baseUrl || ""),
        };
        persistUiSoon();
        render();
      }

      function beginCreate() {
        state.ui.selectedId = "";
        state.ui.draftMode = "create";
        state.ui.draft = { ...DEFAULT_FORM };
        requestPreviewFor("POST", "/host/servers", { id: null, name: "", base_url: "" });
        persistUiSoon();
        render();
        formNameEl.focus();
      }

      function resetDraft() {
        const organ = selectedOrgan();
        if (organ) {
          state.ui.draftMode = "edit";
          state.ui.draft = {
            id: organ.id,
            name: organ.name,
            baseUrl: organ.baseUrl,
          };
        } else {
          state.ui.draftMode = "create";
          state.ui.draft = { ...DEFAULT_FORM };
        }
        persistUiSoon();
        render();
      }

      async function loadOrgans() {
        state.loading = true;
        setStatus("Loading organ roster from host...", "warn");
        renderFooter();
        try {
          const response = await fetch("/host/servers");
          const raw = await response.text().catch(() => "");
          const parsed = raw ? JSON.parse(raw) : [];
          if (!response.ok) {
            throw new Error(typeof parsed === "string" ? parsed : JSON.stringify(parsed, null, 2));
          }
          state.organs = Array.isArray(parsed) ? parsed.map(normalizeOrgan).filter(Boolean) : [];
          state.lastLoadedAt = new Date();
          state.lastResponse = JSON.stringify(parsed, null, 2);
          responseEl.textContent = state.lastResponse;
          ensureSelection();
          if (!selectedOrgan() && state.organs.length) {
            state.ui.selectedId = state.organs[0].id;
          }
          if (!state.organs.length) {
            state.ui.draftMode = "create";
            state.ui.draft = { ...DEFAULT_FORM };
          }
          setStatus("Organ roster refreshed.", "ok");
          persistUiSoon();
          render();
        } catch (error) {
          const message = String(error instanceof Error ? error.message : error);
          state.lastResponse = message;
          responseEl.textContent = message;
          setStatus("Failed to load host organ state.", "error");
        } finally {
          state.loading = false;
          renderFooter();
        }
      }

      function normalizeOrgan(raw) {
        if (!raw || typeof raw !== "object") {
          return null;
        }
        return {
          id: String(raw.id || "").trim(),
          name: String(raw.name || "").trim(),
          baseUrl: String(raw.baseUrl || "").trim(),
          requiresAuth: Boolean(raw.requiresAuth),
          authenticated: Boolean(raw.authenticated),
          sessionState: typeof raw.sessionState === "string" ? raw.sessionState : null,
          usernameHint: String(raw.usernameHint || "").trim(),
          connectedAtUnix: Number.isFinite(raw.connectedAtUnix) ? raw.connectedAtUnix : null,
          lastError: String(raw.lastError || "").trim(),
        };
      }

      function buildPayload() {
        return {
          id: state.ui.draftMode === "create" ? optionalTrimmed(formIdEl.value) : undefined,
          name: formNameEl.value.trim(),
          base_url: formBaseUrlEl.value.trim(),
        };
      }

      function optionalTrimmed(value) {
        const trimmed = String(value || "").trim();
        return trimmed ? trimmed : null;
      }

      async function saveOrgan() {
        const payload = buildPayload();
        if (!payload.name) {
          setStatus("Name is required.", "error");
          return;
        }
        if (!payload.base_url) {
          setStatus("Base URL is required.", "error");
          return;
        }

        const selected = selectedOrgan();
        const isCreate = state.ui.draftMode === "create" || !selected;
        const method = isCreate ? "POST" : "PATCH";
        const url = isCreate ? "/host/servers" : "/host/servers/" + encodeURIComponent(selected.id);
        requestPreviewFor(method, url, payload);
        setStatus(isCreate ? "Creating organ..." : "Updating organ...", "warn");

        try {
          const response = await fetch(url, {
            method,
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(payload),
          });
          const raw = await response.text().catch(() => "");
          let parsed = raw;
          try {
            parsed = raw ? JSON.parse(raw) : null;
          } catch {
            // keep raw
          }
          responseEl.textContent = typeof parsed === "string" ? parsed : JSON.stringify(parsed, null, 2);
          if (!response.ok) {
            throw new Error(typeof parsed === "string" ? parsed : JSON.stringify(parsed, null, 2));
          }
          const nextId = parsed && typeof parsed === "object" ? String(parsed.id || "").trim() : "";
          if (nextId) {
            state.ui.selectedId = nextId;
          }
          state.ui.draftMode = "edit";
          setStatus(isCreate ? "Organ created." : "Organ updated.", "ok");
          await loadOrgans();
        } catch (error) {
          setStatus(isCreate ? "Create failed." : "Update failed.", "error");
          responseEl.textContent = String(error instanceof Error ? error.message : error);
        }
      }

      async function deleteOrgan() {
        const selected = selectedOrgan();
        if (!selected) {
          setStatus("Select an organ before deleting.", "error");
          return;
        }
        const url = "/host/servers/" + encodeURIComponent(selected.id);
        requestPreviewFor("DELETE", url, null);
        setStatus("Deleting organ...", "warn");
        try {
          const response = await fetch(url, { method: "DELETE" });
          const raw = await response.text().catch(() => "");
          responseEl.textContent = raw || JSON.stringify({ status: response.status }, null, 2);
          if (!response.ok) {
            throw new Error(raw || "Delete failed.");
          }
          state.ui.selectedId = "";
          state.ui.draftMode = "create";
          state.ui.draft = { ...DEFAULT_FORM };
          setStatus("Organ deleted.", "ok");
          await loadOrgans();
        } catch (error) {
          setStatus("Delete failed.", "error");
          responseEl.textContent = String(error instanceof Error ? error.message : error);
        }
      }

      searchInput.addEventListener("input", () => {
        state.ui.search = searchInput.value;
        persistUiSoon();
        renderRoster();
      });

      rosterEl.addEventListener("click", (event) => {
        const card = event.target instanceof Element ? event.target.closest(".organCard") : null;
        if (!card) {
          return;
        }
        const id = String(card.getAttribute("data-id") || "").trim();
        if (!id) {
          return;
        }
        state.ui.selectedId = id;
        state.ui.draftMode = "edit";
        persistUiSoon();
        render();
      });

      formIdEl.addEventListener("input", () => {
        state.ui.draftMode = "create";
        patchDraft({ ...state.ui.draft, id: formIdEl.value });
        requestPreviewFor("POST", "/host/servers", buildPayload());
      });
      formNameEl.addEventListener("input", () => {
        patchDraft({ ...state.ui.draft, name: formNameEl.value });
        requestPreviewFor(state.ui.draftMode === "create" ? "POST" : "PATCH", state.ui.draftMode === "create" ? "/host/servers" : "/host/servers/" + encodeURIComponent(state.ui.selectedId || "{server_id}"), buildPayload());
      });
      formBaseUrlEl.addEventListener("input", () => {
        patchDraft({ ...state.ui.draft, baseUrl: formBaseUrlEl.value });
        requestPreviewFor(state.ui.draftMode === "create" ? "POST" : "PATCH", state.ui.draftMode === "create" ? "/host/servers" : "/host/servers/" + encodeURIComponent(state.ui.selectedId || "{server_id}"), buildPayload());
      });

      refreshButton.addEventListener("click", () => {
        void loadOrgans();
      });
      newButton.addEventListener("click", beginCreate);
      resetButton.addEventListener("click", resetDraft);
      saveButton.addEventListener("click", () => {
        void saveOrgan();
      });
      deleteButton.addEventListener("click", () => {
        void deleteOrgan();
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

      searchInput.value = state.ui.search;
      requestPreviewFor("GET", "/host/servers", null);
      responseEl.textContent = state.lastResponse;
      render();
      if (!bindBridgeWhenReady()) {
        window.setTimeout(bindBridgeWhenReady, 0);
      }
      void loadOrgans();
    "#)],
    }
}

fn body() -> Markup {
    html! {
        main id="app" class="app" data-lince-bridge-root {
            section class="panel header" {
                div class="headerRow" {
                    div {
                        div class="eyebrow" { "Remote organ control" }
                        h1 class="title" { "Organ management" }
                        p class="copy" {
                            "Manage host organ profiles with CRUD, inspect connection health, and see exactly what the host knows about each session before a backend widget tries to use it."
                        }
                    }
                    div class="summary" {
                        div class="metric" {
                            span class="label" { "Organs" }
                            strong id="metric-total" class="metricValue" { "0" }
                        }
                        div class="metric" {
                            span class="label" { "Connected" }
                            strong id="metric-connected" class="metricValue" { "0" }
                        }
                        div class="metric" {
                            span class="label" { "Locked" }
                            strong id="metric-locked" class="metricValue" { "0" }
                        }
                    }
                }
                div class="toolbar" {
                    div class="toolbarGrow" {
                        input id="search-input" class="search mono" type="search" placeholder="Search by id, organ name, base URL, user, or last error";
                    }
                    button id="refresh-button" class="button" type="button" { "Refresh host state" }
                    button id="new-button" class="button button--primary" type="button" { "New organ" }
                }
            }

            section class="workspace" {
                div class="column" {
                    section class="panel roster" {
                        div class="sectionHeader" {
                            div {
                                h2 class="sectionTitle" { "Roster" }
                                div class="sectionCopy" { "Live host-side list with auth and session metadata." }
                            }
                        }
                        div id="roster" class="list" {}
                    }
                }

                div class="column" {
                    section class="panel detail" {
                        div class="sectionHeader" {
                            div {
                                h2 class="sectionTitle" { "Selected organ" }
                                div id="form-mode" class="sectionCopy" { "Edit organ" }
                            }
                        }

                        div class="detailBody" {
                            section class="hero" {
                                div class="heroTitle" {
                                    div {
                                        h3 id="detail-name" class="heroName" { "No organ selected" }
                                        div id="detail-id" class="mutedText mono" { "Select an organ from the roster." }
                                    }
                                    div class="microLabel" { "host metadata" }
                                }
                                div id="detail-url" class="organUrl mono" { "Refresh to load the current host state." }
                                div id="detail-chips" class="detailStats" {}
                            }

                            section class="statsGrid" {
                                div class="statCard" {
                                    div class="microLabel" { "Auth profile" }
                                    strong id="detail-auth" class="statValue" { "No selection" }
                                }
                                div class="statCard" {
                                    div class="microLabel" { "Session state" }
                                    strong id="detail-session" class="statValue" { "No selection" }
                                }
                                div class="statCard" {
                                    div class="microLabel" { "Username hint" }
                                    strong id="detail-user" class="statValue" { "No selection" }
                                }
                                div class="statCard" {
                                    div class="microLabel" { "Connected at" }
                                    strong id="detail-connected" class="statValue" { "No selection" }
                                }
                            }

                            section class="formGrid" {
                                div class="fieldWrap" {
                                    label class="label" for="field-id" { "Organ id" }
                                    input id="field-id" class="field mono" type="text" placeholder="Optional on create, host slugifies it";
                                    div class="fieldHint" { "On create, leave empty to derive the identifier from the organ name. On update, the current route id stays authoritative." }
                                }
                                div class="fieldWrap" {
                                    label class="label" for="field-name" { "Name" }
                                    input id="field-name" class="field" type="text" placeholder="North District Organ";
                                }
                                div class="fieldWrap" {
                                    label class="label" for="field-base-url" { "Base URL" }
                                    input id="field-base-url" class="field mono" type="text" placeholder="https://organ.example";
                                    div class="fieldHint" { "The host trims whitespace and strips a trailing slash before saving." }
                                }
                                div class="actions" {
                                    button id="save-button" class="button button--primary" type="button" { "Save organ" }
                                    button id="reset-button" class="button" type="button" { "Reset form" }
                                    button id="delete-button" class="button button--danger" type="button" { "Delete organ" }
                                }
                            }

                            section class="diagnostics" {
                                div class="fieldWrap" {
                                    div class="label" { "Last error" }
                                    div id="detail-error" class="fieldHint" { "No selection" }
                                }
                                div class="fieldWrap" {
                                    div class="label" { "Request preview" }
                                    pre id="request-preview" {}
                                }
                                div class="fieldWrap" {
                                    div class="label" { "Response / transport log" }
                                    pre id="response-preview" {}
                                }
                            }
                        }
                    }
                }
            }

            div class="statusBar" {
                div id="status" class="statusText" data-tone="idle" { "Waiting for the first refresh." }
                div id="footer-meta" class="footerMeta" { "steady" }
            }
        }
    }
}
