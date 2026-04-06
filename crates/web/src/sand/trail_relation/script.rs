pub(crate) fn script() -> String {
    r#"
    (() => {
        const state = {
            contract: null,
            binding: null,
            lastSnapshot: null,
            source: null,
        };

        const elements = {
            status: document.getElementById("trail-status"),
            bindingCopy: document.getElementById("trail-binding-copy"),
            overwriteCopy: document.getElementById("trail-overwrite-copy"),
            searchForm: document.getElementById("trail-search-form"),
            searchAssignee: document.getElementById("trail-search-assignee"),
            searchCategory: document.getElementById("trail-search-category"),
            searchHead: document.getElementById("trail-search-head"),
            searchResults: document.getElementById("trail-search-results"),
            createForm: document.getElementById("trail-create-form"),
            createSource: document.getElementById("trail-create-source"),
            createAssignee: document.getElementById("trail-create-assignee"),
            syncScope: document.getElementById("trail-sync-scope"),
            syncFields: document.getElementById("trail-sync-fields"),
            syncSubmit: document.getElementById("trail-sync-submit"),
            tree: document.getElementById("trail-tree"),
        };

        function instanceId() {
            return window.frameElement?.dataset?.packageInstanceId || "preview";
        }

        function contractUrl() {
            return "/host/widgets/" + encodeURIComponent(instanceId()) + "/contract";
        }

        function streamUrl() {
            return "/host/widgets/" + encodeURIComponent(instanceId()) + "/stream";
        }

        function actionUrl(action) {
            return "/host/widgets/" + encodeURIComponent(instanceId()) + "/actions/" + encodeURIComponent(action);
        }

        function setStatus(text) {
            elements.status.textContent = text;
        }

        function escapeHtml(value) {
            return String(value ?? "")
                .replaceAll("&", "&amp;")
                .replaceAll("<", "&lt;")
                .replaceAll(">", "&gt;")
                .replaceAll("\"", "&quot;")
                .replaceAll("'", "&#39;");
        }

        function quantityLabel(quantity) {
            if (quantity === 1) return "Done";
            if (quantity === -1) return "Ready";
            return "Locked";
        }

        async function postJson(action, payload) {
            const response = await fetch(actionUrl(action), {
                method: "POST",
                credentials: "same-origin",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(payload || {}),
            });
            const data = await response.json().catch(() => ({}));
            if (!response.ok) {
                throw new Error(data?.message || data?.error || ("Request failed with " + response.status));
            }
            return data;
        }

        async function loadContract() {
            setStatus("Loading contract...");
            const response = await fetch(contractUrl(), { credentials: "same-origin" });
            const data = await response.json();
            if (!response.ok) {
                throw new Error(data?.message || data?.error || "Failed to load contract.");
            }
            state.contract = data;
            state.binding = data?.binding || null;
            if (state.binding?.sync) {
                elements.syncScope.value = state.binding.sync.scope || "t";
                elements.syncFields.value = state.binding.sync.fields || "hb";
            }
            renderBinding();
            setStatus("Ready");
            if (state.binding?.trailRootRecordId) {
                connectStream();
            }
        }

        function renderBinding() {
            const binding = state.binding;
            if (!binding?.trailRootRecordId) {
                elements.bindingCopy.textContent = "No trail bound.";
                elements.overwriteCopy.hidden = true;
                elements.overwriteCopy.textContent = "";
                return;
            }
            elements.bindingCopy.textContent =
                "Bound root #" + binding.trailRootRecordId + " · view " + (binding.viewId ?? "?");
            if (binding.sync) {
                const fields = binding.sync.fields || "qhb";
                const overwritten = [];
                const preserved = [];
                if (fields.includes("q")) overwritten.push("quantity"); else preserved.push("quantity");
                if (fields.includes("h")) overwritten.push("head"); else preserved.push("head");
                if (fields.includes("b")) overwritten.push("body"); else preserved.push("body");
                elements.overwriteCopy.hidden = false;
                elements.overwriteCopy.textContent =
                    "Sync source #" + binding.sync.syncSourceRecordId +
                    " overwrites " + overwritten.join(", ") +
                    " and preserves " + preserved.join(", ") + ".";
            } else {
                elements.overwriteCopy.hidden = true;
                elements.overwriteCopy.textContent = "";
            }
        }

        function renderSearchResults(results) {
            if (!Array.isArray(results) || results.length === 0) {
                elements.searchResults.innerHTML = "<p class='small'>No records matched these filters.</p>";
                return;
            }
            elements.searchResults.innerHTML = results.map((row) => {
                const categories = JSON.parse(row.categoriesJson || "[]");
                const assignees = JSON.parse(row.assigneeNamesJson || "[]");
                return `
                    <article class="resultCard">
                        <div class="row">
                            <div>
                                <strong>#${row.id} ${escapeHtml(row.head || "(untitled)")}</strong>
                                <div class="resultMeta">
                                    ${(categories || []).map((value) => `<span class="pill">${escapeHtml(value)}</span>`).join(" ")}
                                    ${(assignees || []).map((value) => `<span class="pill">${escapeHtml(value)}</span>`).join(" ")}
                                </div>
                            </div>
                            <div class="rowActions">
                                <button class="button" data-bind-root="${row.id}">Open</button>
                                <button class="button" data-use-source="${row.id}">Use as source</button>
                            </div>
                        </div>
                    </article>
                `;
            }).join("");
        }

        function renderTree(snapshot) {
            const rows = snapshot?.rows || [];
            if (!rows.length) {
                elements.tree.innerHTML = "<p class='small'>No trail rows yet.</p>";
                return;
            }
            elements.tree.innerHTML = rows.map((row) => {
                const depth = Number(row.depth || 0);
                return `
                    <article class="trailNode" style="margin-left:${depth * 20}px">
                        <div class="row">
                            <div>
                                <strong>#${escapeHtml(row.id)} ${escapeHtml(row.head)}</strong>
                                <div class="nodeMeta">
                                    <span class="pill">${quantityLabel(Number(row.quantity))}</span>
                                    ${(JSON.parse(row.categories_json || "[]") || []).map((value) => `<span class="pill">${escapeHtml(value)}</span>`).join(" ")}
                                </div>
                            </div>
                            <div class="rowActions">
                                <button class="button" data-set-quantity="${row.id}" data-quantity="0">Lock</button>
                                <button class="button" data-set-quantity="${row.id}" data-quantity="-1">Ready</button>
                                <button class="button buttonAccent" data-set-quantity="${row.id}" data-quantity="1">Done</button>
                            </div>
                        </div>
                    </article>
                `;
            }).join("");
        }

        function connectStream() {
            if (state.source) {
                state.source.close();
            }
            const source = new EventSource(streamUrl(), { withCredentials: true });
            state.source = source;
            setStatus("Connecting stream...");
            source.addEventListener("trail-sync", (event) => {
                const payload = JSON.parse(event.data);
                state.binding = payload.binding || state.binding;
                state.lastSnapshot = payload.snapshot || null;
                renderBinding();
                renderTree(state.lastSnapshot);
                setStatus("Live");
            });
            source.addEventListener("trail-error", (event) => {
                const payload = JSON.parse(event.data);
                setStatus(payload.message || "Trail stream error");
            });
            source.onerror = () => {
                setStatus("Trail stream disconnected");
            };
        }

        elements.searchForm.addEventListener("submit", async (event) => {
            event.preventDefault();
            try {
                setStatus("Searching records...");
                const result = await postJson("search-trails", {
                    assigneeId: elements.searchAssignee.value ? Number(elements.searchAssignee.value) : null,
                    category: elements.searchCategory.value || null,
                    headContains: elements.searchHead.value || null,
                });
                renderSearchResults(result.results || []);
                setStatus("Search finished");
            } catch (error) {
                setStatus(error.message);
            }
        });

        elements.searchResults.addEventListener("click", async (event) => {
            const bindButton = event.target.closest("[data-bind-root]");
            const useSourceButton = event.target.closest("[data-use-source]");
            if (useSourceButton) {
                elements.createSource.value = useSourceButton.getAttribute("data-use-source") || "";
                return;
            }
            if (!bindButton) {
                return;
            }
            try {
                setStatus("Binding trail...");
                const result = await postJson("bind-trail", {
                    trailRootRecordId: Number(bindButton.getAttribute("data-bind-root")),
                });
                state.binding = result.detail || null;
                renderBinding();
                connectStream();
            } catch (error) {
                setStatus(error.message);
            }
        });

        elements.createForm.addEventListener("submit", async (event) => {
            event.preventDefault();
            try {
                setStatus("Creating trail...");
                const result = await postJson("create-trail", {
                    sourceRecordId: Number(elements.createSource.value),
                    assigneeId: Number(elements.createAssignee.value),
                    scope: elements.syncScope.value || "t",
                    fields: elements.syncFields.value || "hb",
                });
                state.binding = result.detail || null;
                renderBinding();
                connectStream();
            } catch (error) {
                setStatus(error.message);
            }
        });

        elements.syncSubmit.addEventListener("click", async () => {
            try {
                setStatus("Running sync...");
                const result = await postJson("run-trail-sync", {
                    trailRootRecordId: state.binding?.trailRootRecordId || null,
                    scope: elements.syncScope.value || "t",
                    fields: elements.syncFields.value || "hb",
                });
                state.binding = {
                    ...(state.binding || {}),
                    ...(result.detail || {}),
                };
                renderBinding();
                setStatus("Sync requested");
            } catch (error) {
                setStatus(error.message);
            }
        });

        elements.tree.addEventListener("click", async (event) => {
            const button = event.target.closest("[data-set-quantity]");
            if (!button || !state.binding?.trailRootRecordId) {
                return;
            }
            try {
                const recordId = Number(button.getAttribute("data-set-quantity"));
                const quantity = Number(button.getAttribute("data-quantity"));
                setStatus("Updating trail...");
                await postJson("set-trail-quantity", {
                    trailRootRecordId: state.binding.trailRootRecordId,
                    recordId,
                    quantity,
                });
                setStatus("Trail updated");
            } catch (error) {
                setStatus(error.message);
            }
        });

        loadContract().catch((error) => {
            setStatus(error.message || "Failed to load Trail Relation");
        });
    })();
    "#.to_string()
}
