pub(crate) fn script() -> String {
    r###"
    (() => {
        const state = {
            contract: null,
            binding: null,
            lastSnapshot: null,
            source: null,
            discoverRecords: [],
            filteredRecords: [],
            selectedOriginalId: null,
            discoverHydrated: false,
            discoverHydrating: false,
            discoverFilterTimer: null,
            assigneeSuggestions: {
                discover: [],
                create: [],
            },
            assigneeTimers: {
                discover: null,
                create: null,
            },
            assigneeRequestSeq: {
                discover: 0,
                create: 0,
            },
        };

        const elements = {
            status: document.getElementById("trail-status"),
            discoverPanel: document.getElementById("trail-discover-panel"),
            bindingCopy: document.getElementById("trail-binding-copy"),
            overwriteCopy: document.getElementById("trail-overwrite-copy"),
            searchAssignee: document.getElementById("trail-search-assignee"),
            searchAssigneeSuggestions: document.getElementById("trail-search-assignee-suggestions"),
            searchCategory: document.getElementById("trail-search-category"),
            searchHead: document.getElementById("trail-search-head"),
            searchSummary: document.getElementById("trail-search-summary"),
            searchResults: document.getElementById("trail-search-results"),
            createSource: document.getElementById("trail-create-source"),
            createSourceLabel: document.getElementById("trail-create-source-label"),
            createSourceClear: document.getElementById("trail-create-source-clear"),
            createAssignee: document.getElementById("trail-create-assignee"),
            createAssigneeSuggestions: document.getElementById("trail-create-assignee-suggestions"),
            createSubmit: document.getElementById("trail-create-submit"),
            syncScope: document.getElementById("trail-sync-scope"),
            syncScopeCopy: document.getElementById("trail-sync-scope-copy"),
            syncFieldQ: document.getElementById("trail-sync-field-q"),
            syncFieldH: document.getElementById("trail-sync-field-h"),
            syncFieldB: document.getElementById("trail-sync-field-b"),
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

        function valueOf(object, ...keys) {
            for (const key of keys) {
                if (object && object[key] != null) {
                    return object[key];
                }
            }
            return null;
        }

        function parseJsonArray(value) {
            try {
                const parsed = JSON.parse(value || "[]");
                return Array.isArray(parsed) ? parsed : [];
            } catch (_error) {
                return [];
            }
        }

        function normalizeText(value) {
            return String(value ?? "").trim().toLowerCase();
        }

        function splitCsv(value) {
            return String(value ?? "")
                .split(",")
                .map((item) => item.trim().toLowerCase())
                .filter(Boolean);
        }

        function truncateText(value, limit = 160) {
            const text = String(value ?? "").trim();
            if (text.length <= limit) {
                return text;
            }
            return text.slice(0, limit - 1).trimEnd() + "…";
        }

        function rowId(row) {
            return Number(valueOf(row, "id") || 0);
        }

        function rowHead(row) {
            return String(valueOf(row, "head") || "");
        }

        function rowBody(row) {
            return String(valueOf(row, "body") || "");
        }

        function rowPrimaryCategory(row) {
            return String(valueOf(row, "primaryCategory", "primary_category") || "").trim();
        }

        function rowCategories(row) {
            const categories = parseJsonArray(valueOf(row, "categoriesJson", "categories_json"));
            if (categories.length) {
                return categories;
            }
            const primary = rowPrimaryCategory(row);
            return primary ? [primary] : [];
        }

        function rowAssigneeIds(row) {
            return parseJsonArray(valueOf(row, "assigneeIdsJson", "assignee_ids_json")).map((value) => String(value));
        }

        function rowAssigneeNames(row) {
            return parseJsonArray(valueOf(row, "assigneeNamesJson", "assignee_names_json"));
        }

        function rowAssigneeUsernames(row) {
            return parseJsonArray(valueOf(row, "assigneeUsernamesJson", "assignee_usernames_json"));
        }

        function selectedOriginal() {
            return state.discoverRecords.find((row) => rowId(row) === Number(state.selectedOriginalId)) || null;
        }

        function quantityLabel(quantity) {
            if (quantity === 1) return "Done";
            if (quantity === -1) return "Ready";
            return "Locked";
        }

        function scopeCopy(scope) {
            if (scope === "n") return "Node syncs a single record.";
            if (scope === "nt") return "Both syncs the record and its children.";
            return "Tree syncs children only.";
        }

        function normalizeFields(fields) {
            const input = String(fields || "");
            let normalized = "";
            if (input.includes("q")) normalized += "q";
            if (input.includes("h")) normalized += "h";
            if (input.includes("b")) normalized += "b";
            return normalized || "hb";
        }

        function currentFields() {
            let fields = "";
            if (elements.syncFieldQ.checked) fields += "q";
            if (elements.syncFieldH.checked) fields += "h";
            if (elements.syncFieldB.checked) fields += "b";
            return fields;
        }

        function requireFields() {
            const fields = currentFields();
            if (!fields) {
                throw new Error("Select at least one property to sync.");
            }
            return fields;
        }

        function applyFields(fields) {
            const normalized = normalizeFields(fields);
            elements.syncFieldQ.checked = normalized.includes("q");
            elements.syncFieldH.checked = normalized.includes("h");
            elements.syncFieldB.checked = normalized.includes("b");
        }

        function fieldLabelList(fields) {
            const labels = [];
            if (fields.includes("q")) labels.push("quantity");
            if (fields.includes("h")) labels.push("head");
            if (fields.includes("b")) labels.push("body");
            return labels;
        }

        function updateScopeCopy() {
            elements.syncScopeCopy.textContent = scopeCopy(elements.syncScope.value || "t");
        }

        function selectedOriginalSummary(row) {
            const categories = rowCategories(row);
            const parts = ["#" + rowId(row), rowHead(row) || "(untitled)"];
            if (categories.length) {
                parts.push("[" + categories.join(", ") + "]");
            }
            return parts.join(" ");
        }

        function updateCreateState() {
            elements.createSubmit.disabled = !(selectedOriginal() && elements.createAssignee.value.trim());
        }

        function renderSelectedOriginal(preserveDraft = false) {
            const original = selectedOriginal();
            if (original) {
                elements.createSource.value = String(rowId(original));
                elements.createSourceLabel.value = selectedOriginalSummary(original);
            } else {
                elements.createSource.value = "";
                if (!preserveDraft) {
                    elements.createSourceLabel.value = "";
                }
            }
            updateCreateState();
        }

        function clearSelectedOriginal(preserveDraft = false) {
            state.selectedOriginalId = null;
            renderSelectedOriginal(preserveDraft);
        }

        function setSelectedOriginalById(recordId) {
            state.selectedOriginalId = Number(recordId);
            renderSelectedOriginal(false);
            renderDiscoverSummary();
            renderSearchResults();
        }

        function focusDiscoverPanel() {
            elements.discoverPanel.scrollIntoView({ behavior: "smooth", block: "start" });
        }

        function localRecordMatches(row) {
            const headFilter = normalizeText(elements.searchHead.value);
            const categoryFilters = splitCsv(elements.searchCategory.value);
            const assigneeFilter = normalizeText(elements.searchAssignee.value);

            if (headFilter) {
                const headText = rowHead(row).toLowerCase();
                const bodyText = rowBody(row).toLowerCase();
                const idText = String(rowId(row));
                if (!headText.includes(headFilter) && !bodyText.includes(headFilter) && !idText.includes(headFilter)) {
                    return false;
                }
            }

            if (categoryFilters.length) {
                const categories = rowCategories(row).map((value) => normalizeText(value));
                const matchesCategory = categoryFilters.some((filterValue) =>
                    categories.some((categoryValue) => categoryValue.includes(filterValue))
                );
                if (!matchesCategory) {
                    return false;
                }
            }

            if (assigneeFilter) {
                const candidates = [
                    ...rowAssigneeIds(row),
                    ...rowAssigneeNames(row),
                    ...rowAssigneeUsernames(row),
                ].map((value) => normalizeText(value));
                const matchesAssignee = candidates.some((value) => value.includes(assigneeFilter));
                if (!matchesAssignee) {
                    return false;
                }
            }

            return true;
        }

        function renderDiscoverSummary() {
            if (!state.discoverHydrated) {
                elements.searchSummary.textContent = "Loading records...";
                return;
            }
            let text = "Showing " + state.filteredRecords.length + " of " + state.discoverRecords.length + " fetched records.";
            const original = selectedOriginal();
            if (original) {
                text += " Selected original: #" + rowId(original) + ".";
            }
            elements.searchSummary.textContent = text;
        }

        function renderSearchResults() {
            if (!state.discoverHydrated) {
                elements.searchResults.innerHTML = "<p class=\"small emptyState\">Loading records...</p>";
                return;
            }

            if (!state.filteredRecords.length) {
                elements.searchResults.innerHTML = "<p class=\"small emptyState\">No fetched records match the current filters.</p>";
                return;
            }

            elements.searchResults.innerHTML = state.filteredRecords.map((row) => {
                const id = rowId(row);
                const categories = rowCategories(row);
                const assigneeNames = rowAssigneeNames(row);
                const assigneeUsernames = rowAssigneeUsernames(row);
                const selectedClass = id === Number(state.selectedOriginalId) ? " isSelected" : "";
                const excerpt = truncateText(rowBody(row));
                return `
                    <article class="resultCard${selectedClass}" data-select-record="${id}">
                        <div class="row">
                            <div>
                                <strong>#${id} ${escapeHtml(rowHead(row) || "(untitled)")}</strong>
                                ${excerpt ? `<p class="resultExcerpt">${escapeHtml(excerpt)}</p>` : ""}
                                <div class="resultMeta">
                                    ${categories.map((value) => `<span class="pill">${escapeHtml(value)}</span>`).join(" ")}
                                    ${assigneeNames.map((value, index) => {
                                        const username = assigneeUsernames[index];
                                        const label = username ? `${value} (@${username})` : value;
                                        return `<span class="pill">${escapeHtml(label)}</span>`;
                                    }).join(" ")}
                                </div>
                            </div>
                            <div class="rowActions">
                                <button class="button" type="button" data-bind-root="${id}">Open trail</button>
                            </div>
                        </div>
                    </article>
                `;
            }).join("");
        }

        function applyDiscoverFilters() {
            state.filteredRecords = state.discoverRecords.filter(localRecordMatches);
            renderDiscoverSummary();
            renderSearchResults();
        }

        function scheduleDiscoverFiltering() {
            clearTimeout(state.discoverFilterTimer);
            state.discoverFilterTimer = window.setTimeout(() => {
                if (!state.discoverHydrated) {
                    hydrateDiscoverRecords().catch((error) => setStatus(error.message));
                    return;
                }
                applyDiscoverFilters();
            }, 140);
        }

        function suggestionElements(kind) {
            if (kind === "discover") {
                return {
                    input: elements.searchAssignee,
                    panel: elements.searchAssigneeSuggestions,
                };
            }
            return {
                input: elements.createAssignee,
                panel: elements.createAssigneeSuggestions,
            };
        }

        function hideAssigneeSuggestions(kind) {
            const { panel } = suggestionElements(kind);
            state.assigneeSuggestions[kind] = [];
            panel.hidden = true;
            panel.innerHTML = "";
        }

        function renderAssigneeSuggestions(kind) {
            const { panel } = suggestionElements(kind);
            const rows = state.assigneeSuggestions[kind] || [];
            if (!rows.length) {
                hideAssigneeSuggestions(kind);
                return;
            }
            panel.innerHTML = rows.map((row) => {
                const preferredValue = row.username || String(row.id);
                return `
                    <button
                        class="suggestionButton"
                        type="button"
                        data-assignee-kind="${kind}"
                        data-assignee-value="${escapeHtml(preferredValue)}"
                    >
                        <strong>#${escapeHtml(row.id)} ${escapeHtml(row.name)}</strong>
                        <span class="suggestionMeta">@${escapeHtml(row.username)}</span>
                    </button>
                `;
            }).join("");
            panel.hidden = false;
        }

        function applyAssigneeSuggestion(kind, value) {
            const { input } = suggestionElements(kind);
            input.value = value;
            hideAssigneeSuggestions(kind);
            if (kind === "discover") {
                scheduleDiscoverFiltering();
            } else {
                updateCreateState();
            }
        }

        function scheduleAssigneeLookup(kind) {
            clearTimeout(state.assigneeTimers[kind]);
            const { input } = suggestionElements(kind);
            const query = input.value.trim();
            if (!query) {
                hideAssigneeSuggestions(kind);
                return;
            }

            state.assigneeTimers[kind] = window.setTimeout(async () => {
                const requestSeq = ++state.assigneeRequestSeq[kind];
                try {
                    const result = await postJson("search-assignees", { query });
                    if (requestSeq !== state.assigneeRequestSeq[kind]) {
                        return;
                    }
                    state.assigneeSuggestions[kind] = Array.isArray(result.results) ? result.results : [];
                    renderAssigneeSuggestions(kind);
                } catch (error) {
                    console.error(error);
                }
            }, 140);
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

        async function hydrateDiscoverRecords() {
            if (state.discoverHydrating) {
                return;
            }
            state.discoverHydrating = true;
            setStatus("Loading discover records...");
            try {
                const result = await postJson("search-trails", {
                    assignee: null,
                    category: null,
                    headContains: null,
                });
                state.discoverRecords = Array.isArray(result.results) ? result.results : [];
                state.discoverHydrated = true;
                if (state.selectedOriginalId && !selectedOriginal()) {
                    clearSelectedOriginal(false);
                }
                applyDiscoverFilters();
                if (!state.binding?.trailRootRecordId) {
                    setStatus("Ready");
                }
            } finally {
                state.discoverHydrating = false;
            }
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
                applyFields(state.binding.sync.fields || "hb");
            } else {
                elements.syncScope.value = "t";
                applyFields("hb");
            }

            updateScopeCopy();
            renderSelectedOriginal(false);
            renderBinding();
            await hydrateDiscoverRecords();
            if (state.binding?.trailRootRecordId) {
                connectStream();
            }
        }

        function renderBinding() {
            const binding = state.binding;
            elements.syncSubmit.disabled = !binding?.trailRootRecordId;

            if (!binding?.trailRootRecordId) {
                elements.bindingCopy.textContent = "No trail bound.";
                elements.overwriteCopy.hidden = true;
                elements.overwriteCopy.textContent = "";
                return;
            }

            elements.bindingCopy.textContent =
                "Bound root #" + binding.trailRootRecordId + " · view " + (binding.viewId ?? "?");

            if (binding.sync) {
                const fields = normalizeFields(binding.sync.fields || "hb");
                const overwritten = fieldLabelList(fields);
                const preserved = ["quantity", "head", "body"].filter((label) => !overwritten.includes(label));
                const overwrittenCopy = overwritten.length ? overwritten.join(", ") : "nothing";
                const preservedCopy = preserved.length ? preserved.join(", ") : "nothing";
                elements.syncScope.value = binding.sync.scope || "t";
                applyFields(fields);
                updateScopeCopy();
                elements.overwriteCopy.hidden = false;
                elements.overwriteCopy.textContent =
                    "Sync source #" + binding.sync.syncSourceRecordId +
                    " overwrites " + overwrittenCopy +
                    " and preserves " + preservedCopy + ".";
            } else {
                elements.overwriteCopy.hidden = true;
                elements.overwriteCopy.textContent = "";
            }
        }

        function renderTree(snapshot) {
            const rows = snapshot?.rows || [];
            if (!rows.length) {
                elements.tree.innerHTML = "<p class=\"small emptyState\">No trail rows yet.</p>";
                return;
            }

            elements.tree.innerHTML = rows.map((row) => {
                const depth = Number(valueOf(row, "depth") || 0);
                const categories = parseJsonArray(valueOf(row, "categoriesJson", "categories_json"));
                const recordId = valueOf(row, "id");
                return `
                    <article class="trailNode" style="margin-left:${depth * 20}px">
                        <div class="row">
                            <div>
                                <strong>#${escapeHtml(recordId)} ${escapeHtml(valueOf(row, "head") || "")}</strong>
                                <div class="nodeMeta">
                                    <span class="pill">${quantityLabel(Number(valueOf(row, "quantity") || 0))}</span>
                                    ${categories.map((value) => `<span class="pill">${escapeHtml(value)}</span>`).join(" ")}
                                </div>
                            </div>
                            <div class="rowActions">
                                <button class="button" type="button" data-set-quantity="${escapeHtml(recordId)}" data-quantity="0">Lock</button>
                                <button class="button" type="button" data-set-quantity="${escapeHtml(recordId)}" data-quantity="-1">Ready</button>
                                <button class="button buttonAccent" type="button" data-set-quantity="${escapeHtml(recordId)}" data-quantity="1">Done</button>
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

        async function handleCreateTrail() {
            try {
                setStatus("Creating trail...");
                const original = selectedOriginal();
                if (!original) {
                    throw new Error("Select an original record in Discover before creating a trail.");
                }
                const result = await postJson("create-trail", {
                    sourceRecordId: rowId(original),
                    assignee: elements.createAssignee.value.trim(),
                    scope: elements.syncScope.value || "t",
                    fields: requireFields(),
                });
                state.binding = result.detail || null;
                renderBinding();
                connectStream();
                setStatus("Trail created. Waiting for sync stream...");
            } catch (error) {
                setStatus(error.message);
            }
        }

        elements.searchHead.addEventListener("input", scheduleDiscoverFiltering);
        elements.searchCategory.addEventListener("input", scheduleDiscoverFiltering);
        elements.searchAssignee.addEventListener("input", () => {
            scheduleDiscoverFiltering();
            scheduleAssigneeLookup("discover");
        });

        elements.searchResults.addEventListener("click", async (event) => {
            const bindButton = event.target.closest("[data-bind-root]");
            if (bindButton) {
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
                return;
            }

            const resultCard = event.target.closest("[data-select-record]");
            if (!resultCard) {
                return;
            }
            setSelectedOriginalById(Number(resultCard.getAttribute("data-select-record")));
        });

        elements.createSubmit.addEventListener("click", handleCreateTrail);
        elements.createAssignee.addEventListener("input", () => {
            updateCreateState();
            scheduleAssigneeLookup("create");
        });

        elements.createSourceLabel.addEventListener("focus", focusDiscoverPanel);
        elements.createSourceLabel.addEventListener("click", focusDiscoverPanel);
        elements.createSourceLabel.addEventListener("input", () => {
            clearSelectedOriginal(true);
            elements.searchHead.value = elements.createSourceLabel.value;
            scheduleDiscoverFiltering();
        });

        elements.createSourceClear.addEventListener("click", () => {
            clearSelectedOriginal(false);
            elements.searchHead.value = "";
            scheduleDiscoverFiltering();
            focusDiscoverPanel();
        });

        elements.syncSubmit.addEventListener("click", async () => {
            try {
                setStatus("Running sync...");
                const result = await postJson("run-trail-sync", {
                    trailRootRecordId: state.binding?.trailRootRecordId || null,
                    scope: elements.syncScope.value || "t",
                    fields: requireFields(),
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

        elements.syncScope.addEventListener("change", updateScopeCopy);
        [elements.syncFieldQ, elements.syncFieldH, elements.syncFieldB].forEach((input) => {
            input.addEventListener("change", () => {
                if (!currentFields()) {
                    elements.overwriteCopy.hidden = false;
                    elements.overwriteCopy.textContent = "Select at least one property to sync.";
                } else if (state.binding?.sync) {
                    renderBinding();
                } else {
                    elements.overwriteCopy.hidden = true;
                    elements.overwriteCopy.textContent = "";
                }
            });
        });

        elements.searchAssigneeSuggestions.addEventListener("click", (event) => {
            const button = event.target.closest("[data-assignee-kind]");
            if (!button) {
                return;
            }
            applyAssigneeSuggestion(
                button.getAttribute("data-assignee-kind"),
                button.getAttribute("data-assignee-value") || "",
            );
        });

        elements.createAssigneeSuggestions.addEventListener("click", (event) => {
            const button = event.target.closest("[data-assignee-kind]");
            if (!button) {
                return;
            }
            applyAssigneeSuggestion(
                button.getAttribute("data-assignee-kind"),
                button.getAttribute("data-assignee-value") || "",
            );
        });

        document.addEventListener("click", (event) => {
            if (!event.target.closest(".autocompleteHost")) {
                hideAssigneeSuggestions("discover");
                hideAssigneeSuggestions("create");
            }
        });

        loadContract().catch((error) => {
            setStatus(error.message || "Failed to load Trail Relation");
        });
    })();
    "###.to_string()
}
