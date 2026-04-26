pub(super) fn script() -> String {
    r#"
      (() => {
        const frame = window.frameElement;
        const statusPill = document.getElementById("table-status");
        const tableDetails = document.getElementById("table-details");
        const contentShell = document.getElementById("content-shell");
        const bootstrap = document.getElementById("table-stream-bootstrap");
        const createOpenButton = document.getElementById("create-open");
        const createPanel = document.getElementById("create-panel");
        const createTableSelect = document.getElementById("create-table-select");
        const createFields = document.getElementById("create-fields");
        const createSubmitButton = document.getElementById("create-submit");
        const tablePanel = document.getElementById("table-body");
        const datastarReady = import("/static/vendored/datastar.js").catch(() => null);
        const serverId = String(frame?.dataset?.linceServerId || "").trim();
        const viewId = Number(String(frame?.dataset?.linceViewId || "").trim());
        const instanceId = String(frame?.dataset?.packageInstanceId || "preview").trim() || "preview";
        const settingsKey = "table-nerd/" + instanceId;

        const state = {
          controller: null,
          reconnectTimer: null,
          reconnectAttempt: 0,
          scrollTimer: null,
          streamGeneration: 0,
          streamUrl: "",
          nerdMode: false,
          focusedRowIndex: 0,
          focusedColumnIndex: 0,
          createOpen: false,
          createLoading: false,
          createSchemas: [],
          createSelectedTable: "record",
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
          }

          if (contentShell) {
            contentShell.dataset.createOpen = state.createOpen ? "true" : "false";
          }

          if (createOpenButton) {
            createOpenButton.textContent = state.createOpen ? "Close" : "Create";
            createOpenButton.disabled = !serverId;
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

        function loadCreateSelection(tableName) {
          const nextTable = String(tableName || "record").trim() || "record";
          snapshotCreateDraft(state.createSelectedTable);
          state.createSelectedTable = nextTable;
          renderCreateTableSelect();
          renderCreateFields();
        }

        async function loadCreateSchemas(preferredTable) {
          if (!serverId) {
            setStatus("Configure server", "error");
            return;
          }

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
          syncCreatePanelVisibility();

          if (!nextOpen) {
            return;
          }

          renderCreateTableSelect();
          renderCreateFields();
          void loadCreateSchemas(detectPreferredTableName());
        }

        function buildCreateEndpoint(tableName) {
          if (!serverId) {
            return "";
          }

          const trimmed = String(tableName || "record").trim() || "record";
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
            "/table/stream"
          );
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

        function dispatchDatastarEvent(eventName, detail) {
          document.dispatchEvent(
            new CustomEvent("datastar-fetch", {
              detail: {
                type: eventName,
                argsRaw: detail,
              },
            }),
          );
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
          const row = focusedRow();
          if (!row) {
            return null;
          }

          return row.querySelectorAll("td")[state.focusedColumnIndex] || null;
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
            delete tablePanel.dataset.mode;
            clearSelectionAttributes();
            return;
          }

          state.focusedRowIndex = clamp(state.focusedRowIndex, 0, currentRows.length - 1);
          state.focusedColumnIndex = clamp(
            state.focusedColumnIndex,
            0,
            Math.max(0, currentColumns.length - 1),
          );

          clearSelectionAttributes();
          syncModeSelect();

          if (state.nerdMode) {
            tablePanel.dataset.mode = "helix";
            currentRows.forEach((row, rowIndex) => {
              row.dataset.rowIndex = String(rowIndex);
              if (rowIndex === state.focusedRowIndex) {
                row.dataset.rowFocused = "true";
                row.scrollIntoView({ block: "nearest", inline: "nearest" });
              }
            });

          } else {
            delete tablePanel.dataset.mode;
            currentRows.forEach((row, rowIndex) => {
              row.dataset.rowIndex = String(rowIndex);
            });
          }
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

        function handleTableKeydown(event) {
          if (!state.nerdMode || !tablePanel) {
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
                if (!event.data || !event.event.startsWith("datastar-")) {
                  continue;
                }

                const payload = parseSsePayload(event.data);
                if (payload && typeof payload === "object") {
                  dispatchDatastarEvent(event.event, payload);
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

            const button = target.closest("[data-delete-row-id][data-delete-table-name]");
            if (!button || !(button instanceof HTMLElement)) {
              return;
            }

            event.preventDefault();
            event.stopPropagation();
            deleteRowFromTable(button);
          });
          tablePanel.addEventListener("pointerdown", () => {
            if (!state.nerdMode) {
              return;
            }
            window.requestAnimationFrame(() => {
              tablePanel.focus({ preventScroll: true });
            });
          });
          tablePanel.addEventListener("focus", () => {
            if (state.nerdMode) {
              syncSelection();
            }
          });

          const observer = new MutationObserver(() => {
            syncSelection();
          });
          observer.observe(tablePanel, { childList: true, subtree: true });
        }

        if (createOpenButton) {
          createOpenButton.addEventListener("click", () => {
            toggleCreatePanel();
          });
        }

        if (createTableSelect) {
          createTableSelect.addEventListener("change", () => {
            const nextTable = String(createTableSelect.value || "record").trim() || "record";
            snapshotCreateDraft(state.createSelectedTable);
            state.createSelectedTable = nextTable;
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

        const storedSettings = readSettings();
        if (storedSettings) {
          state.nerdMode = storedSettings.nerdMode;
        }
        syncModeSelect();
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

        datastarReady.then(() => {
          state.streamUrl = buildStreamUrl();
          if (!state.streamUrl) {
            setStatus("Configurar", "idle");
            return;
          }

          syncSelection();
          connectStream(false);
        });

        if (tableDetails) {
          const observer = new MutationObserver(() => {
            syncModeSelect();
          });
          observer.observe(tableDetails, { childList: true, subtree: true });
        }
      })();
    "#.to_string()
}
