pub(super) fn script() -> String {
    // Stage 8b rebuild (Track A): the kanban sand now speaks ONLY Protein (reads)
    // + Actions (writes) over the widget bridge — no more server view stream, no
    // `read_view_stream`. It subscribes to a records Protein, buckets records into
    // columns by a configurable field (default: `concept`), renders the board
    // client-side, and moves/creates/edits/deletes cards with typed Actions.
    //
    // It is driven by a GENERAL, reusable Protein (the card's saved/inline Protein
    // from the Data panel), and is tolerant-ignore of extra included data: it
    // renders the fields it understands (uid/head/slug/the column field) and
    // silently drops the rest, so the same Protein can also feed other sands.
    //
    // There is no built-in record sidepanel: clicking a card emits the
    // `recordClicked` ABI event carrying `record.uid`, so a Record Info sand (in
    // the same group, Track B) can show that record's details over Protein.
    r#"
      (() => {
        const host = window.LinceWidgetHost || null;
        const frame = window.frameElement;
        const instanceId = String(frame?.dataset?.packageInstanceId || "preview").trim() || "preview";

        const boardPanel = document.getElementById("kanban-board");
        const statusPill = document.getElementById("kanban-status");
        const detailsPanel = document.getElementById("kanban-details");
        const infoOpenButton = document.getElementById("kanban-info-open");
        const infoCloseButton = document.getElementById("kanban-info-close");
        const toastLayer = document.getElementById("kanban-toasts");

        // The column field maps a record value to a column, and a card move to the
        // Action that persists it. Only fields with an Action are movable; others
        // group read-only. `concept` (set-concept) is the default and the one the
        // migration wires; more can be added here without touching render logic.
        const COLUMN_ACTIONS = {
          concept: (uid, columnId) => ({ action: "set-concept", target: uid, concept: columnId || null }),
          quantity: (uid, columnId) => ({ action: "set-quantity", target: uid, value: Number(columnId) || 0 }),
        };

        const state = {
          rows: [],
          live: false,
          unsubscribe: null,
          subKey: "",
          drivingLabel: "all records",
          columnField: "concept",
          config: { columns: [], hiddenColumns: [] },
          selectedUid: null,
          editing: null, // uid being title-edited
          infoOpen: false,
        };

        function setStatus(text, tone) {
          if (!statusPill) return;
          statusPill.dataset.tone = tone || "idle";
          statusPill.setAttribute("aria-label", text);
          statusPill.title = text;
        }

        function toast(message) {
          if (!toastLayer) return;
          const el = document.createElement("div");
          el.className = "kanbanToast";
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

        // ---- config from card state ------------------------------------------

        function readConfig() {
          const cardState = (typeof host?.getCardState === "function" ? host.getCardState() : null) || {};
          const cfg = (cardState.kanban && typeof cardState.kanban === "object") ? cardState.kanban : {};
          state.columnField = String(cfg.columnField || "concept");
          state.config = {
            columns: Array.isArray(cfg.columns) ? cfg.columns : [],
            // "hiding part of the kanban": listed column ids are not rendered.
            hiddenColumns: Array.isArray(cfg.hiddenColumns) ? cfg.hiddenColumns.map(String) : [],
          };
        }

        // ---- column model ----------------------------------------------------

        function columnValueOf(row) {
          const raw = row ? row[state.columnField] : null;
          return raw === null || raw === undefined || raw === "" ? "" : String(raw);
        }

        // Columns = configured columns (order + labels preserved), unioned with the
        // distinct values found in the data, minus hidden ones. An empty-value
        // "Unassigned" column always exists so field-less cards have a home.
        function deriveColumns() {
          const map = new Map(); // id -> label
          for (const column of state.config.columns) {
            const id = column && column.id != null ? String(column.id) : "";
            map.set(id, (column && column.label) || id || "Unassigned");
          }
          if (!map.has("")) map.set("", "Unassigned");
          for (const row of state.rows) {
            const id = columnValueOf(row);
            if (!map.has(id)) map.set(id, id || "Unassigned");
          }
          const hidden = new Set(state.config.hiddenColumns);
          return [...map.entries()]
            .filter(([id]) => !hidden.has(id))
            .map(([id, label]) => ({ id, label }));
        }

        function moveActionFor(uid, columnId) {
          const builder = COLUMN_ACTIONS[state.columnField];
          return builder ? builder(uid, columnId) : null;
        }

        // ---- rendering -------------------------------------------------------

        function cardTitle(row) {
          return String(row.head || row.slug || row.uid || "untitled");
        }

        function renderBoard() {
          if (!boardPanel) return;
          const columns = deriveColumns();
          if (!state.rows.length && !state.config.columns.length) {
            boardPanel.replaceChildren(
              Object.assign(document.createElement("div"), {
                className: "kanbanEmpty",
                textContent: state.live ? "No cards yet" : "Opening…",
              }),
            );
            return;
          }

          const frag = document.createDocumentFragment();
          for (const column of columns) {
            frag.appendChild(renderColumn(column));
          }
          boardPanel.replaceChildren(frag);

          if (state.editing) {
            const editor = boardPanel.querySelector(".cardTitleEditor");
            if (editor) { editor.focus(); editor.select(); }
          }
        }

        function renderColumn(column) {
          const el = document.createElement("section");
          el.className = "kanbanColumn";
          el.dataset.columnId = column.id;

          const rows = state.rows.filter((row) => columnValueOf(row) === column.id);

          const head = document.createElement("div");
          head.className = "kanbanColumnHead";
          const name = document.createElement("span");
          name.className = "kanbanColumnName";
          name.textContent = column.label;
          const count = document.createElement("span");
          count.className = "kanbanColumnCount";
          count.textContent = String(rows.length);
          head.append(name, count);
          el.appendChild(head);

          const cards = document.createElement("div");
          cards.className = "kanbanColumnCards";
          for (const row of rows) cards.appendChild(renderCard(row));
          el.appendChild(cards);

          // Drop target: moving a card here rewrites the column field.
          const movable = Boolean(moveActionFor("x", column.id));
          if (movable) {
            el.addEventListener("dragover", (event) => {
              event.preventDefault();
              el.classList.add("isDropTarget");
            });
            el.addEventListener("dragleave", () => el.classList.remove("isDropTarget"));
            el.addEventListener("drop", (event) => {
              event.preventDefault();
              el.classList.remove("isDropTarget");
              const uid = event.dataTransfer?.getData("text/x-uid") || "";
              if (uid) moveCard(uid, column.id);
            });

            const add = document.createElement("button");
            add.type = "button";
            add.className = "kanbanColumnAdd";
            add.textContent = "+ Add card";
            add.addEventListener("click", () => createCard(column.id));
            el.appendChild(add);
          }

          return el;
        }

        function renderCard(row) {
          const card = document.createElement("article");
          card.className = "kanbanCard";
          card.dataset.uid = row.uid;
          card.draggable = true;
          if (row.uid === state.selectedUid) card.classList.add("isSelected");

          const rowWrap = document.createElement("div");
          rowWrap.className = "kanbanCardRow";

          const title = document.createElement("div");
          title.className = "kanbanCardTitle";
          if (state.editing === row.uid) {
            const input = document.createElement("input");
            input.className = "cardTitleEditor";
            input.value = cardTitle(row);
            let done = false;
            const commit = (save) => {
              if (done) return;
              done = true;
              state.editing = null;
              if (save && input.value !== cardTitle(row)) {
                void act({ action: "edit-record-text", target: row.uid, head: input.value });
              }
              renderBoard();
            };
            input.addEventListener("blur", () => commit(true));
            input.addEventListener("keydown", (event) => {
              if (event.key === "Enter") { event.preventDefault(); commit(true); }
              else if (event.key === "Escape") { event.preventDefault(); commit(false); }
            });
            title.appendChild(input);
          } else {
            title.textContent = cardTitle(row);
            title.title = "Click to open · double-click to rename";
          }
          rowWrap.appendChild(title);

          const del = document.createElement("button");
          del.type = "button";
          del.className = "kanbanCardDelete";
          del.textContent = "×";
          del.title = "Deactivate (delete)";
          del.addEventListener("click", (event) => {
            event.stopPropagation();
            void deleteCard(row.uid);
          });
          rowWrap.appendChild(del);
          card.appendChild(rowWrap);

          if (row.body) {
            const meta = document.createElement("div");
            meta.className = "kanbanCardMeta";
            meta.textContent = String(row.body).slice(0, 80);
            card.appendChild(meta);
          }

          card.addEventListener("dragstart", (event) => {
            event.dataTransfer?.setData("text/x-uid", row.uid);
            if (event.dataTransfer) event.dataTransfer.effectAllowed = "move";
            card.classList.add("isDragging");
          });
          card.addEventListener("dragend", () => card.classList.remove("isDragging"));

          // Single click = select + emit recordClicked (record detail is the Record
          // Info sand's job). Double-click a card = inline-rename its title.
          card.addEventListener("click", () => selectCard(row));
          card.addEventListener("dblclick", (event) => {
            event.preventDefault();
            state.editing = row.uid;
            renderBoard();
          });

          return card;
        }

        function renderDetails() {
          if (!detailsPanel) return;
          const grid = detailsPanel.querySelector(".kanbanDetailGrid");
          if (!grid) return;
          grid.replaceChildren();
          const pill = (label) => {
            const span = document.createElement("span");
            span.className = "kanbanPill";
            span.textContent = label;
            grid.appendChild(span);
          };
          pill("protein: " + state.drivingLabel);
          pill("column field: " + state.columnField);
          pill("cards: " + state.rows.length);
          pill("columns: " + deriveColumns().length);
          pill("live: " + (state.live ? "yes" : "connecting"));
        }

        // ---- writes ----------------------------------------------------------

        function moveCard(uid, columnId) {
          const row = state.rows.find((entry) => entry.uid === uid);
          if (!row) return;
          if (columnValueOf(row) === columnId) return; // already here
          const action = moveActionFor(uid, columnId);
          if (!action) return;
          setStatus("moving…", "busy");
          void act(action);
        }

        async function createCard(columnId) {
          setStatus("creating…", "busy");
          const result = await act({
            action: "create-record",
            kind: "plain",
            head: "New card",
            body: "",
            quantity: 0,
          });
          if (!result || result.ok === false) return;
          // create-record cannot set the column field; classify the fresh record
          // into the column it was dropped into (when the field is movable).
          const uid = String(result.created || result.uid || "").trim();
          if (uid && columnId) {
            const action = moveActionFor(uid, columnId);
            if (action) await act(action);
          }
          setStatus("card created", "ok");
        }

        async function deleteCard(uid) {
          setStatus("deactivating…", "busy");
          if (state.selectedUid === uid) state.selectedUid = null;
          await act({ action: "deactivate", target: uid });
        }

        function selectCard(row) {
          state.selectedUid = row.uid;
          renderBoard();
          if (typeof host?.emit === "function") {
            host.emit("recordClicked", {
              table: "record",
              recordId: row.uid,
              record: {
                uid: row.uid,
                slug: row.slug ?? null,
                head: row.head ?? "",
                body: row.body ?? "",
                kind: row.kind ?? null,
                quantity: row.quantity ?? null,
                concept: row.concept ?? null,
              },
            });
          }
        }

        // ---- subscription ----------------------------------------------------

        function drivingSpec() {
          const cardState = (typeof host?.getCardState === "function" ? host.getCardState() : null) || {};
          const saved = String(cardState.savedProtein || "").trim();
          if (saved) return { kind: "saved", key: "saved:" + saved, name: saved, label: saved };
          if (cardState.protein && typeof cardState.protein === "object") {
            return {
              kind: "ast",
              key: "ast:" + JSON.stringify(cardState.protein),
              protein: cardState.protein,
              label: cardState.protein.source || "record",
            };
          }
          return { kind: "all", key: "all", protein: { source: "record", limit: 500 }, label: "all records" };
        }

        function onRows(payload) {
          // Tolerant-ignore: keep the record fields we render; drop everything else.
          const rows = Array.isArray(payload?.rows) ? payload.rows : [];
          state.rows = rows;
          state.live = payload?.live !== false;
          setStatus(state.live ? (rows.length + " cards") : "waiting", state.live ? "ok" : "idle");
          renderBoard();
          renderDetails();
        }

        function subscribe() {
          readConfig();
          if (typeof host?.subscribeProtein !== "function") {
            setStatus("bridge unavailable", "error");
            return;
          }
          const spec = drivingSpec();
          // Re-derive columns/labels even when the driving Protein is unchanged,
          // since the kanban config can change independently.
          if (spec.key === state.subKey) {
            renderBoard();
            renderDetails();
            return;
          }
          if (typeof state.unsubscribe === "function") { state.unsubscribe(); state.unsubscribe = null; }
          state.subKey = spec.key;
          state.drivingLabel = spec.label;
          state.rows = [];
          state.live = false;
          renderBoard();
          setStatus("connecting…", "busy");
          if (spec.kind === "saved" && typeof host.subscribeSaved === "function") {
            state.unsubscribe = host.subscribeSaved("kanban", spec.name, onRows);
          } else {
            state.unsubscribe = host.subscribeProtein("kanban", spec.protein, onRows);
          }
        }

        // ---- info panel ------------------------------------------------------

        function toggleInfo(open) {
          state.infoOpen = open === undefined ? !state.infoOpen : Boolean(open);
          if (detailsPanel) {
            detailsPanel.hidden = !state.infoOpen;
            detailsPanel.setAttribute("aria-hidden", String(!state.infoOpen));
          }
          renderDetails();
        }

        // ---- wire up ---------------------------------------------------------

        infoOpenButton?.addEventListener("click", () => toggleInfo(true));
        infoCloseButton?.addEventListener("click", () => toggleInfo(false));

        renderBoard();
        subscribe();

        // The host pushes card state after the ready handshake and whenever the
        // sand-settings modal changes the driving Protein or kanban config.
        document.addEventListener("lince-bridge-state", () => subscribe());

        window.addEventListener("beforeunload", () => {
          if (typeof state.unsubscribe === "function") state.unsubscribe();
        });
      })();
    "#
    .to_string()
}
