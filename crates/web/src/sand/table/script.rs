pub(super) fn script() -> String {
    r#"
      (() => {
        const frame = window.frameElement;
        const statusPill = document.getElementById("table-status");
        const tableDetails = document.getElementById("table-details");
        const contentShell = document.getElementById("content-shell");
        const bootstrap = document.getElementById("table-stream-bootstrap");
        const infoOpenButton = document.getElementById("info-open");
        const infoCloseButton = document.getElementById("info-close");
        const createOpenButton = document.getElementById("create-open");
        const createCloseButton = document.getElementById("create-close");
        const createPanel = document.getElementById("create-panel");
        const createTableSelect = document.getElementById("create-table-select");
        const createFields = document.getElementById("create-fields");
        const createSubmitButton = document.getElementById("create-submit");
        const tablePanel = document.getElementById("table-body");
        const toastLayer = document.getElementById("table-toasts");
        const serverId = String(frame?.dataset?.linceServerId || "").trim();
        const viewId = Number(String(frame?.dataset?.linceViewId || "").trim());
        const instanceId = String(frame?.dataset?.packageInstanceId || "preview").trim() || "preview";
        const settingsKey = "table-nerd/" + instanceId;

        const state = {
          controller: null,
          reconnectTimer: null,
          reconnectAttempt: 0,
          scrollTimer: null,
          editDebounceTimer: null,
          editSaveController: null,
          streamGeneration: 0,
          streamUrl: "",
          nerdMode: false,
          focusedRowIndex: 0,
          focusedColumnIndex: 0,
          focusedRowId: "",
          focusedColumnKey: "",
          editingCell: null,
          createOpen: false,
          infoOpen: false,
          createLoading: false,
          createSchemas: [],
          createSelectedTable: "record",
          createPreferredTable: "record",
          createDrafts: {},
        };

        function clamp(value, min, max) {
          return Math.min(max, Math.max(min, value));
        }

        function setStatus(text, tone = "idle") {
          if (!statusPill) {
            return;
          }

          statusPill.textContent = text;
          statusPill.dataset.tone = tone;
        }

        function showErrorToast(reason) {
          if (!toastLayer) {
            return;
          }

          const message = "couldnt save (" + String(reason || "unknown error").trim() + ")";
          const toast = document.createElement("div");
          toast.className = "toast";
          toast.textContent = message;
          toastLayer.appendChild(toast);

          window.setTimeout(() => {
            toast.remove();
          }, 5000);
        }

        function stopEditDebounceTimer() {
          if (state.editDebounceTimer) {
            window.clearTimeout(state.editDebounceTimer);
            state.editDebounceTimer = null;
          }
        }

        function stopEditSaveRequest() {
          if (state.editSaveController) {
            state.editSaveController.abort();
            state.editSaveController = null;
          }
        }

        function currentSourceTableName() {
          const tbodyTable = String(tablePanel?.querySelector("tbody[data-source-table]")?.dataset.sourceTable || "").trim();
          return tbodyTable;
        }

        function escapeCssSelector(value) {
          if (window.CSS?.escape) {
            return window.CSS.escape(String(value));
          }

          return String(value).replaceAll("\\", "\\\\").replaceAll('"', '\\"');
        }

        function currentFocusedRowElement() {
          return rows()[state.focusedRowIndex] || null;
        }

        function cellAt(rowIndex, columnIndex) {
          const row = rows()[rowIndex] || null;
          if (!row) {
            return null;
          }

          return row.querySelectorAll("td")[columnIndex] || null;
        }

        function currentFocusedCellElement() {
          return cellAt(state.focusedRowIndex, state.focusedColumnIndex);
        }

        function cellValueElement(cell) {
          if (!(cell instanceof HTMLElement)) {
            return null;
          }

          return cell.querySelector(".cellValue");
        }

        function parseFocusedRowId(cell) {
          if (!(cell instanceof HTMLElement)) {
            return "";
          }

          const row = cell.closest("tr");
          const directRowId = String(row?.dataset.rowId || "").trim();
          if (directRowId) {
            return directRowId;
          }

          const cellRowId = String(cell.dataset.rowId || "").trim();
          return cellRowId;
        }

        function readSettings() {
          try {
            const raw = window.localStorage?.getItem?.(settingsKey);
            if (!raw) {
              return null;
            }

            const parsed = JSON.parse(raw);
            if (!parsed || typeof parsed !== "object") {
              return null;
            }

            return {
              nerdMode: String(parsed.mode || "common") === "helix",
            };
          } catch {
            return null;
          }
        }

        function writeSettings() {
          try {
            window.localStorage?.setItem?.(
              settingsKey,
              JSON.stringify({
                mode: state.nerdMode ? "helix" : "common",
              }),
            );
          } catch {
            // ignore storage failures
          }
        }

        function currentModeSelect() {
          return document.getElementById("table-mode");
        }

        function syncModeSelect() {
          const select = currentModeSelect();
          if (!select) {
            return;
          }

          select.value = state.nerdMode ? "helix" : "common";
        }

        function currentViewSql() {
          return String(tableDetails?.querySelector("pre.codeBlock")?.textContent || "").trim();
        }

        function detectPreferredTableName() {
          const tbodyTable = String(tablePanel?.querySelector("tbody[data-source-table]")?.dataset.sourceTable || "").trim();
          if (tbodyTable) {
            return tbodyTable.toLowerCase();
          }

          const sql = currentViewSql();
          if (!sql) {
            return "record";
          }

          const match = sql.match(/\bfrom\s+(?:`([^`]+)`|\[([^\]]+)\]|"([^"]+)"|([a-zA-Z_][a-zA-Z0-9_]*))/i);
          const table = String((match && (match[1] || match[2] || match[3] || match[4])) || "").trim();
          return table ? table.toLowerCase() : "record";
        }

        function syncPanelVisibility() {
          if (tableDetails) {
            tableDetails.hidden = !state.infoOpen;
            tableDetails.setAttribute("aria-hidden", state.infoOpen ? "false" : "true");
          }

          if (contentShell) {
            contentShell.dataset.infoOpen = state.infoOpen ? "true" : "false";
          }

          if (infoOpenButton) {
            infoOpenButton.setAttribute("aria-expanded", state.infoOpen ? "true" : "false");
          }

          if (infoCloseButton) {
            infoCloseButton.disabled = !state.infoOpen;
          }
        }

        function refreshCreateSchemaIfNeeded() {
          if (!state.createOpen || state.createLoading) {
            return;
          }

          const preferredTable = detectPreferredTableName();
          if (!preferredTable || preferredTable === state.createPreferredTable) {
            return;
          }

          state.createPreferredTable = preferredTable;
          void loadCreateSchemas(preferredTable);
        }

        function buildSchemaUrl(preferredTable) {
          if (!serverId) {
            return "";
          }

          const table = String(preferredTable || "record").trim() || "record";
          return (
            "/host/integrations/servers/" +
            encodeURIComponent(serverId) +
            "/table/schema?preferred_table=" +
            encodeURIComponent(table)
          );
        }

        function schemaFieldInputKind(fieldKind) {
          switch (String(fieldKind || "")) {
            case "boolean":
              return "boolean";
            case "integer":
              return "integer";
            case "number":
              return "number";
            case "textarea":
              return "textarea";
            case "password":
              return "password";
            default:
              return "text";
          }
        }

        function escapeHtml(value) {
          return String(value)
            .replaceAll("&", "&amp;")
            .replaceAll("<", "&lt;")
            .replaceAll(">", "&gt;")
            .replaceAll('"', "&quot;")
            .replaceAll("'", "&#39;");
        }

        function currentCreateSchema() {
          return (
            state.createSchemas.find((schema) => schema.name === state.createSelectedTable) ||
            state.createSchemas[0] ||
            null
          );
        }

        function createDraftFor(tableName) {
          const draft = state.createDrafts[tableName];
          return draft && typeof draft === "object" ? draft : {};
        }

        function snapshotCreateDraft(tableName) {
          if (!createFields) {
            return;
          }

          const draft = {};
          createFields.querySelectorAll("[data-create-field-name]").forEach((element) => {
            if (!(element instanceof HTMLElement)) {
              return;
            }

            const fieldName = String(element.dataset.createFieldName || "").trim();
            const inputKind = String(element.dataset.createFieldKind || "text");
            if (!fieldName) {
              return;
            }

            if (inputKind === "boolean") {
              const value = String(element.value || "").trim();
              if (value) {
                draft[fieldName] = value;
              }
              return;
            }

            if (inputKind === "textarea" || inputKind === "text" || inputKind === "password") {
              const value = String(element.value || "");
              if (value.trim()) {
                draft[fieldName] = value;
              }
              return;
            }

            const value = String(element.value || "").trim();
            if (value) {
              draft[fieldName] = value;
            }
          });

          state.createDrafts[tableName] = draft;
        }

        function renderCreateFields() {
          if (!createFields) {
            return;
          }

          const schema = currentCreateSchema();
          if (!schema) {
            createFields.innerHTML = "";
            return;
          }

          const draft = createDraftFor(schema.name);
          createFields.innerHTML = schema.fields
            .map((field) => {
              const inputKind = schemaFieldInputKind(field.input_kind);
              const inputId = `create-${schema.name}-${field.name}`;
              const fieldValue = draft[field.name] ?? "";

              if (inputKind === "boolean") {
                const booleanValue = String(fieldValue ?? "").trim();
                return `
                  <div class="createField">
                    <label class="fieldLabel" for="${escapeHtml(inputId)}">${escapeHtml(field.name)}</label>
                    <select
                      id="${escapeHtml(inputId)}"
                      class="field field--select"
                      data-create-field-name="${escapeHtml(field.name)}"
                      data-create-field-kind="${escapeHtml(inputKind)}"
                    >
                      <option value=""${booleanValue ? "" : " selected"}></option>
                      <option value="true"${booleanValue === "true" ? " selected" : ""}>true</option>
                      <option value="false"${booleanValue === "false" ? " selected" : ""}>false</option>
                    </select>
                  </div>
                `;
              }

              const fieldClass = inputKind === "textarea" ? "field field--textarea" : "field";
              const inputType =
                inputKind === "password"
                  ? "password"
                  : inputKind === "integer" || inputKind === "number"
                    ? "number"
                    : "text";
              const extraAttrs = inputKind === "integer" ? ' step="1"' : "";
              if (inputKind === "textarea") {
                return `
                  <div class="createField">
                    <label class="fieldLabel" for="${escapeHtml(inputId)}">${escapeHtml(field.name)}</label>
                    <textarea
                      id="${escapeHtml(inputId)}"
                      class="${fieldClass}"
                      rows="3"
                      spellcheck="false"
                      data-create-field-name="${escapeHtml(field.name)}"
                      data-create-field-kind="${escapeHtml(inputKind)}"
                    >${escapeHtml(fieldValue)}</textarea>
                  </div>
                `;
              }

              return `
                <div class="createField">
                  <label class="fieldLabel" for="${escapeHtml(inputId)}">${escapeHtml(field.name)}</label>
                  <input
                    id="${escapeHtml(inputId)}"
                    class="${fieldClass}"
                    type="${escapeHtml(inputType)}"
                    ${extraAttrs}
                    autocomplete="off"
                    spellcheck="false"
                    value="${escapeHtml(fieldValue)}"
                    data-create-field-name="${escapeHtml(field.name)}"
                    data-create-field-kind="${escapeHtml(inputKind)}"
                  />
                </div>
              `;
            })
            .join("");

          if (createTableSelect) {
            createTableSelect.value = schema.name;
          }

          if (createSubmitButton) {
            createSubmitButton.disabled = !serverId || state.createLoading;
          }
        }

        function syncCreatePanelVisibility() {
          if (createPanel) {
            createPanel.hidden = !state.createOpen;
            createPanel.setAttribute("aria-hidden", state.createOpen ? "false" : "true");
          }

          if (contentShell) {
            contentShell.dataset.createOpen = state.createOpen ? "true" : "false";
          }

          if (createOpenButton) {
            createOpenButton.textContent = "Create";
            createOpenButton.disabled = !serverId;
            createOpenButton.setAttribute("aria-expanded", state.createOpen ? "true" : "false");
          }

          if (createCloseButton) {
            createCloseButton.disabled = !state.createOpen;
          }

          if (createTableSelect) {
            createTableSelect.disabled = !state.createOpen || state.createLoading;
          }

          if (createSubmitButton) {
            createSubmitButton.disabled = !state.createOpen || !serverId || state.createLoading;
          }
        }

        function renderCreateTableSelect() {
          if (!createTableSelect) {
            return;
          }

          const tables = Array.isArray(state.createSchemas) ? state.createSchemas : [];
          createTableSelect.innerHTML = tables
            .map(
              (schema) =>
                `<option value="${escapeHtml(schema.name)}">${escapeHtml(schema.name)}</option>`,
            )
            .join("");
          createTableSelect.value =
            tables.some((schema) => schema.name === state.createSelectedTable)
              ? state.createSelectedTable
              : tables[0]?.name || "record";
        }

        async function loadCreateSchemas(preferredTable) {
          if (!serverId) {
            setStatus("Configure server", "error");
            return;
          }

          state.createPreferredTable = String(preferredTable || "record").trim() || "record";
          state.createLoading = true;
          syncCreatePanelVisibility();
          setStatus("Loading schema", "loading");

          try {
            const response = await fetch(buildSchemaUrl(preferredTable), {
              headers: { Accept: "application/json" },
              cache: "no-store",
            });

            const raw = await response.text().catch(() => "");
            let payload = null;
            try {
              payload = raw ? JSON.parse(raw) : null;
            } catch {
              payload = null;
            }

            if (response.status === 401) {
              window.LinceWidgetHost?.invalidateServerAuth?.(serverId);
              throw new Error("Server locked. Authenticate that server in the host first.");
            }

            if (!response.ok) {
              throw new Error(
                (payload && typeof payload.error === "string" && payload.error) ||
                  raw ||
                  `Nao foi possivel ler o schema (${response.status}).`,
              );
            }

            const nextTables = Array.isArray(payload?.tables) ? payload.tables : [];
            state.createSchemas = nextTables;
            const nextSelectedTable = String(payload?.preferred_table || nextTables[0]?.name || "record")
              .trim()
              .toLowerCase() || "record";
            state.createPreferredTable = nextSelectedTable;
            state.createSelectedTable = nextSelectedTable;
            renderCreateTableSelect();
            renderCreateFields();
            setStatus("Ready to create " + nextSelectedTable, "ok");
          } catch (error) {
            setStatus("Schema failed", "error");
            console.error(error);
            state.createSchemas = [];
            state.createSelectedTable = "record";
            renderCreateTableSelect();
            renderCreateFields();
          } finally {
            state.createLoading = false;
            syncCreatePanelVisibility();
          }
        }

        function toggleCreatePanel(forceOpen) {
          const nextOpen = typeof forceOpen === "boolean" ? forceOpen : !state.createOpen;
          state.createOpen = nextOpen;
          if (nextOpen) {
            state.infoOpen = false;
          }
          syncPanelVisibility();
          syncCreatePanelVisibility();

          if (!nextOpen) {
            return;
          }

          renderCreateTableSelect();
          renderCreateFields();
          void loadCreateSchemas(detectPreferredTableName());
        }

        function openInfoPanel(forceOpen) {
          const nextOpen = typeof forceOpen === "boolean" ? forceOpen : true;
          state.infoOpen = nextOpen;
          if (nextOpen) {
            state.createOpen = false;
          }
          syncPanelVisibility();
          syncCreatePanelVisibility();
        }

        function closeCreatePanel() {
          state.createOpen = false;
          syncCreatePanelVisibility();
        }

        function closeInfoPanel() {
          state.infoOpen = false;
          syncPanelVisibility();
        }

        function buildCreateEndpoint(tableName) {
          if (!serverId) {
            return "";
          }

          const trimmed = String(tableName || "record").trim() || "record";
          if (trimmed === "organ") {
            return "/host/integrations/servers/" + encodeURIComponent(serverId) + "/organ";
          }
          return (
            "/host/integrations/servers/" +
            encodeURIComponent(serverId) +
            "/table/" +
            encodeURIComponent(trimmed)
          );
        }

        function readCreatePayload() {
          const schema = currentCreateSchema();
          if (!schema || !createFields) {
            return {};
          }

          const payload = {};
          createFields.querySelectorAll("[data-create-field-name]").forEach((element) => {
            if (!(element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement || element instanceof HTMLSelectElement)) {
              return;
            }

            const fieldName = String(element.dataset.createFieldName || "").trim();
            const fieldKind = String(element.dataset.createFieldKind || "text");
            if (!fieldName) {
              return;
            }

            if (fieldKind === "boolean") {
              const raw = String(element.value || "").trim();
              if (raw === "true") {
                payload[fieldName] = true;
              } else if (raw === "false") {
                payload[fieldName] = false;
              }
              return;
            }

            const raw = String(element.value || "");
            if (!raw.trim()) {
              return;
            }

            if (fieldKind === "integer") {
              const parsed = Number.parseInt(raw, 10);
              if (!Number.isNaN(parsed)) {
                payload[fieldName] = parsed;
              }
              return;
            }

            if (fieldKind === "number") {
              const parsed = Number(raw);
              if (!Number.isNaN(parsed)) {
                payload[fieldName] = parsed;
              }
              return;
            }

            payload[fieldName] = raw;
          });

          return payload;
        }

        async function submitCreate() {
          if (!serverId) {
            setStatus("Configure server", "error");
            return;
          }

          snapshotCreateDraft(state.createSelectedTable);
          const tableName = state.createSelectedTable || detectPreferredTableName() || "record";
          const endpoint = buildCreateEndpoint(tableName);
          const payload = readCreatePayload();
          if (!endpoint) {
            setStatus("Configure server", "error");
            return;
          }

          state.createLoading = true;
          syncCreatePanelVisibility();
          setStatus("Creating row", "loading");

          try {
            const response = await fetch(endpoint, {
              method: "POST",
              headers: { "Content-Type": "application/json" },
              body: JSON.stringify(payload),
              cache: "no-store",
            });

            const raw = await response.text().catch(() => "");
            let parsed = null;
            try {
              parsed = raw ? JSON.parse(raw) : null;
            } catch {
              parsed = null;
            }

            if (response.status === 401) {
              window.LinceWidgetHost?.invalidateServerAuth?.(serverId);
              throw new Error("Server locked. Authenticate that server in the host first.");
            }

            if (!response.ok) {
              throw new Error(
                (parsed && typeof parsed.error === "string" && parsed.error) ||
                  raw ||
                  `Create failed (${response.status}).`,
              );
            }

            setStatus("Created " + tableName, "live");
            window.TableWidget?.reconnect?.();
          } catch (error) {
            setStatus("Create failed", "error");
            if (error instanceof Error) {
              console.error(error);
            } else {
              console.error(new Error("Nao foi possivel criar a linha."));
            }
          } finally {
            state.createLoading = false;
            syncCreatePanelVisibility();
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

        function valueToCell(value) {
          if (value === null || value === undefined) {
            return { text: "", kind: "null" };
          }
          if (typeof value === "string") {
            return { text: value, kind: "string" };
          }
          if (typeof value === "number") {
            return { text: String(value), kind: "number" };
          }
          if (typeof value === "boolean") {
            return { text: String(value), kind: "boolean" };
          }

          try {
            return { text: JSON.stringify(value, null, 2), kind: "json" };
          } catch {
            return { text: String(value), kind: "json" };
          }
        }

        function prettifyKey(key) {
          return String(key || "")
            .replaceAll("_", " ")
            .replaceAll("-", " ")
            .replace(/\s+/g, " ")
            .trim()
            .replace(/\b\w/g, (match) => match.toUpperCase()) || "Value";
        }

        function columnSpecFromValue(rawColumn, index) {
          if (typeof rawColumn === "string") {
            const key = rawColumn.trim() || "col_" + (index + 1);
            return { key, label: prettifyKey(rawColumn) };
          }

          if (typeof rawColumn === "number") {
            return { key: "col_" + (index + 1), label: String(rawColumn) };
          }

          if (rawColumn && typeof rawColumn === "object") {
            const key = String(
              rawColumn.key || rawColumn.name || rawColumn.id || rawColumn.field || rawColumn.label || "col_" + (index + 1),
            ).trim();
            const label = String(rawColumn.label || rawColumn.name || rawColumn.key || prettifyKey(key));
            return { key: key || "col_" + (index + 1), label };
          }

          const key = "col_" + (index + 1);
          return { key, label: key };
        }

        function extractRowsSource(value) {
          if (Array.isArray(value)) {
            return value;
          }

          if (value && typeof value === "object") {
            for (const key of ["rows", "items", "data"]) {
              if (Array.isArray(value[key])) {
                return value[key];
              }
            }
          }

          return [value];
        }

        function inferColumns(rowsSource, explicitColumns) {
          const columns = [];
          const seen = new Set();
          const pushColumn = (key, label) => {
            const normalizedKey = String(key || "").trim();
            if (!normalizedKey || seen.has(normalizedKey)) {
              return;
            }
            seen.add(normalizedKey);
            columns.push({ key: normalizedKey, label: String(label || normalizedKey) });
          };

          if (Array.isArray(explicitColumns) && explicitColumns.length) {
            explicitColumns.forEach((rawColumn, index) => {
              const column = columnSpecFromValue(rawColumn, index);
              pushColumn(column.key, column.label);
            });
            return columns;
          }

          const arrayRows = rowsSource.filter(Array.isArray);
          if (arrayRows.length) {
            const maxLength = arrayRows.reduce((max, row) => Math.max(max, row.length), 0);
            for (let index = 0; index < maxLength; index += 1) {
              pushColumn("col_" + (index + 1), "Column " + (index + 1));
            }
            return columns;
          }

          const keys = new Set();
          for (const row of rowsSource) {
            if (row && typeof row === "object" && !Array.isArray(row)) {
              Object.keys(row).forEach((key) => keys.add(key));
            }
          }

          Array.from(keys)
            .sort()
            .forEach((key) => pushColumn(key, prettifyKey(key)));

          if (!columns.length) {
            pushColumn("value", "Value");
          }

          return columns;
        }

        function pickRowKey(row, index) {
          if (row && typeof row === "object" && !Array.isArray(row)) {
            for (const key of ["id", "key", "uuid", "slug", "name"]) {
              const cell = valueToCell(row[key]);
              if (cell.text.trim()) {
                return key + ":" + cell.text;
              }
            }
          }

          return "row-" + index;
        }

        function normalizeRows(rowsSource, columns) {
          return rowsSource.map((row, rowIndex) => {
            const cells = {};
            if (Array.isArray(row)) {
              columns.forEach((column, columnIndex) => {
                cells[column.key] = valueToCell(row[columnIndex]);
              });
            } else if (row && typeof row === "object") {
              columns.forEach((column) => {
                cells[column.key] = valueToCell(row[column.key]);
              });
            } else {
              cells.value = valueToCell(row);
            }

            return { key: pickRowKey(row, rowIndex), cells };
          });
        }

        function extractPrimaryTableName(query) {
          const match = String(query || "").match(/\bfrom\s+(?:`([^`]+)`|\[([^\]]+)\]|"([^"]+)"|([a-zA-Z_][a-zA-Z0-9_]*))/i);
          return String((match && (match[1] || match[2] || match[3] || match[4])) || "").trim();
        }

        function normalizeSnapshot(payload) {
          const root = payload && typeof payload === "object" && !Array.isArray(payload) ? payload : {};
          const query = String(root.query || "");
          const rowsSource = extractRowsSource(payload);
          const columns = inferColumns(rowsSource, Array.isArray(root.columns) ? root.columns : null);
          return {
            title: String(root.name || root.title || root.view_name || root.viewName || "Generic table"),
            subtitle: query || rowsSource.length + " row" + (rowsSource.length === 1 ? "" : "s"),
            sql: query,
            kind: Array.isArray(payload) ? "array" : root.rows ? "view-snapshot" : "object",
            sourceTable: extractPrimaryTableName(query),
            columns,
            rows: normalizeRows(rowsSource, columns),
          };
        }

        function ensureTableElement(table) {
          let frameElement = tablePanel?.querySelector(".tableFrame");
          if (!(frameElement instanceof HTMLElement)) {
            frameElement = document.createElement("div");
            frameElement.className = "tableFrame";
            tablePanel?.replaceChildren(frameElement);
          }

          let tableElement = frameElement.querySelector("table.table");
          if (!(tableElement instanceof HTMLTableElement)) {
            tableElement = document.createElement("table");
            tableElement.className = "table";
            frameElement.replaceChildren(tableElement);
          }

          return tableElement;
        }

        function syncTableHeader(tableElement, columns) {
          let thead = tableElement.querySelector("thead");
          const hideHeaderRow = columns.length === 1 && columns[0]?.key === "head";
          if (hideHeaderRow) {
            thead?.remove();
            return;
          }

          if (!(thead instanceof HTMLTableSectionElement)) {
            thead = document.createElement("thead");
            tableElement.prepend(thead);
          }

          const existingKeys = Array.from(thead.querySelectorAll("th")).map((th) => th.dataset.columnKey || "");
          const nextKeys = columns.map((column) => column.key);
          if (existingKeys.join("\u0000") === nextKeys.join("\u0000")) {
            Array.from(thead.querySelectorAll("th .columnName")).forEach((name, index) => {
              name.textContent = columns[index]?.label || "";
            });
            return;
          }

          const row = document.createElement("tr");
          columns.forEach((column, columnIndex) => {
            const th = document.createElement("th");
            th.scope = "col";
            th.dataset.columnIndex = String(columnIndex);
            th.dataset.columnKey = column.key;
            const name = document.createElement("div");
            name.className = "columnName";
            name.textContent = column.label;
            th.append(name);
            row.append(th);
          });
          thead.replaceChildren(row);
        }

        function rowIdFromCells(row) {
          return String(row.cells?.id?.text || "").trim();
        }

        function updateCellValue(cellElement, valueElement, row, column, cellValue, sourceTable) {
          const editing = state.editingCell;
          const rowId = rowIdFromCells(row);
          const isEditingThisCell =
            editing &&
            String(editing.rowId || "") === rowId &&
            String(editing.columnKey || "") === column.key;

          if (isEditingThisCell) {
            editing.cell = cellElement;
            editing.valueElement = valueElement;
            if (editing.currentText === editing.lastSavedText && editing.textarea instanceof HTMLTextAreaElement) {
              const start = editing.textarea.selectionStart;
              const end = editing.textarea.selectionEnd;
              editing.currentText = cellValue.text;
              editing.lastSavedText = cellValue.text;
              editing.textarea.value = cellValue.text;
              editing.textarea.setSelectionRange(
                clamp(start, 0, editing.textarea.value.length),
                clamp(end, 0, editing.textarea.value.length),
              );
            }
            return;
          }

          valueElement.textContent = cellValue.text;
          if (column.key === "id" && rowId && sourceTable) {
            let button = cellElement.querySelector("[data-delete-row-id][data-delete-table-name]");
            if (!(button instanceof HTMLButtonElement)) {
              button = document.createElement("button");
              button.className = "button button--danger rowDeleteButton";
              button.type = "button";
              button.textContent = "Delete";
              cellElement.append(button);
            }
            button.dataset.deleteRowId = rowId;
            button.dataset.deleteTableName = sourceTable;
            button.title = "Delete row";
            button.setAttribute("aria-label", "Delete row " + rowId + " from " + sourceTable);
          }
        }

        function renderTableSnapshot(table) {
          if (!tablePanel) {
            return;
          }

          const tableElement = ensureTableElement(table);
          syncTableHeader(tableElement, table.columns);

          let tbody = tableElement.querySelector("tbody");
          if (!(tbody instanceof HTMLTableSectionElement)) {
            tbody = document.createElement("tbody");
            tableElement.append(tbody);
          }

          if (table.sourceTable) {
            tbody.dataset.sourceTable = table.sourceTable;
          } else {
            delete tbody.dataset.sourceTable;
          }

          const existingRows = new Map(
            Array.from(tbody.querySelectorAll("tr")).map((row) => [row.dataset.rowKey || "", row]),
          );
          const nextRows = [];

          table.rows.forEach((row, rowIndex) => {
            const rowId = rowIdFromCells(row);
            let rowElement = existingRows.get(row.key);
            if (!(rowElement instanceof HTMLTableRowElement)) {
              rowElement = document.createElement("tr");
            }

            rowElement.dataset.rowKey = row.key;
            rowElement.dataset.rowIndex = String(rowIndex);
            rowElement.dataset.rowId = rowId;
            rowElement.dataset.rowQuantity = String(row.cells?.quantity?.text || "");

            table.columns.forEach((column, columnIndex) => {
              const selector = `td[data-column-key="${escapeCssSelector(column.key)}"]`;
              let cellElement = rowElement.querySelector(selector);
              if (!(cellElement instanceof HTMLTableCellElement)) {
                cellElement = document.createElement("td");
                rowElement.append(cellElement);
              }

              const cellValue = row.cells[column.key] || { text: "", kind: "null" };
              cellElement.className = column.key === "id" ? "cell cell--id" : "cell";
              cellElement.dataset.columnIndex = String(columnIndex);
              cellElement.dataset.columnKey = column.key;
              cellElement.dataset.cellKind = cellValue.kind;
              if (rowId) {
                cellElement.dataset.rowId = rowId;
              } else {
                delete cellElement.dataset.rowId;
              }

              let valueElement = cellElement.querySelector(".cellValue");
              if (!(valueElement instanceof HTMLElement)) {
                valueElement = document.createElement("div");
                cellElement.prepend(valueElement);
              }
              valueElement.className = column.key === "id" ? "cellValue cellValue--id" : "cellValue";
              updateCellValue(cellElement, valueElement, row, column, cellValue, table.sourceTable);
            });

            Array.from(rowElement.querySelectorAll("td[data-column-key]")).forEach((cell) => {
              if (!table.columns.some((column) => column.key === cell.dataset.columnKey)) {
                cell.remove();
              }
            });

            nextRows.push(rowElement);
          });

          nextRows.forEach((rowElement) => {
            tbody.append(rowElement);
          });
          Array.from(tbody.querySelectorAll("tr")).forEach((rowElement) => {
            if (!nextRows.includes(rowElement)) {
              rowElement.remove();
            }
          });
          syncSelection();
        }

        function renderDetailsSnapshot(table) {
          if (!tableDetails) {
            return;
          }

          const title = tableDetails.querySelector(".detailCard:not(.detailCard--setting) .detailTitle");
          if (title) {
            title.textContent = table.title;
          }
          const copy = tableDetails.querySelector(".detailCard:not(.detailCard--setting) .detailCopy");
          if (copy) {
            copy.textContent = table.subtitle;
          }
          const code = tableDetails.querySelector("pre.codeBlock");
          if (code) {
            code.textContent = table.sql || "No SQL query was provided.";
          }
        }

        function renderSnapshotPayload(payload) {
          const table = normalizeSnapshot(payload);
          renderDetailsSnapshot(table);
          renderTableSnapshot(table);
          setStatus("Live", "live");
        }

        function buildDeleteRowUrl(tableName, rowId) {
          if (!serverId) {
            return "";
          }

          const trimmedTableName = String(tableName || "").trim();
          const numericRowId = Number(rowId);
          if (!trimmedTableName || !Number.isInteger(numericRowId) || numericRowId <= 0) {
            return "";
          }

          return (
            "/host/integrations/servers/" +
            encodeURIComponent(serverId) +
            "/table/" +
            encodeURIComponent(trimmedTableName) +
            "/" +
            encodeURIComponent(String(numericRowId))
          );
        }

        async function deleteRowFromTable(button) {
          if (!(button instanceof HTMLElement)) {
            return;
          }

          const rowId = Number(button.dataset.deleteRowId || 0);
          const tableName = String(
            button.dataset.deleteTableName ||
              button.closest("tbody")?.dataset.sourceTable ||
              button.closest("table")?.dataset.sourceTable ||
              "",
          ).trim();
          const deleteUrl = buildDeleteRowUrl(tableName, rowId);
          if (!deleteUrl) {
            return;
          }

          const previousLabel = button.textContent || "";
          button.disabled = true;
          button.dataset.deleting = "true";
          setStatus("Deleting", "loading");

          try {
            const response = await fetch(deleteUrl, {
              method: "DELETE",
              headers: {
                Accept: "application/json",
              },
              cache: "no-store",
            });

            if (response.status === 401) {
              window.LinceWidgetHost?.invalidateServerAuth?.(serverId);
              setStatus("Bloqueado", "error");
              return;
            }

            const rawBody = await response.text().catch(() => "");
            let payload = null;
            if (rawBody) {
              try {
                payload = JSON.parse(rawBody);
              } catch {
                payload = null;
              }
            }

            if (!response.ok) {
              throw new Error(
                (payload && typeof payload.message === "string" && payload.message) ||
                  rawBody ||
                  `Nao foi possivel excluir a linha (${response.status}).`,
              );
            }

            if (!payload || Number(payload.rows_affected || 0) <= 0) {
              throw new Error("A linha nao foi encontrada na tabela consultada.");
            }

            setStatus("Live", "live");
          } catch (error) {
            setStatus("Delete failed", "error");
            if (error instanceof Error) {
              console.error(error);
            } else {
              console.error(new Error("Nao foi possivel excluir a linha."));
            }
          } finally {
            button.disabled = false;
            delete button.dataset.deleting;
            button.textContent = previousLabel;
          }
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

        function stopScrollTimer() {
          if (state.scrollTimer) {
            window.clearTimeout(state.scrollTimer);
            state.scrollTimer = null;
          }
        }

        function setScrolling(active) {
          if (!tablePanel) {
            return;
          }

          if (active) {
            tablePanel.dataset.scrolling = "true";
            stopScrollTimer();
            state.scrollTimer = window.setTimeout(() => {
              delete tablePanel.dataset.scrolling;
              state.scrollTimer = null;
            }, 160);
            return;
          }

          delete tablePanel.dataset.scrolling;
          stopScrollTimer();
        }

        function clearSelectionAttributes() {
          if (!tablePanel) {
            return;
          }

          tablePanel
            .querySelectorAll("[data-row-focused], [data-focused-cell]")
            .forEach((node) => {
              delete node.dataset.rowFocused;
              delete node.dataset.focusedCell;
            });
        }

        function rows() {
          return tablePanel
            ? Array.from(tablePanel.querySelectorAll("tbody tr"))
            : [];
        }

        function columns() {
          return tablePanel
            ? Array.from(tablePanel.querySelectorAll("thead th"))
            : [];
        }

        function focusedRow() {
          return rows()[state.focusedRowIndex] || null;
        }

        function focusedCell() {
          return cellAt(state.focusedRowIndex, state.focusedColumnIndex);
        }

        function cellByIdentity(rowId, columnKey) {
          const trimmedRowId = String(rowId || "").trim();
          const trimmedColumnKey = String(columnKey || "").trim();
          if (!trimmedRowId || !trimmedColumnKey) {
            return null;
          }

          const row = rows().find(
            (candidate) => String(candidate.dataset.rowId || "").trim() === trimmedRowId,
          );
          if (!(row instanceof HTMLElement)) {
            return null;
          }

          const safeColumnKey = escapeCssSelector(trimmedColumnKey);
          return row.querySelector(`td[data-column-key="${safeColumnKey}"]`);
        }

        function focusedCellContext() {
          const cell = focusedCell();
          if (!(cell instanceof HTMLElement)) {
            return null;
          }

          const row = cell.closest("tr");
          if (!(row instanceof HTMLElement)) {
            return null;
          }

          return {
            cell,
            row,
            rowId: String(row.dataset.rowId || "").trim(),
            tableName: String(row.closest("tbody")?.dataset.sourceTable || currentSourceTableName() || "").trim(),
            columnKey: String(cell.dataset.columnKey || "").trim(),
            kind: String(cell.dataset.cellKind || "string").trim() || "string",
          };
        }

        function syncFocusedCellAttributes() {
          const cell = focusedCell();
          if (!(cell instanceof HTMLElement)) {
            return;
          }

          cell.dataset.focusedCell = "true";
        }

        function placeCellTextarea(editing, options = {}) {
          if (
            !editing ||
            !(editing.cell instanceof HTMLElement) ||
            !(editing.valueElement instanceof HTMLElement)
          ) {
            return null;
          }

          let textarea = editing.textarea;
          if (textarea instanceof HTMLTextAreaElement && editing.valueElement.contains(textarea)) {
            return textarea;
          }

          const rect = editing.valueElement.getBoundingClientRect();
          textarea = document.createElement("textarea");
          textarea.className = "cellEditor";
          textarea.value = String(editing.currentText ?? "");
          textarea.rows = 1;
          textarea.spellcheck = false;
          textarea.setAttribute("aria-label", "Cell value");
          textarea.style.width = Math.max(24, Math.ceil(rect.width)) + "px";
          textarea.style.height = Math.max(20, Math.ceil(rect.height)) + "px";
          editing.valueElement.dataset.editingCell = "true";
          editing.valueElement.replaceChildren(textarea);
          editing.cell.dataset.focusedCell = "true";
          editing.cell.dataset.editingCell = "true";
          editing.textarea = textarea;

          focusCellEditor(textarea, options);
          return textarea;
        }

        function rememberEditorSelection(editing) {
          const textarea = editing?.textarea;
          if (!(textarea instanceof HTMLTextAreaElement)) {
            return;
          }

          editing.selectionStart = textarea.selectionStart;
          editing.selectionEnd = textarea.selectionEnd;
        }

        function updateEditingTextFromTextarea(editing) {
          if (!editing) {
            return;
          }

          if (editing.textarea instanceof HTMLTextAreaElement) {
            editing.currentText = editing.textarea.value;
          }
        }

        function removeCellTextarea(editing, replacementText) {
          if (!editing) {
            return;
          }

          const editingCell = editing.cell;
          const valueElement = editing.valueElement;
          if (valueElement instanceof HTMLElement) {
            delete valueElement.dataset.editingCell;
            valueElement.textContent = String(replacementText ?? editing.currentText ?? "");
          }

          if (editingCell instanceof HTMLElement) {
            delete editingCell.dataset.editingCell;
          }
        }

        function clearEditingCellState() {
          removeCellTextarea(state.editingCell);
          state.editingCell = null;
          stopEditDebounceTimer();
        }

        function restoreEditingCellIfNeeded() {
          const editing = state.editingCell;
          if (!editing) {
            return;
          }

          const cell = cellByIdentity(editing.rowId, editing.columnKey) || cellAt(editing.rowIndex, editing.columnIndex);
          if (!(cell instanceof HTMLElement)) {
            clearEditingCellState();
            return;
          }

          const valueElement = cellValueElement(cell);
          if (!(valueElement instanceof HTMLElement)) {
            clearEditingCellState();
            return;
          }

          state.editingCell = {
            ...editing,
            cell,
            valueElement,
            textarea: null,
          };
          placeCellTextarea(state.editingCell, { preserveSelection: true });
        }

        function syncSelection() {
          if (!tablePanel) {
            return;
          }

          const currentRows = rows();
          const currentColumns = columns();
          if (!currentRows.length) {
            state.focusedRowIndex = 0;
            state.focusedColumnIndex = 0;
            state.focusedRowId = "";
            state.focusedColumnKey = "";
            delete tablePanel.dataset.mode;
            clearSelectionAttributes();
            clearEditingCellState();
            return;
          }

          if (state.focusedRowId) {
            const preferredRowIndex = currentRows.findIndex(
              (row) => String(row.dataset.rowId || "").trim() === state.focusedRowId,
            );
            if (preferredRowIndex >= 0) {
              state.focusedRowIndex = preferredRowIndex;
            }
          }

          if (state.focusedColumnKey) {
            const preferredColumnIndex = currentColumns.findIndex(
              (column) => String(column.dataset.columnKey || "").trim() === state.focusedColumnKey,
            );
            if (preferredColumnIndex >= 0) {
              state.focusedColumnIndex = preferredColumnIndex;
            }
          }

          state.focusedRowIndex = clamp(state.focusedRowIndex, 0, currentRows.length - 1);
          state.focusedColumnIndex = clamp(
            state.focusedColumnIndex,
            0,
            Math.max(0, currentColumns.length - 1),
          );

          const focused = focusedCellContext();
          state.focusedRowId = focused?.rowId || "";
          state.focusedColumnKey = focused?.columnKey || "";

          clearSelectionAttributes();
          syncModeSelect();

          currentRows.forEach((row, rowIndex) => {
            row.dataset.rowIndex = String(rowIndex);
            if (state.nerdMode && rowIndex === state.focusedRowIndex) {
              row.dataset.rowFocused = "true";
              row.scrollIntoView({ block: "nearest", inline: "nearest" });
            }
          });

          if (state.nerdMode) {
            tablePanel.dataset.mode = "helix";
          } else {
            delete tablePanel.dataset.mode;
          }

          const cell = focusedCell();
          if (cell instanceof HTMLElement) {
            cell.dataset.focusedCell = "true";
          }

          restoreEditingCellIfNeeded();
        }

        function setNerdMode(enabled) {
          state.nerdMode = enabled === true;
          writeSettings();
          syncSelection();
        }

        function moveFocus(rowDelta, columnDelta) {
          const currentRows = rows();
          const currentColumns = columns();
          if (!currentRows.length) {
            return;
          }

          if (rowDelta !== 0) {
            state.focusedRowIndex = clamp(
              state.focusedRowIndex + rowDelta,
              0,
              currentRows.length - 1,
            );
          }

          if (columnDelta !== 0 && currentColumns.length) {
            state.focusedColumnIndex = clamp(
              state.focusedColumnIndex + columnDelta,
              0,
              currentColumns.length - 1,
            );
          }

          syncSelection();
        }

        function focusCellEditor(textarea, options = {}) {
          if (!(textarea instanceof HTMLTextAreaElement)) {
            return;
          }

          textarea.focus({ preventScroll: true });
          window.requestAnimationFrame(() => {
            if (!textarea.isConnected) {
              return;
            }

            if (options.selectAll === true) {
              textarea.select();
              return;
            }

            if (Number.isInteger(options.caretOffset)) {
              const offset = clamp(options.caretOffset, 0, textarea.value.length);
              textarea.setSelectionRange(offset, offset);
              return;
            }

            if (
              options.preserveSelection === true &&
              Number.isInteger(state.editingCell?.selectionStart)
            ) {
              const start = clamp(state.editingCell.selectionStart, 0, textarea.value.length);
              const end = clamp(
                Number.isInteger(state.editingCell.selectionEnd)
                  ? state.editingCell.selectionEnd
                  : start,
                0,
                textarea.value.length,
              );
              textarea.setSelectionRange(start, end);
            }
          });
        }

        function textOffsetWithin(root, node, nodeOffset) {
          if (!(root instanceof Node) || !(node instanceof Node) || !root.contains(node)) {
            return null;
          }

          let offset = 0;
          const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
          while (walker.nextNode()) {
            const current = walker.currentNode;
            const length = current.textContent?.length || 0;
            if (current === node) {
              return offset + clamp(nodeOffset, 0, length);
            }
            offset += length;
          }

          return offset;
        }

        function caretOffsetFromPoint(valueElement, clientX, clientY) {
          if (!(valueElement instanceof HTMLElement)) {
            return null;
          }

          if (document.caretPositionFromPoint) {
            const position = document.caretPositionFromPoint(clientX, clientY);
            const node = position?.offsetNode || null;
            if (node instanceof Node && valueElement.contains(node)) {
              return textOffsetWithin(valueElement, node, position.offset);
            }
          }

          if (document.caretRangeFromPoint) {
            const range = document.caretRangeFromPoint(clientX, clientY);
            const node = range?.startContainer || null;
            if (node instanceof Node && valueElement.contains(node)) {
              return textOffsetWithin(valueElement, node, range.startOffset);
            }
          }

          return null;
        }

        function isInsideEditingValue(node) {
          const textarea = state.editingCell?.textarea;
          if (!(textarea instanceof HTMLTextAreaElement) || !(node instanceof Node)) {
            return false;
          }

          return node === textarea || textarea.contains(node);
        }

        function isEditingElementMutation(mutation) {
          const valueElement = state.editingCell?.valueElement;
          if (!(valueElement instanceof HTMLElement)) {
            return false;
          }

          return mutation.target === valueElement || valueElement.contains(mutation.target);
        }

        function beginEditOnCell(cell, options = {}) {
          if (!(cell instanceof HTMLElement)) {
            return;
          }

          if (state.editingCell?.cell === cell) {
            placeCellTextarea(state.editingCell, { preserveSelection: true });
            return;
          }

          if (state.editingCell) {
            const previous = state.editingCell;
            updateEditingTextFromTextarea(previous);
            clearEditingCellState();
            void saveEditingCell(previous, { exit: false });
          }

          const row = cell.closest("tr");
          const valueElement = cellValueElement(cell);
          if (!(row instanceof HTMLElement) || !(valueElement instanceof HTMLElement)) {
            return;
          }

          const columnKey = String(cell.dataset.columnKey || "").trim();
          if (!columnKey) {
            return;
          }

          if (columnKey === "id") {
            showErrorToast("id column is read-only");
            return;
          }

          const rowId = String(row.dataset.rowId || "").trim();
          const tableName = String(row.closest("tbody")?.dataset.sourceTable || currentSourceTableName() || "").trim();
          const rowIndex = Number.parseInt(String(row.dataset.rowIndex || state.focusedRowIndex || 0), 10);
          const columnIndex = Number.parseInt(String(cell.dataset.columnIndex || state.focusedColumnIndex || 0), 10);
          const kind = String(cell.dataset.cellKind || "string").trim() || "string";
          const text = String(valueElement.textContent || "");

          state.focusedRowIndex = Number.isInteger(rowIndex) && rowIndex >= 0 ? rowIndex : state.focusedRowIndex;
          state.focusedColumnIndex = Number.isInteger(columnIndex) && columnIndex >= 0 ? columnIndex : state.focusedColumnIndex;
          state.focusedRowId = rowId;
          state.focusedColumnKey = columnKey;

          state.editingCell = {
            cell,
            valueElement,
            rowIndex: state.focusedRowIndex,
            columnIndex: state.focusedColumnIndex,
            rowId,
            tableName,
            columnKey,
            kind,
            originalText: text,
            lastSavedText: text,
            currentText: text,
            selectionStart: 0,
            selectionEnd: 0,
            textarea: null,
          };

          clearSelectionAttributes();
          cell.dataset.focusedCell = "true";
          placeCellTextarea(state.editingCell, {
            caretOffset: options.caretOffset,
            selectAll: options.selectAll === true,
          });
        }

        function normalizeCellEditValue(editing, rawText) {
          const kind = String(editing?.kind || "string").trim() || "string";
          const text = String(rawText ?? "");
          const trimmed = text.trim();

          if (kind === "number") {
            if (!trimmed) {
              return null;
            }
            const parsed = Number(text);
            if (Number.isNaN(parsed)) {
              throw new Error("expected a number");
            }
            return parsed;
          }

          if (kind === "boolean") {
            if (!trimmed) {
              return null;
            }
            if (["true", "1", "yes", "on"].includes(trimmed.toLowerCase())) {
              return true;
            }
            if (["false", "0", "no", "off"].includes(trimmed.toLowerCase())) {
              return false;
            }
            throw new Error("expected true or false");
          }

          if (kind === "null") {
            return trimmed ? text : null;
          }

          return text;
        }

        function readEditingCellText(editing) {
          return String(editing?.currentText || "");
        }

        function readRowCellText(row, columnKey) {
          if (!(row instanceof HTMLElement)) {
            return "";
          }

          const safeColumnKey = escapeCssSelector(String(columnKey || "").trim());
          if (!safeColumnKey) {
            return "";
          }

          return String(
            row.querySelector(`td[data-column-key="${safeColumnKey}"] .cellValue`)?.textContent || "",
          );
        }

        function buildOrganSavePayload(editing, rawText) {
          const row = editing?.cell?.closest("tr");
          if (!(row instanceof HTMLElement)) {
            throw new Error("row is missing");
          }

          const payload = {
            name: readRowCellText(row, "name"),
            base_url: readRowCellText(row, "base_url"),
          };

          if (editing.columnKey === "name") {
            payload.name = rawText;
          } else if (editing.columnKey === "base_url") {
            payload.base_url = rawText;
          }

          return payload;
        }

        function buildSaveTarget(editing, rawText) {
          const currentTableName = String(editing?.tableName || "").trim();
          const currentRowId = String(editing?.rowId || "").trim();
          const currentColumnKey = String(editing?.columnKey || "").trim();

          if (!currentTableName) {
            throw new Error("table is unknown");
          }

          if (!currentRowId) {
            throw new Error("row id is missing");
          }

          if (!currentColumnKey || currentColumnKey === "id") {
            throw new Error("id column is read-only");
          }

          if (currentTableName === "organ") {
            return {
              url: "/organ/" + encodeURIComponent(currentRowId),
              payload: buildOrganSavePayload(editing, rawText),
            };
          }

          if (!serverId) {
            throw new Error("server is not configured");
          }

          if (!/^[0-9]+$/.test(currentRowId)) {
            throw new Error("row id is missing");
          }

          return {
            url:
              "/host/integrations/servers/" +
              encodeURIComponent(serverId) +
              "/table/" +
              encodeURIComponent(currentTableName) +
              "/" +
              encodeURIComponent(currentRowId),
            payload: { [currentColumnKey]: normalizeCellEditValue(editing, rawText) },
          };
        }

        async function saveEditingCell(editing, options = {}) {
          if (!editing) {
            return;
          }

          const rawText = readEditingCellText(editing);
          if (rawText === editing.lastSavedText) {
            if (options.exit === true) {
              clearEditingCellState();
            }
            return;
          }

          let target;
          try {
            target = buildSaveTarget(editing, rawText);
          } catch (error) {
            if (error instanceof Error) {
              showErrorToast(error.message);
            } else {
              showErrorToast("invalid value");
            }
            return;
          }

          const controller = new AbortController();
          state.editSaveController = controller;
          const requestBody = JSON.stringify(target.payload);

          try {
            setStatus("Saving", "loading");
            const response = await fetch(target.url, {
              method: "PATCH",
              headers: {
                "Content-Type": "application/json",
                Accept: "application/json",
              },
              body: requestBody,
              cache: "no-store",
              signal: controller.signal,
            });

            const rawBody = await response.text().catch(() => "");
            let payload = null;
            if (rawBody) {
              try {
                payload = JSON.parse(rawBody);
              } catch {
                payload = null;
              }
            }

            if (response.status === 401) {
              window.LinceWidgetHost?.invalidateServerAuth?.(serverId);
              setStatus("Bloqueado", "error");
              throw new Error("server auth expired");
            }

            if (!response.ok) {
              throw new Error(
                (payload && typeof payload.message === "string" && payload.message) ||
                  (payload && typeof payload.error === "string" && payload.error) ||
                  rawBody ||
                  `patch failed (${response.status})`,
              );
            }

            editing.lastSavedText = rawText;
            setStatus("Live", "live");
            if (options.exit === true) {
              clearEditingCellState();
            }
          } catch (error) {
            if (controller.signal.aborted) {
              return;
            }

            const reason = error instanceof Error ? error.message : "patch failed";
            setStatus("Save failed", "error");
            showErrorToast(reason);
          } finally {
            if (state.editSaveController === controller) {
              state.editSaveController = null;
            }
          }
        }

        function scheduleEditingCellSave() {
          if (!state.editingCell) {
            return;
          }

          stopEditDebounceTimer();
          stopEditSaveRequest();
          state.editDebounceTimer = window.setTimeout(() => {
            const editing = state.editingCell;
            if (!editing) {
              return;
            }

            void saveEditingCell(editing, { exit: false });
          }, 500);
        }

        function handleTableKeydown(event) {
          if (!tablePanel) {
            return;
          }

          if (state.editingCell) {
            if (event.key === "Escape") {
              event.preventDefault();
              const editing = state.editingCell;
              if (editing) {
                editing.currentText = editing.lastSavedText;
                if (editing.textarea instanceof HTMLTextAreaElement) {
                  editing.textarea.value = editing.lastSavedText;
                }
              }
              clearEditingCellState();
              syncSelection();
              return;
            }

            return;
          }

          if (!state.nerdMode) {
            if (event.key === "F2") {
              const cell = focusedCell();
              if (cell instanceof HTMLElement) {
                event.preventDefault();
                beginEditOnCell(cell, { selectAll: false });
              }
            }
            return;
          }

          if (event.metaKey || event.ctrlKey || event.altKey) {
            return;
          }

          const key = event.key;
          if (key === "j" || key === "ArrowDown") {
            event.preventDefault();
            moveFocus(1, 0);
            return;
          }

          if (key === "k" || key === "ArrowUp") {
            event.preventDefault();
            moveFocus(-1, 0);
            return;
          }

          if (key === "h" || key === "ArrowLeft") {
            event.preventDefault();
            moveFocus(0, -1);
            return;
          }

          if (key === "l" || key === "ArrowRight") {
            event.preventDefault();
            moveFocus(0, 1);
            return;
          }

          if (key === "F2") {
            event.preventDefault();
            const cell = focusedCell();
            if (cell instanceof HTMLElement) {
              beginEditOnCell(cell, { selectAll: false });
            }
          }
        }

        function clearStream() {
          stopReconnectTimer();
          setScrolling(false);

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

        async function connectStream(reset) {
          clearStream();

          if (!state.streamUrl) {
            setStatus("Configurar", "idle");
            return;
          }

          if (reset) {
            setStatus("Connecting", "loading");
          }

          const generation = ++state.streamGeneration;
          const controller = new AbortController();
          state.controller = controller;

          try {
            if (bootstrap) {
              bootstrap.dataset.streamUrl = state.streamUrl;
            }

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
              window.LinceWidgetHost?.invalidateServerAuth?.(serverId);
              setStatus("Bloqueado", "error");
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

                const payload = parseSsePayload(event.data);
                if (event.event === "snapshot") {
                  renderSnapshotPayload(payload);
                } else if (event.event === "error") {
                  setStatus("Offline", "error");
                  showErrorToast(
                    payload && typeof payload === "object"
                      ? payload.message || payload.error || "stream error"
                      : payload || "stream error",
                  );
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

        window.TableWidget = {
          reconnect() {
            state.streamUrl = buildStreamUrl();
            if (!state.streamUrl) {
              setStatus("Configurar", "idle");
              return;
            }
            state.reconnectAttempt = 0;
            connectStream(true);
          },
        };

        if (tablePanel) {
          tablePanel.tabIndex = 0;
          tablePanel.addEventListener("keydown", handleTableKeydown);
          tablePanel.addEventListener("click", (event) => {
            const target = event.target;
            if (!(target instanceof HTMLElement)) {
              return;
            }

            if (target.closest(".cellEditor")) {
              return;
            }

            const button = target.closest("[data-delete-row-id][data-delete-table-name]");
            if (button instanceof HTMLElement) {
              event.preventDefault();
              event.stopPropagation();
              deleteRowFromTable(button);
              return;
            }

            const cell = target.closest("td[data-column-key]");
            if (!(cell instanceof HTMLElement)) {
              return;
            }

            const row = cell.closest("tr");
            if (row instanceof HTMLElement) {
              state.focusedRowIndex = Number.parseInt(String(row.dataset.rowIndex || state.focusedRowIndex || 0), 10);
              state.focusedColumnIndex = Number.parseInt(String(cell.dataset.columnIndex || state.focusedColumnIndex || 0), 10);
              state.focusedRowId = String(row.dataset.rowId || "").trim();
              state.focusedColumnKey = String(cell.dataset.columnKey || "").trim();
            }

            if (state.nerdMode) {
              syncSelection();
              return;
            }

            const valueElement = cellValueElement(cell);
            const caretOffset = caretOffsetFromPoint(valueElement, event.clientX, event.clientY);
            event.preventDefault();
            beginEditOnCell(cell, { caretOffset, selectAll: false });
          });
          tablePanel.addEventListener("pointerdown", () => {
            if (!state.nerdMode) {
              return;
            }
            window.requestAnimationFrame(() => {
              tablePanel.focus({ preventScroll: true });
            });
          });
          tablePanel.addEventListener("focusin", (event) => {
            const target = event.target;
            if (!(target instanceof HTMLElement)) {
              return;
            }

            if (target.closest(".cellEditor")) {
              return;
            }

            const cell = target.closest("td[data-column-key]");
            if (!(cell instanceof HTMLElement)) {
              return;
            }

            const row = cell.closest("tr");
            if (row instanceof HTMLElement) {
              state.focusedRowIndex = Number.parseInt(String(row.dataset.rowIndex || state.focusedRowIndex || 0), 10);
              state.focusedColumnIndex = Number.parseInt(String(cell.dataset.columnIndex || state.focusedColumnIndex || 0), 10);
              state.focusedRowId = String(row.dataset.rowId || "").trim();
              state.focusedColumnKey = String(cell.dataset.columnKey || "").trim();
            }

            if (state.nerdMode || state.editingCell) {
              syncSelection();
            }
          });
          tablePanel.addEventListener("focusout", (event) => {
            const target = event.target;
            if (!(target instanceof HTMLElement)) {
              return;
            }

            const editing = state.editingCell;
            if (!editing || target !== editing.textarea) {
              return;
            }

            updateEditingTextFromTextarea(editing);
            window.requestAnimationFrame(() => {
              if (state.editingCell !== editing) {
                return;
              }

              const activeElement = document.activeElement;
              if (
                activeElement instanceof Node &&
                (isInsideEditingValue(activeElement) || !editing.textarea?.isConnected)
              ) {
                return;
              }

              void saveEditingCell(editing, { exit: false });
            });
          });
          tablePanel.addEventListener("input", (event) => {
            const target = event.target;
            if (!(target instanceof HTMLElement)) {
              return;
            }

            const editing = state.editingCell;
            if (!editing || target !== editing.textarea) {
              return;
            }

            editing.currentText = editing.textarea.value;
            rememberEditorSelection(editing);
            scheduleEditingCellSave();
          });
          tablePanel.addEventListener("keyup", () => {
            rememberEditorSelection(state.editingCell);
          });
          tablePanel.addEventListener("mouseup", () => {
            rememberEditorSelection(state.editingCell);
          });

          const observer = new MutationObserver((mutations) => {
            const onlyEditingValueChanged =
              state.editingCell &&
              mutations.length > 0 &&
              mutations.every(isEditingElementMutation);

            if (!onlyEditingValueChanged) {
              syncSelection();
              refreshCreateSchemaIfNeeded();
            }
          });
          observer.observe(tablePanel, { childList: true, characterData: true, subtree: true });
        }

        if (createOpenButton) {
          createOpenButton.addEventListener("click", () => {
            toggleCreatePanel(true);
          });
        }

        if (infoOpenButton) {
          infoOpenButton.addEventListener("click", () => {
            openInfoPanel(true);
          });
        }

        if (createCloseButton) {
          createCloseButton.addEventListener("click", () => {
            closeCreatePanel();
          });
        }

        if (infoCloseButton) {
          infoCloseButton.addEventListener("click", () => {
            closeInfoPanel();
          });
        }

        if (createTableSelect) {
          createTableSelect.addEventListener("change", () => {
            const nextTable = String(createTableSelect.value || "record").trim() || "record";
            snapshotCreateDraft(state.createSelectedTable);
            state.createSelectedTable = nextTable;
            state.createPreferredTable = nextTable;
            renderCreateTableSelect();
            renderCreateFields();
          });
        }

        if (createSubmitButton) {
          createSubmitButton.addEventListener("click", () => {
            void submitCreate();
          });
        }

        if (createFields) {
          createFields.addEventListener("input", () => {
            snapshotCreateDraft(state.createSelectedTable);
          });
          createFields.addEventListener("change", () => {
            snapshotCreateDraft(state.createSelectedTable);
          });
        }

        if (tableDetails || tablePanel) {
          const observer = new MutationObserver(() => {
            refreshCreateSchemaIfNeeded();
          });
          if (tableDetails) {
            observer.observe(tableDetails, { childList: true, subtree: true });
          }
          if (tablePanel) {
            observer.observe(tablePanel, { childList: true, subtree: true });
          }
        }

        const storedSettings = readSettings();
        if (storedSettings) {
          state.nerdMode = storedSettings.nerdMode;
        }
        syncModeSelect();
        syncPanelVisibility();
        syncCreatePanelVisibility();
        renderCreateTableSelect();
        renderCreateFields();

        document.addEventListener("change", (event) => {
          const target = event.target;
          if (!(target instanceof HTMLSelectElement) || target.id !== "table-mode") {
            return;
          }

          setNerdMode(target.value === "helix");
          if (state.nerdMode && tablePanel) {
            tablePanel.focus({ preventScroll: true });
          }
        });

        state.streamUrl = buildStreamUrl();
        if (!state.streamUrl) {
          setStatus("Configurar", "idle");
        } else {
          syncSelection();
          connectStream(false);
        }

        if (tableDetails) {
          const observer = new MutationObserver(() => {
            syncModeSelect();
          });
          observer.observe(tableDetails, { childList: true, subtree: true });
        }
      })();
    "#.to_string()
}
