pub(crate) const EDITOR_RUNTIME: &str = r##"
      (() => {
        if (window.LinceRecordEditor) {
          return;
        }

        const DEFAULT_CONTEXT = {
          mode: "standalone",
          recordId: null,
          recordSyncUid: "",
          ownerOrganId: null,
          fieldPolicy: "head_body",
        };

        function escapeHtml(value) {
          return String(value ?? "")
            .replace(/&/g, "&amp;")
            .replace(/</g, "&lt;")
            .replace(/>/g, "&gt;")
            .replace(/"/g, "&quot;")
            .replace(/'/g, "&#39;");
        }

        function normalizeContext(context = {}) {
          const recordId = Number(context.recordId || context.record_id || 0) || null;
          return {
            ...DEFAULT_CONTEXT,
            ...context,
            mode: context.mode === "embedded" ? "embedded" : "standalone",
            recordId,
            recordSyncUid: String(context.recordSyncUid || context.record_sync_uid || ""),
            ownerOrganId: Number(context.ownerOrganId || context.owner_organ_id || 0) || null,
            fieldPolicy: String(context.fieldPolicy || context.field_policy || "head_body"),
          };
        }

        async function readJson(response) {
          const body = await response.json().catch(() => null);
          if (!response.ok) {
            throw new Error(body?.error || body?.message || `Request failed with ${response.status}`);
          }
          return body;
        }

        async function api(path, options = {}) {
          const response = await fetch(path, {
            method: options.method || "GET",
            headers: options.body ? { "content-type": "application/json" } : undefined,
            body: options.body ? JSON.stringify(options.body) : undefined,
          });
          return readJson(response);
        }

        function documentUid(record, fieldName) {
          const syncUid = String(record?.sync_uid || record?.syncUid || "");
          return syncUid ? `record:${syncUid}:${fieldName}` : "";
        }

        function emit(root, type, detail = {}) {
          root.dispatchEvent(new CustomEvent(`record-editor:${type}`, {
            bubbles: true,
            detail,
          }));
        }

        function render(root, state) {
          const standalone = state.context.mode === "standalone";
          const hasRecord = Boolean(state.record?.id);
          const bodyOnly = state.context.fieldPolicy === "body_only";
          root.innerHTML = `
            <section class="recordEditor" data-record-editor-ui>
              <div class="recordEditor__bar">
                ${standalone ? `<button class="recordEditor__statusButton" type="button" data-picker-toggle title="Select an existing record"><span class="recordEditor__status"></span></button>` : ""}
                <span class="recordEditor__mode">${standalone ? (hasRecord ? `Record #${escapeHtml(state.record.id)}` : "Draft") : "Embedded record"}</span>
                <span class="recordEditor__spacer"></span>
                <button class="recordEditor__button" type="button" data-preview-toggle>${state.preview ? "Raw" : "MD"}</button>
                <button class="recordEditor__button" data-primary="true" type="button" data-save>${state.saving ? "Saving" : "Save"}</button>
              </div>
              <div class="recordEditor__picker" ${state.pickerOpen && standalone ? "" : "hidden"}>
                <input class="recordEditor__search" type="search" data-search placeholder="Search records" value="${escapeHtml(state.search)}">
                <div class="recordEditor__results">
                  ${state.results.map((record) => `<button class="recordEditor__result" type="button" data-select-record="${escapeHtml(record.id)}">${escapeHtml(record.head || `Record #${record.id}`)}</button>`).join("")}
                </div>
              </div>
              ${bodyOnly ? "" : `<input class="recordEditor__title" data-title placeholder="Title" value="${escapeHtml(state.head)}">`}
              ${state.preview
                ? `<article class="recordEditor__body markdownRender" data-preview>${window.renderMarkdown ? window.renderMarkdown(state.body) : escapeHtml(state.body)}</article>`
                : `<textarea class="recordEditor__body" data-body spellcheck="true" placeholder="Write note body">${escapeHtml(state.body)}</textarea>`}
              <div class="recordEditor__footer ${state.error ? "recordEditor__error" : ""}">${escapeHtml(state.error || state.status || (hasRecord ? "Synced through Record text updates." : "Add a title to create a record."))}</div>
            </section>
          `;
        }

        function bind(root, state, apiObject) {
          root.addEventListener("input", (event) => {
            const title = event.target.closest("[data-title]");
            const body = event.target.closest("[data-body]");
            const search = event.target.closest("[data-search]");
            if (title) {
              state.head = title.value;
              state.dirty = true;
              emit(root, "dirty-changed", { dirty: true });
            }
            if (body) {
              state.body = body.value;
              state.dirty = true;
              emit(root, "dirty-changed", { dirty: true });
            }
            if (search) {
              state.search = search.value;
              void apiObject.search();
            }
          });

          root.addEventListener("click", (event) => {
            if (event.target.closest("[data-picker-toggle]")) {
              state.pickerOpen = !state.pickerOpen;
              render(root, state);
              return;
            }
            if (event.target.closest("[data-preview-toggle]")) {
              state.preview = !state.preview;
              render(root, state);
              return;
            }
            if (event.target.closest("[data-save]")) {
              void apiObject.save();
              return;
            }
            const selected = event.target.closest("[data-select-record]");
            if (selected) {
              void apiObject.load(Number(selected.dataset.selectRecord));
            }
          });

          root.addEventListener("change", (event) => {
            const title = event.target.closest("[data-title]");
            if (
              title &&
              state.context.mode === "standalone" &&
              !state.record?.id &&
              String(state.head || "").trim()
            ) {
              void apiObject.save();
            }
          });

          root.addEventListener("keydown", (event) => {
            if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "s") {
              event.preventDefault();
              void apiObject.save();
            }
          });
        }

        function mount(root, context = {}) {
          const state = {
            context: normalizeContext(context),
            record: null,
            head: "",
            body: "",
            dirty: false,
            saving: false,
            preview: false,
            pickerOpen: false,
            search: "",
            results: [],
            status: "",
            error: "",
          };

          const apiObject = {
            async load(recordId = state.context.recordId) {
              if (!recordId) {
                render(root, state);
                return;
              }
              try {
                state.error = "";
                state.status = "Loading";
                render(root, state);
                const record = await api(`/table/record/${recordId}`);
                state.record = record;
                state.context.recordId = Number(record.id || recordId);
                state.context.recordSyncUid = String(record.sync_uid || "");
                state.context.ownerOrganId = Number(record.owner_organ_id || 0) || null;
                state.head = String(record.head || "");
                state.body = String(record.body || "");
                state.dirty = false;
                state.pickerOpen = false;
                state.status = "Loaded";
                await apiObject.pullCrdt();
                emit(root, "record-updated", { record: state.record });
              } catch (error) {
                state.error = error instanceof Error ? error.message : String(error);
              }
              render(root, state);
            },
            async pullCrdt() {
              if (!state.record?.sync_uid) {
                return;
              }
              for (const field of ["head", "body"]) {
                const uid = documentUid(state.record, field);
                if (!uid) continue;
                await fetch(`/sync/crdt/text/snapshot?document_uid=${encodeURIComponent(uid)}`).catch(() => null);
              }
            },
            async search() {
              try {
                const rows = await api("/table/record");
                const query = state.search.trim().toLowerCase();
                state.results = (Array.isArray(rows) ? rows : [])
                  .filter((record) => !query || String(record.head || "").toLowerCase().includes(query))
                  .slice(0, 20);
                render(root, state);
              } catch (error) {
                state.error = error instanceof Error ? error.message : String(error);
                render(root, state);
              }
            },
            async save() {
              if (state.saving) {
                return;
              }
              if (state.context.mode === "embedded" && !state.record?.id && !state.context.recordId) {
                state.error = "Embedded editor needs a record.";
                render(root, state);
                return;
              }
              const head = state.context.fieldPolicy === "body_only" ? state.head : state.head.trim();
              if (!state.record?.id && !head) {
                state.status = "Draft only";
                render(root, state);
                return;
              }
              state.saving = true;
              state.error = "";
              state.status = "Saving";
              emit(root, "save-state-changed", { saving: true });
              render(root, state);
              try {
                if (!state.record?.id) {
                  const created = await api("/table/record", {
                    method: "POST",
                    body: {
                      quantity: 0,
                      head,
                      body: state.body.trim() ? state.body : null,
                    },
                  });
                  const recordId = Number(
                    created?.row?.id ||
                      created?.id ||
                      created?.last_insert_rowid ||
                      created?.lastInsertRowid ||
                      created?.row_id ||
                      0,
                  );
                  if (!recordId) {
                    const rows = await api("/table/record");
                    const match = (Array.isArray(rows) ? rows : []).find((record) => String(record.head || "") === head);
                    if (!match?.id) throw new Error("Record was created but no id was returned.");
                    await apiObject.load(Number(match.id));
                  } else {
                    emit(root, "record-created", { recordId });
                    await apiObject.load(recordId);
                  }
                } else {
                  await api(`/table/record/${state.record.id}`, {
                    method: "PATCH",
                    body: {
                      head: state.context.fieldPolicy === "body_only" ? state.record.head : head,
                      body: state.body.trim() ? state.body : null,
                    },
                  });
                  await apiObject.load(state.record.id);
                }
                state.dirty = false;
                state.status = "Saved";
                emit(root, "dirty-changed", { dirty: false });
              } catch (error) {
                state.error = error instanceof Error ? error.message : String(error);
              } finally {
                state.saving = false;
                emit(root, "save-state-changed", { saving: false });
                render(root, state);
              }
            },
            context(nextContext) {
              state.context = normalizeContext({ ...state.context, ...nextContext });
              if (state.context.recordId) {
                void apiObject.load(state.context.recordId);
              } else {
                render(root, state);
              }
            },
            destroy() {
              root.innerHTML = "";
            },
          };

          bind(root, state, apiObject);
          render(root, state);
          if (state.context.recordId) {
            void apiObject.load(state.context.recordId);
          }
          return apiObject;
        }

        window.LinceRecordEditor = { mount };
      })();
"##;

pub(super) fn script() -> String {
    let mut script = String::from(crate::sand::shared_markdown::JS_HELPERS);
    script.push_str(EDITOR_RUNTIME);
    script.push_str(
        r##"
      const root = document.querySelector("[data-record-editor-root]");
      const params = new URLSearchParams(window.location.search);
      window.LinceRecordEditor.mount(root, {
        mode: params.get("mode") === "embedded" ? "embedded" : "standalone",
        recordId: Number(params.get("record_id") || params.get("recordId") || 0) || null,
        recordSyncUid: params.get("record_sync_uid") || params.get("recordSyncUid") || "",
        fieldPolicy: params.get("field_policy") || params.get("fieldPolicy") || "head_body",
      });
    "##,
    );
    script
}
