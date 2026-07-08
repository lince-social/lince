pub(super) fn script() -> String {
    // Stage 8b rebuild: the table sand now speaks ONLY Protein (reads) + Actions
    // (writes) over the widget bridge — no more server-rendered datastar HTML on
    // an SSE stream, no `/host/integrations/.../table` REST. It subscribes to a
    // records Protein, renders the table client-side, and edits/creates/deletes
    // via typed Actions. Feature parity (live table, inline edit, create drafts,
    // delete, toasts, info panel) — not pixel parity with the old server table.
    r#"
      (() => {
        const host = window.LinceWidgetHost || null;
        const frame = window.frameElement;
        const instanceId = String(frame?.dataset?.packageInstanceId || "preview").trim() || "preview";

        const statusPill = document.getElementById("table-status");
        const infoOpenButton = document.getElementById("info-open");
        const infoCloseButton = document.getElementById("info-close");
        const createOpenButton = document.getElementById("create-open");
        const createCloseButton = document.getElementById("create-close");
        const createPanel = document.getElementById("create-panel");
        const detailsPanel = document.getElementById("table-details");
        const kindSelect = document.getElementById("create-table-select");
        const createFields = document.getElementById("create-fields");
        const createSubmitButton = document.getElementById("create-submit");
        const tablePanel = document.getElementById("table-body");
        const toastLayer = document.getElementById("table-toasts");

        // Columns the table shows. `read` pulls the value from a Protein row;
        // `action` builds the Action that persists an edit (null = read-only).
        const COLUMNS = [
          { key: "slug", label: "slug", read: (r) => r.slug ?? "",
            action: (uid, v) => ({ action: "set-slug", target: uid, slug: v || null }) },
          { key: "head", label: "head", read: (r) => r.head ?? "",
            action: (uid, v) => ({ action: "edit-record-text", target: uid, head: v }) },
          { key: "body", label: "body", read: (r) => r.body ?? "",
            action: (uid, v) => ({ action: "edit-record-text", target: uid, body: v }) },
          { key: "quantity", label: "qty", read: (r) => String(r.quantity ?? 0),
            action: (uid, v) => ({ action: "set-quantity", target: uid, value: Number(v) || 0 }) },
          { key: "kind", label: "kind", read: (r) => r.kind ?? "plain", action: null },
        ];
        const KINDS = ["plain", "rule", "signal", "transfer", "decision", "device", "organ", "person", "protein", "sand"];

        const state = {
          rows: [],
          live: false,
          unsubscribe: null,
          subKey: "",
          drivingLabel: "all records",
          editing: null, // { uid, key }
          createOpen: false,
          infoOpen: false,
          draft: { kind: "plain", slug: "", head: "", body: "", quantity: "0" },
        };

        function setStatus(text, tone = "idle") {
          if (!statusPill) return;
          statusPill.dataset.tone = tone;
          statusPill.setAttribute("aria-label", text);
          statusPill.title = text;
        }

        function toast(message) {
          if (!toastLayer) return;
          const el = document.createElement("div");
          el.className = "toast";
          el.textContent = String(message || "unknown error");
          toastLayer.appendChild(el);
          window.setTimeout(() => el.remove(), 5000);
        }

        async function act(action) {
          if (typeof host?.act !== "function") {
            toast("bridge unavailable");
            return { ok: false };
          }
          try {
            const result = await host.act(action);
            if (!result || result.ok === false) {
              toast("couldn't save (" + String(result?.message || "rejected") + ")");
            }
            return result || { ok: false };
          } catch (error) {
            toast("couldn't save (" + String(error?.message || error) + ")");
            return { ok: false };
          }
        }

        // ---- table rendering -------------------------------------------------

        function renderTable() {
          if (!tablePanel) return;
          if (!state.rows.length) {
            tablePanel.innerHTML =
              '<div class="tableFrame"><div class="emptyState">' +
              '<div class="stateTitle">' + (state.live ? "No rows yet" : "Opening…") + '</div>' +
              '<div class="stateCopy">Records you create appear here live. Use Create to add one.</div>' +
              '</div></div>';
            return;
          }

          const table = document.createElement("table");
          table.className = "dataTable";

          const thead = document.createElement("thead");
          const headRow = document.createElement("tr");
          for (const column of COLUMNS) {
            const th = document.createElement("th");
            th.textContent = column.label;
            headRow.appendChild(th);
          }
          const actionsHead = document.createElement("th");
          actionsHead.textContent = "";
          headRow.appendChild(actionsHead);
          thead.appendChild(headRow);
          table.appendChild(thead);

          const tbody = document.createElement("tbody");
          for (const row of state.rows) {
            const tr = document.createElement("tr");
            tr.dataset.uid = row.uid;
            for (const column of COLUMNS) {
              tr.appendChild(renderCell(row, column));
            }
            const actionsCell = document.createElement("td");
            actionsCell.className = "rowActions";
            const del = document.createElement("button");
            del.type = "button";
            del.className = "button button--ghost";
            del.textContent = "×";
            del.title = "Deactivate (delete)";
            del.addEventListener("click", () => deleteRow(row.uid));
            actionsCell.appendChild(del);
            tr.appendChild(actionsCell);
            tbody.appendChild(tr);
          }
          table.appendChild(tbody);

          tablePanel.replaceChildren(table);

          // Focus the active editor now that it is attached to the document.
          if (state.editing) {
            const editor = tablePanel.querySelector(".cellEditor");
            if (editor) { editor.focus(); editor.select(); }
          }
        }

        function renderCell(row, column) {
          const td = document.createElement("td");
          td.dataset.column = column.key;
          const value = column.read(row);
          const editing = state.editing && state.editing.uid === row.uid && state.editing.key === column.key;

          if (editing && column.action) {
            const input = document.createElement("input");
            input.className = "cellEditor";
            input.value = value;
            td.appendChild(input);
            // focus happens after the table is attached (see renderTable)
            let done = false;
            const commit = (save) => {
              if (done) return; // Enter then blur must not double-fire
              done = true;
              state.editing = null;
              if (save && input.value !== value) {
                void act(column.action(row.uid, input.value));
              }
              renderTable(); // restore the cell; a live update refreshes the value
            };
            input.addEventListener("blur", () => commit(true));
            input.addEventListener("keydown", (event) => {
              if (event.key === "Enter") { event.preventDefault(); commit(true); }
              else if (event.key === "Escape") { event.preventDefault(); commit(false); }
            });
          } else {
            td.textContent = value;
            if (column.action) {
              td.classList.add("editable");
              td.title = "Click to edit";
              td.addEventListener("click", () => {
                state.editing = { uid: row.uid, key: column.key };
                renderTable();
              });
            }
          }
          return td;
        }

        function renderDetails() {
          if (!detailsPanel) return;
          const metrics = detailsPanel.querySelector(".detailGrid");
          if (metrics) {
            metrics.replaceChildren();
            const pill = (label) => {
              const span = document.createElement("span");
              span.className = "pill";
              span.textContent = label;
              metrics.appendChild(span);
            };
            pill("protein: " + state.drivingLabel);
            pill("rows: " + state.rows.length);
            pill("columns: " + COLUMNS.length);
            pill("live: " + (state.live ? "yes" : "connecting"));
          }
        }

        // ---- create panel ----------------------------------------------------

        function renderCreateFields() {
          if (!createFields) return;
          createFields.replaceChildren();
          const textField = (name, label, placeholder) => {
            const wrap = document.createElement("label");
            wrap.className = "field";
            const span = document.createElement("span");
            span.className = "fieldLabel";
            span.textContent = label;
            const input = document.createElement("input");
            input.className = "field field--input";
            input.value = state.draft[name] ?? "";
            input.placeholder = placeholder || "";
            input.addEventListener("input", () => { state.draft[name] = input.value; });
            wrap.append(span, input);
            createFields.appendChild(wrap);
          };
          textField("head", "head", "Title");
          textField("body", "body", "Body");
          textField("slug", "slug", "optional.slug");
          textField("quantity", "quantity", "0");
        }

        function renderKindSelect() {
          if (!kindSelect) return;
          kindSelect.replaceChildren();
          for (const kind of KINDS) {
            const option = document.createElement("option");
            option.value = kind;
            option.textContent = kind;
            if (kind === state.draft.kind) option.selected = true;
            kindSelect.appendChild(option);
          }
        }

        async function submitCreate() {
          const draft = state.draft;
          if (createSubmitButton) createSubmitButton.disabled = true;
          const action = {
            action: "create-record",
            kind: draft.kind || "plain",
            head: draft.head || "",
            body: draft.body || "",
            quantity: Number(draft.quantity) || 0,
          };
          const slug = String(draft.slug || "").trim();
          if (slug) action.slug = slug;
          const result = await act(action);
          if (createSubmitButton) createSubmitButton.disabled = false;
          if (result && result.ok !== false) {
            state.draft = { kind: draft.kind, slug: "", head: "", body: "", quantity: "0" };
            renderCreateFields();
            toggleCreate(false);
            setStatus("row created", "ok");
          }
        }

        async function deleteRow(uid) {
          setStatus("deactivating…", "busy");
          await act({ action: "deactivate", target: uid });
        }

        // ---- panels ----------------------------------------------------------

        function toggleCreate(open) {
          state.createOpen = open === undefined ? !state.createOpen : Boolean(open);
          if (!createPanel) return;
          createPanel.hidden = !state.createOpen;
          createPanel.setAttribute("aria-hidden", String(!state.createOpen));
          if (state.createOpen) { state.infoOpen = false; syncInfo(); }
        }

        function syncInfo() {
          if (!detailsPanel) return;
          detailsPanel.hidden = !state.infoOpen;
          detailsPanel.setAttribute("aria-hidden", String(!state.infoOpen));
        }

        function toggleInfo(open) {
          state.infoOpen = open === undefined ? !state.infoOpen : Boolean(open);
          if (state.infoOpen) { state.createOpen = false; toggleCreate(false); state.infoOpen = true; }
          syncInfo();
          renderDetails();
        }

        // ---- subscription ----------------------------------------------------

        // The card's chosen Protein drives the data (the new "view selection"):
        // cardState.savedProtein = a saved-Protein slug, or cardState.protein =
        // an inline AST. Absent either, fall back to a broad records window.
        function drivingSpec() {
          const cardState = (typeof host?.getCardState === "function" ? host.getCardState() : null) || {};
          const saved = String(cardState.savedProtein || "").trim();
          if (saved) return { kind: "saved", key: "saved:" + saved, name: saved, label: saved };
          if (cardState.protein && typeof cardState.protein === "object") {
            return { kind: "ast", key: "ast:" + JSON.stringify(cardState.protein), protein: cardState.protein, label: cardState.protein.source || "record" };
          }
          return { kind: "all", key: "all", protein: { source: "record", limit: 500 }, label: "all records" };
        }

        function onRows(payload) {
          // Tolerant-ignore: render the record fields we understand; any extra
          // included data (facts/promises/links/aggregates) is simply dropped.
          const rows = Array.isArray(payload?.rows) ? payload.rows : [];
          state.rows = rows;
          state.live = payload?.live !== false;
          setStatus(state.live ? (rows.length + " rows") : "waiting", state.live ? "ok" : "idle");
          renderTable();
          renderDetails();
        }

        function subscribe() {
          if (typeof host?.subscribeProtein !== "function") {
            setStatus("bridge unavailable", "error");
            return;
          }
          const spec = drivingSpec();
          if (spec.key === state.subKey) return; // driving Protein unchanged
          if (typeof state.unsubscribe === "function") { state.unsubscribe(); state.unsubscribe = null; }
          state.subKey = spec.key;
          state.drivingLabel = spec.label;
          state.rows = [];
          state.live = false;
          renderTable();
          setStatus("connecting…", "busy");
          if (spec.kind === "saved" && typeof host.subscribeSaved === "function") {
            state.unsubscribe = host.subscribeSaved("table", spec.name, onRows);
          } else {
            state.unsubscribe = host.subscribeProtein("table", spec.protein, onRows);
          }
        }

        // ---- wire up ---------------------------------------------------------

        infoOpenButton?.addEventListener("click", () => toggleInfo(true));
        infoCloseButton?.addEventListener("click", () => toggleInfo(false));
        createOpenButton?.addEventListener("click", () => toggleCreate(true));
        createCloseButton?.addEventListener("click", () => toggleCreate(false));
        createSubmitButton?.addEventListener("click", () => { void submitCreate(); });
        kindSelect?.addEventListener("change", () => { state.draft.kind = kindSelect.value; });

        renderKindSelect();
        renderCreateFields();
        renderTable();
        subscribe();

        // The host pushes card state after the ready handshake and whenever the
        // sand-settings modal changes the driving Protein — re-subscribe on any
        // change (subscribe() no-ops when the driving Protein is unchanged).
        document.addEventListener("lince-bridge-state", () => subscribe());

        window.addEventListener("beforeunload", () => {
          if (typeof state.unsubscribe === "function") state.unsubscribe();
        });
      })();
    "#
    .to_string()
}
