pub(crate) fn script() -> String {
    let mut script = String::from(include_str!("logic.js"));
    script.push_str(
    r####"
    (() => {
        const d3 = window.d3;
        const TrailRelationLogic = globalThis.TrailRelationLogic;
        let datastarApi = null;
        const datastarReady = import("/static/vendored/datastar.js")
            .then((module) => {
                datastarApi = module;
                return module;
            })
            .catch(() => null);
        const trailBootstrap = document.getElementById("trail-stream-bootstrap");

        const DEFAULT_PHYSICS = {
            charge: -200,
            distance: 110,
            collision: 24,
        };

        const state = {
            binding: null,
            sourceServerId: null,
            snapshot: null,
            stream: null,
            selectedOriginal: null,
            selectedNodeId: null,
            originalRecordRequestSeq: 0,
            originalRecordCatalogLoaded: false,
            originalRecordCatalogLoadPromise: null,
            originalRecordCatalog: [],
            originalRecordSuggestions: [],
            assigneeRequestSeq: {
                create: 0,
            },
            assigneeSuggestions: {
                create: [],
            },
            pendingQuantityChanges: new Map(),
            physics: { ...DEFAULT_PHYSICS },
            graph: {
                svg: null,
                viewport: null,
                linkLayer: null,
                nodeLayer: null,
                labelLayer: null,
                zoom: null,
                zoomScale: 1,
                simulation: null,
                resizeObserver: null,
                width: 0,
                height: 0,
                nodes: [],
                links: [],
                needsFit: true,
            },
        };

        const elements = {
            statusPill: document.getElementById("trail-status-pill"),
            boundPill: document.getElementById("trail-bound-pill"),
            rowPill: document.getElementById("trail-row-pill"),
            linkPill: document.getElementById("trail-link-pill"),
            viewPill: document.getElementById("trail-view-pill"),
            zoomPill: document.getElementById("trail-zoom-pill"),
            graphStage: document.getElementById("trail-graph-stage"),
            graph: document.getElementById("trail-graph"),
            graphEmpty: document.getElementById("trail-empty"),
            zoomOutButton: document.getElementById("trail-zoom-out"),
            zoomInButton: document.getElementById("trail-zoom-in"),
            fitButton: document.getElementById("trail-fit"),
            centerButton: document.getElementById("trail-center"),
            originalRecordInput: document.getElementById("trail-original-record"),
            originalRecordSuggestions: document.getElementById("trail-original-record-suggestions"),
            selectedOriginalTitle: document.getElementById("trail-selected-original"),
            selectedOriginalCopy: document.getElementById("trail-selected-original-copy"),
            createAssignee: document.getElementById("trail-create-assignee"),
            createAssigneeSuggestions: document.getElementById("trail-create-assignee-suggestions"),
            viewName: document.getElementById("trail-view-name"),
            createSubmit: document.getElementById("trail-create-submit"),
            physicsCharge: document.getElementById("trail-physics-charge"),
            physicsDistance: document.getElementById("trail-physics-distance"),
            physicsCollision: document.getElementById("trail-physics-collision"),
        };

        function instanceId() {
            return window.frameElement?.dataset?.packageInstanceId || "preview";
        }

        function currentFrameServerId() {
            return String(window.frameElement?.dataset?.linceServerId || "").trim();
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

        function recordCollectionUrl() {
            const serverId = String(state.sourceServerId || currentFrameServerId() || "").trim();
            if (!serverId) {
                return null;
            }
            return "/host/integrations/servers/" + encodeURIComponent(serverId) + "/table/record";
        }

        function boundTrailStorageKey() {
            return "lince.widget.trail_relation." + instanceId() + ".boundTrailRoot";
        }

        function readStoredTrailRoot() {
            try {
                const value = Number(window.localStorage?.getItem?.(boundTrailStorageKey()) || 0);
                return Number.isFinite(value) && value > 0 ? value : null;
            } catch (_error) {
                return null;
            }
        }

        function writeStoredTrailRoot(recordId) {
            try {
                if (Number(recordId) > 0) {
                    window.localStorage?.setItem?.(boundTrailStorageKey(), String(recordId));
                }
            } catch (_error) {
            }
        }

        function setStatus(text) {
            elements.statusPill.textContent = text;
        }

        function escapeHtml(value) {
            return String(value ?? "")
                .replaceAll("&", "&amp;")
                .replaceAll("<", "&lt;")
                .replaceAll(">", "&gt;")
                .replaceAll("\"", "&quot;")
                .replaceAll("'", "&#39;");
        }

        function cloneJsonValue(value, fallback = null) {
            try {
                if (value === undefined) {
                    return fallback;
                }
                return JSON.parse(JSON.stringify(value));
            } catch (_error) {
                return fallback;
            }
        }

        function withDatastar(callback) {
            if (datastarApi) {
                callback(datastarApi);
                return;
            }
            void datastarReady.then((module) => {
                if (module) {
                    callback(module);
                }
            });
        }

        function readSignalPath(path) {
            if (datastarApi?.getPath) {
                return cloneJsonValue(datastarApi.getPath(path), null);
            }
            return null;
        }

        function patchTrailSignals(patch) {
            const safePatch = cloneJsonValue(patch, null);
            if (!safePatch || typeof safePatch !== "object") {
                return;
            }
            withDatastar((module) => {
                module?.mergePatch?.(safePatch);
            });
        }

        function parseJsonArray(value) {
            try {
                const parsed = JSON.parse(value || "[]");
                return Array.isArray(parsed) ? parsed : [];
            } catch (_error) {
                return [];
            }
        }

        function valueOf(object, ...keys) {
            for (const key of keys) {
                if (object && object[key] != null) {
                    return object[key];
                }
            }
            return null;
        }

        function normalizeText(value) {
            return String(value ?? "").trim().toLowerCase();
        }

        function nodeIdFromRow(row) {
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

        function rowAssigneeNames(row) {
            return parseJsonArray(valueOf(row, "assigneeNamesJson", "assignee_names_json"));
        }

        function rowAssigneeUsernames(row) {
            return parseJsonArray(valueOf(row, "assigneeUsernamesJson", "assignee_usernames_json"));
        }

        function rowParentIds(row) {
            return parseJsonArray(valueOf(row, "parentIdsJson", "parent_ids_json")).map((value) => Number(value)).filter(Number.isFinite);
        }

        function computeTrailQuantityChanges(recordId, quantity) {
            return TrailRelationLogic.computeTrailQuantityChanges(
                state.snapshot?.rows,
                state.binding?.trailRootRecordId,
                recordId,
                quantity,
            );
        }

        function selectedNode() {
            return state.graph.nodes.find((node) => node.id === Number(state.selectedNodeId)) || null;
        }

        function normalizeFields(fields) {
            const text = String(fields || "");
            let normalized = "";
            if (text.includes("q")) normalized += "q";
            if (text.includes("h")) normalized += "h";
            if (text.includes("b")) normalized += "b";
            return normalized || "hb";
        }

        function currentFields() {
            return "hb";
        }

        function quantityLabel(quantity) {
            if (quantity === 1) return "Done";
            if (quantity === -1) return "Ready";
            return "Locked";
        }

        function normalizedQuantity(value) {
            return TrailRelationLogic.normalizedQuantity(value);
        }

        function scopeCopy(scope) {
            return scope ? String(scope) : "";
        }

        function updateScopeCopy() {
            return;
        }

        function renderZoomPill(scale) {
            const resolved = Number.isFinite(scale) ? scale : state.graph.zoomScale || 1;
            state.graph.zoomScale = resolved;
            elements.zoomPill.textContent = Math.round(resolved * 100) + "%";
        }

        function resetLocalTrailProjection() {
            state.pendingQuantityChanges.clear();
            state.snapshot = null;
            state.selectedNodeId = null;
            renderSelection();
            renderGraph();
        }

        function cloneSnapshot(snapshot) {
            return snapshot
                ? {
                    ...snapshot,
                    rows: Array.isArray(snapshot.rows)
                        ? snapshot.rows.map((row) => ({ ...row }))
                        : [],
                }
                : null;
        }

        function projectLoadedTrailSnapshot() {
            if (!state.binding?.trailRootRecordId || !Array.isArray(state.snapshot?.rows)) {
                return;
            }
            const rootRecordId = Number(state.binding.trailRootRecordId);
            const rootRow = state.snapshot.rows.find((row) => nodeIdFromRow(row) === rootRecordId) || null;
            if (!rootRow || normalizedQuantity(valueOf(rootRow, "quantity")) !== 1) {
                return;
            }

            const projection = TrailRelationLogic.computeTrailQuantityChanges(
                state.snapshot.rows,
                rootRecordId,
                rootRecordId,
                1,
            );
            if (projection.error || !Array.isArray(projection.changes) || !projection.changes.length) {
                return;
            }

            applyQuantityChanges(projection.changes);
        }

        function syncFromSignals(patch = null) {
            const trail = patch?.trail && typeof patch.trail === "object"
                ? patch.trail
                : readSignalPath("trail");
            if (!trail) {
                return false;
            }

            state.sourceServerId =
                String(trail?.source?.serverId || state.sourceServerId || currentFrameServerId() || "").trim() || null;
            state.binding = trail?.binding ? cloneJsonValue(trail.binding, null) : null;
            state.snapshot = cloneSnapshot(state.binding?.snapshot);
            mergeOriginalRecordCatalog(state.snapshot?.rows);

            projectLoadedTrailSnapshot();
            reconcilePendingQuantityChanges();
            renderBinding();
            renderSelectedOriginal();
            renderGraph();

            if (trail?.stream?.status === "error") {
                setStatus(trail.stream.error || "Stream error");
            } else if (state.binding?.trailRootRecordId) {
                setStatus("Live");
            } else {
                setStatus("Ready");
            }

            return true;
        }

        function applyTrailPatchToState(patch) {
            if (!patch || typeof patch !== "object") {
                return;
            }

            const trail = patch.trail;
            if (!trail || typeof trail !== "object") {
                return;
            }

            if (trail.binding && typeof trail.binding === "object") {
                state.binding = {
                    ...(state.binding || {}),
                    ...cloneJsonValue(trail.binding, {}),
                };
                if (trail.binding.snapshot) {
                    state.snapshot = cloneSnapshot(trail.binding.snapshot);
                }
            }

            if (trail.stream && typeof trail.stream === "object") {
                state.binding = state.binding
                    ? {
                        ...(state.binding || {}),
                        stream: {
                            ...(state.binding.stream || {}),
                            ...cloneJsonValue(trail.stream, {}),
                        },
                    }
                    : state.binding;
            }
        }

        function recordPendingQuantityChanges(changes) {
            (Array.isArray(changes) ? changes : []).forEach((entry) => {
                state.pendingQuantityChanges.set(
                    Number(entry.recordId),
                    normalizedQuantity(entry.quantity),
                );
            });
        }

        function reconcilePendingQuantityChanges() {
            if (!Array.isArray(state.snapshot?.rows) || !state.pendingQuantityChanges.size) {
                return;
            }
            const rowsById = new Map(
                state.snapshot.rows.map((row) => [nodeIdFromRow(row), row]),
            );
            for (const [recordId, quantity] of state.pendingQuantityChanges.entries()) {
                const row = rowsById.get(recordId);
                if (row && normalizedQuantity(valueOf(row, "quantity")) === quantity) {
                    state.pendingQuantityChanges.delete(recordId);
                }
            }
            if (!state.pendingQuantityChanges.size) {
                return;
            }
            state.snapshot.rows = state.snapshot.rows.map((row) => {
                const recordId = nodeIdFromRow(row);
                if (!state.pendingQuantityChanges.has(recordId)) {
                    return row;
                }
                return {
                    ...row,
                    quantity: state.pendingQuantityChanges.get(recordId),
                };
            });
        }

        function truncateLabel(value, limit = 18) {
            const text = String(value || "").trim();
            if (text.length <= limit) {
                return text || "(untitled)";
            }
            return text.slice(0, limit - 1).trimEnd() + "…";
        }

        function nodeRadius(node) {
            return 18 + Math.min(Number(node.childrenCount || 0), 6);
        }

        function nodeFill(quantity) {
            if (quantity === 1) return "#7ef0c6";
            if (quantity === -1) return "#f2bb78";
            return "#64748b";
        }

        function postJson(action, payload) {
            return fetch(actionUrl(action), {
                method: "POST",
                credentials: "same-origin",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(payload || {}),
            }).then(async (response) => {
                const data = await response.json().catch(() => ({}));
                if (!response.ok) {
                    throw new Error(data?.message || data?.error || ("Request failed with " + response.status));
                }
                return data;
            });
        }

        function patchRecordRows(rows) {
            const url = recordCollectionUrl();
            if (!url) {
                return Promise.reject(new Error("Trail widget has no serverId bound."));
            }
            return fetch(url, {
                method: "PATCH",
                credentials: "same-origin",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify(rows || []),
            }).then(async (response) => {
                const data = await response.json().catch(() => ({}));
                if (!response.ok) {
                    throw new Error(data?.message || data?.error || ("Request failed with " + response.status));
                }
                return data;
            });
        }

        function fetchViewSnapshot(viewId) {
            const url = viewSnapshotUrl(viewId);
            if (!url) {
                return Promise.reject(new Error("Trail widget has no serverId/viewId bound."));
            }
            return fetch(url, {
                method: "GET",
                credentials: "same-origin",
            }).then(async (response) => {
                const data = await response.json().catch(() => ({}));
                if (!response.ok) {
                    throw new Error(data?.message || data?.error || ("Request failed with " + response.status));
                }
                return data;
            });
        }

        function fetchRecordRow(recordId) {
            const url = recordCollectionUrl();
            if (!url) {
                return Promise.reject(new Error("Trail widget has no serverId bound."));
            }
            return fetch(url + "/" + Number(recordId), {
                method: "GET",
                credentials: "same-origin",
            }).then(async (response) => {
                const data = await response.json().catch(() => ({}));
                if (!response.ok) {
                    throw new Error(data?.message || data?.error || ("Request failed with " + response.status));
                }
                return data;
            });
        }

        function originalRecordSuggestionElements() {
            return {
                input: elements.originalRecordInput,
                panel: elements.originalRecordSuggestions,
            };
        }

        function hideOriginalRecordSuggestions() {
            const { panel } = originalRecordSuggestionElements();
            panel.hidden = true;
            panel.innerHTML = "";
            state.originalRecordSuggestions = [];
        }

        function mergeOriginalRecordCatalog(rows) {
            const catalog = new Map(
                (Array.isArray(state.originalRecordCatalog) ? state.originalRecordCatalog : [])
                    .map((row) => [nodeIdFromRow(row), { ...row }]),
            );
            (Array.isArray(rows) ? rows : []).forEach((row) => {
                const recordId = nodeIdFromRow(row);
                if (recordId > 0) {
                    catalog.set(recordId, { ...row });
                }
            });
            state.originalRecordCatalog = Array.from(catalog.values());
        }

        function originalRecordLabel(row) {
            return "#" + nodeIdFromRow(row) + " " + (rowHead(row) || "(untitled)");
        }

        function originalRecordMeta(row) {
            const categories = rowCategories(row);
            const assignees = rowAssigneeNames(row);
            const meta = [];
            if (categories.length) {
                meta.push("Categories: " + categories.join(", "));
            }
            if (assignees.length) {
                meta.push("Assignees: " + assignees.join(", "));
            }
            return meta;
        }

        function originalRecordMatchesQuery(row, query) {
            const needle = normalizeText(query);
            if (!needle) {
                return true;
            }
            const idText = String(nodeIdFromRow(row));
            const head = normalizeText(rowHead(row));
            const body = normalizeText(rowBody(row));
            const categories = normalizeText(rowCategories(row).join(" "));
            return (
                idText.includes(needle) ||
                head.includes(needle) ||
                body.includes(needle) ||
                categories.includes(needle)
            );
        }

        function filteredOriginalRecordSuggestions() {
            const query = elements.originalRecordInput.value.trim();
            const rows = Array.isArray(state.originalRecordCatalog) ? state.originalRecordCatalog : [];
            const filtered = rows
                .filter((row) => originalRecordMatchesQuery(row, query))
                .sort((left, right) => {
                    const needle = normalizeText(query);
                    const leftId = String(nodeIdFromRow(left));
                    const rightId = String(nodeIdFromRow(right));
                    const leftHead = normalizeText(rowHead(left));
                    const rightHead = normalizeText(rowHead(right));
                    const leftExact = leftId === needle || leftHead === needle;
                    const rightExact = rightId === needle || rightHead === needle;
                    if (leftExact !== rightExact) {
                        return leftExact ? -1 : 1;
                    }
                    const leftPrefix = leftHead.startsWith(needle) || leftId.startsWith(needle);
                    const rightPrefix = rightHead.startsWith(needle) || rightId.startsWith(needle);
                    if (leftPrefix !== rightPrefix) {
                        return leftPrefix ? -1 : 1;
                    }
                    return Number(leftId) - Number(rightId);
                });
            return filtered.slice(0, 12);
        }

        function renderOriginalRecordSuggestions() {
            const { panel } = originalRecordSuggestionElements();
            const rows = filteredOriginalRecordSuggestions();
            state.originalRecordSuggestions = rows;
            if (!rows.length) {
                hideOriginalRecordSuggestions();
                return;
            }
            panel.innerHTML = rows.map((row) => `
                <button
                    class="suggestionButton"
                    type="button"
                    data-original-record-id="${escapeHtml(String(nodeIdFromRow(row)))}"
                >
                    <strong>${escapeHtml(originalRecordLabel(row))}</strong>
                    <span class="suggestionMeta">${escapeHtml(originalRecordMeta(row).join(" · ") || "Type to narrow the list.")}</span>
                </button>
            `).join("");
            panel.hidden = false;
        }

        function selectOriginalRecord(row) {
            if (!row) {
                return;
            }
            state.selectedOriginal = {
                ...row,
                categories: rowCategories(row),
                assigneeNames: rowAssigneeNames(row),
                assigneeUsernames: rowAssigneeUsernames(row),
            };
            elements.originalRecordInput.value = originalRecordLabel(row);
            hideOriginalRecordSuggestions();
            renderSelectedOriginal();
        }

        function loadOriginalRecordCatalog() {
            if (state.originalRecordCatalogLoaded) {
                renderOriginalRecordSuggestions();
                return Promise.resolve(state.originalRecordCatalog);
            }
            if (state.originalRecordCatalogLoadPromise) {
                return state.originalRecordCatalogLoadPromise;
            }
            const url = recordCollectionUrl();
            if (!url) {
                return Promise.resolve([]);
            }
            const requestSeq = ++state.originalRecordRequestSeq;
            state.originalRecordCatalogLoadPromise = postJson("search-trails", {
                headContains: null,
                category: null,
                assignee: null,
            }).then((result) => {
                if (requestSeq !== state.originalRecordRequestSeq) {
                    return [];
                }
                state.originalRecordCatalogLoaded = true;
                state.originalRecordCatalog = Array.isArray(result.results)
                    ? result.results.map((row) => ({ ...row }))
                    : [];
                renderOriginalRecordSuggestions();
                return state.originalRecordCatalog;
            }).catch((error) => {
                console.error(error);
                return [];
            }).finally(() => {
                state.originalRecordCatalogLoadPromise = null;
            });
            return state.originalRecordCatalogLoadPromise;
        }

        function renderSelectedOriginal() {
            if (!state.selectedOriginal) {
                elements.selectedOriginalTitle.textContent = "No original selected";
                elements.selectedOriginalCopy.textContent = "Select a graph node or type a record above to use it as the original record.";
                elements.createSubmit.disabled = true;
                return;
            }

            const categories = rowCategories(state.selectedOriginal);
            elements.selectedOriginalTitle.textContent =
                "#" + nodeIdFromRow(state.selectedOriginal) + " " + (rowHead(state.selectedOriginal) || "(untitled)");
            elements.selectedOriginalCopy.textContent = categories.length
                ? "Categories: " + categories.join(", ")
                : "No categories on the selected original.";
            elements.createSubmit.disabled =
                !elements.createAssignee.value.trim() || !elements.viewName.value.trim();
        }

        function renderBinding() {
            if (!state.binding?.trailRootRecordId) {
                elements.boundPill.textContent = "No trail bound";
                elements.viewPill.textContent = "No view";
                return;
            }

            writeStoredTrailRoot(state.binding.trailRootRecordId);
            elements.boundPill.textContent = "Trail root #" + state.binding.trailRootRecordId;
            elements.viewPill.textContent = "View #" + (state.binding.viewId ?? "?");
        }

        function renderSelection() {
            return;
        }

        function suggestionElements() {
            return {
                input: elements.createAssignee,
                panel: elements.createAssigneeSuggestions,
            };
        }

        function hideAssigneeSuggestions() {
            const { panel } = suggestionElements();
            panel.hidden = true;
            panel.innerHTML = "";
            state.assigneeSuggestions.create = [];
        }

        function renderAssigneeSuggestions() {
            const rows = state.assigneeSuggestions.create || [];
            const { panel } = suggestionElements();
            if (!rows.length) {
                hideAssigneeSuggestions();
                return;
            }
            panel.innerHTML = rows.map((row) => `
                <button
                    class="suggestionButton"
                    type="button"
                    data-assignee-value="${escapeHtml(row.username || String(row.id))}"
                >
                    <strong>#${escapeHtml(row.id)} ${escapeHtml(row.name)}</strong>
                    <span class="suggestionMeta">@${escapeHtml(row.username)}</span>
                </button>
            `).join("");
            panel.hidden = false;
        }

        function requestAssigneeSuggestions() {
            const { input } = suggestionElements();
            const query = input.value.trim();
            if (!query) {
                hideAssigneeSuggestions();
                return;
            }
            const requestSeq = ++state.assigneeRequestSeq.create;
            postJson("search-assignees", { query })
                .then((result) => {
                    if (requestSeq !== state.assigneeRequestSeq.create) {
                        return;
                    }
                    state.assigneeSuggestions.create = Array.isArray(result.results) ? result.results : [];
                    renderAssigneeSuggestions();
                })
                .catch((error) => {
                    console.error(error);
                });
        }

        function applyAssigneeSuggestion(value) {
            const { input } = suggestionElements();
            input.value = value;
            hideAssigneeSuggestions();
            renderSelectedOriginal();
        }

        function initializeGraph() {
            if (!d3) {
                setStatus("Missing d3");
                return;
            }

            const svg = d3.select(elements.graph);
            svg.selectAll("*").remove();

            const defs = svg.append("defs");
            defs.append("marker")
                .attr("id", "trail-arrow")
                .attr("viewBox", "0 -5 10 10")
                .attr("refX", 20)
                .attr("refY", 0)
                .attr("markerWidth", 7)
                .attr("markerHeight", 7)
                .attr("orient", "auto")
                .append("path")
                .attr("fill", "rgba(120, 215, 255, 0.78)")
                .attr("d", "M0,-5L10,0L0,5");

            const viewport = svg.append("g").attr("class", "viewport");
            const linkLayer = viewport.append("g");
            const nodeLayer = viewport.append("g");
            const labelLayer = viewport.append("g");

            const zoom = d3.zoom()
                .scaleExtent([0.25, 3.5])
                .on("zoom", (event) => {
                    viewport.attr("transform", event.transform);
                    renderZoomPill(event.transform.k);
                });

            svg.call(zoom).on("dblclick.zoom", null);
            svg.on("click", () => {
                state.selectedNodeId = null;
                renderSelection();
                updateGraphSelection();
            });

            state.graph.svg = svg;
            state.graph.viewport = viewport;
            state.graph.linkLayer = linkLayer;
            state.graph.nodeLayer = nodeLayer;
            state.graph.labelLayer = labelLayer;
            state.graph.zoom = zoom;
            renderZoomPill(1);

            state.graph.resizeObserver = new ResizeObserver(() => {
                resizeGraph();
            });
            state.graph.resizeObserver.observe(elements.graphStage);
            resizeGraph();
        }

        function resizeGraph() {
            if (!state.graph.svg) {
                return;
            }
            const width = Math.max(elements.graphStage.clientWidth, 240);
            const height = Math.max(elements.graphStage.clientHeight, 240);
            state.graph.width = width;
            state.graph.height = height;
            state.graph.svg.attr("viewBox", `0 0 ${width} ${height}`);
            if (state.graph.simulation) {
                state.graph.simulation
                    .force("x", d3.forceX(width / 2).strength(0.08))
                    .force("y", d3.forceY(height / 2).strength(0.08))
                    .alpha(0.25)
                    .restart();
            }
        }

        function visibleTrailRows(rows) {
            return Array.isArray(rows) ? rows : [];
        }

        function buildGraphData(rows) {
            const previous = new Map(state.graph.nodes.map((node) => [node.id, node]));
            const nodes = rows.map((row) => {
                const id = nodeIdFromRow(row);
                const previousNode = previous.get(id);
                return {
                    id,
                    head: rowHead(row),
                    body: rowBody(row),
                    quantity: normalizedQuantity(valueOf(row, "quantity")),
                    categories: rowCategories(row),
                    assigneeNames: rowAssigneeNames(row),
                    assigneeUsernames: rowAssigneeUsernames(row),
                    childrenCount: Number(valueOf(row, "childrenCount", "children_count") || 0),
                    depth: Number(valueOf(row, "depth") || 0),
                    x: previousNode?.x ?? (state.graph.width / 2) + ((Math.random() - 0.5) * 40),
                    y: previousNode?.y ?? (state.graph.height / 2) + ((Math.random() - 0.5) * 40),
                    vx: previousNode?.vx ?? 0,
                    vy: previousNode?.vy ?? 0,
                };
            });

            const nodeIds = new Set(nodes.map((node) => node.id));
            const seenLinks = new Set();
            const links = [];

            rows.forEach((row) => {
                const childId = nodeIdFromRow(row);
                rowParentIds(row).forEach((parentId) => {
                    if (!nodeIds.has(parentId)) {
                        return;
                    }
                    const key = parentId + "->" + childId;
                    if (seenLinks.has(key)) {
                        return;
                    }
                    seenLinks.add(key);
                    links.push({
                        id: key,
                        source: parentId,
                        target: childId,
                    });
                });
            });

            return { nodes, links };
        }

        function dragBehaviour(simulation) {
            return d3.drag()
                .on("start", (event, node) => {
                    event.sourceEvent.stopPropagation();
                    if (!event.active) {
                        simulation.alphaTarget(0.25).restart();
                    }
                    node.fx = node.x;
                    node.fy = node.y;
                })
                .on("drag", (event, node) => {
                    node.fx = event.x;
                    node.fy = event.y;
                })
                .on("end", (event, node) => {
                    if (!event.active) {
                        simulation.alphaTarget(0);
                    }
                    node.fx = null;
                    node.fy = null;
                });
        }

        function updateGraphSelection() {
            if (!state.graph.nodeLayer) {
                return;
            }
            state.graph.nodeLayer.selectAll("circle")
                .classed("is-selected", (node) => node.id === Number(state.selectedNodeId));
        }

        function fitGraph() {
            if (!state.graph.svg || !state.graph.nodes.length) {
                return;
            }

            const xs = state.graph.nodes.map((node) => node.x).filter(Number.isFinite);
            const ys = state.graph.nodes.map((node) => node.y).filter(Number.isFinite);
            if (!xs.length || !ys.length) {
                return;
            }

            const minX = Math.min(...xs);
            const maxX = Math.max(...xs);
            const minY = Math.min(...ys);
            const maxY = Math.max(...ys);
            const width = Math.max(maxX - minX, 80);
            const height = Math.max(maxY - minY, 80);
            const scale = Math.max(0.35, Math.min(2.2, 0.86 / Math.max(width / state.graph.width, height / state.graph.height)));
            const tx = (state.graph.width - (minX + maxX) * scale) / 2;
            const ty = (state.graph.height - (minY + maxY) * scale) / 2;
            const transform = d3.zoomIdentity.translate(tx, ty).scale(scale);
            state.graph.svg.transition().duration(220).call(state.graph.zoom.transform, transform);
            state.graph.needsFit = false;
        }

        function centerGraph() {
            if (!state.graph.svg || !state.graph.nodes.length) {
                return;
            }
            const avgX = state.graph.nodes.reduce((sum, node) => sum + node.x, 0) / state.graph.nodes.length;
            const avgY = state.graph.nodes.reduce((sum, node) => sum + node.y, 0) / state.graph.nodes.length;
            const transform = d3.zoomIdentity
                .translate((state.graph.width / 2) - avgX, (state.graph.height / 2) - avgY)
                .scale(1);
            state.graph.svg.transition().duration(220).call(state.graph.zoom.transform, transform);
        }

        function zoomBy(multiplier) {
            if (!state.graph.svg || !state.graph.zoom) {
                return;
            }
            const current = d3.zoomTransform(elements.graph);
            const nextScale = Math.max(0.25, Math.min(3.5, current.k * multiplier));
            const centerX = state.graph.width / 2;
            const centerY = state.graph.height / 2;
            const transform = d3.zoomIdentity
                .translate(centerX - ((centerX - current.x) / current.k) * nextScale, centerY - ((centerY - current.y) / current.k) * nextScale)
                .scale(nextScale);
            state.graph.svg.transition().duration(180).call(state.graph.zoom.transform, transform);
        }

        function trimmedLinkCoordinates(link) {
            const source = link.source;
            const target = link.target;
            const dx = target.x - source.x;
            const dy = target.y - source.y;
            const distance = Math.hypot(dx, dy) || 1;
            const ux = dx / distance;
            const uy = dy / distance;
            const sourceRadius = nodeRadius(source) + 2;
            const targetRadius = nodeRadius(target) + 8;
            return {
                x1: source.x + ux * sourceRadius,
                y1: source.y + uy * sourceRadius,
                x2: target.x - ux * targetRadius,
                y2: target.y - uy * targetRadius,
            };
        }

        function applyPhysics() {
            if (!state.graph.simulation) {
                return;
            }
            state.graph.simulation
                .force("charge", d3.forceManyBody().strength(state.physics.charge))
                .force("link", d3.forceLink(state.graph.links).id((linkNode) => linkNode.id).distance(state.physics.distance).strength(0.9))
                .force("collision", d3.forceCollide().radius((node) => nodeRadius(node) + state.physics.collision))
                .force("x", d3.forceX(state.graph.width / 2).strength(0.08))
                .force("y", d3.forceY(state.graph.height / 2).strength(0.08))
                .alpha(0.7)
                .restart();
        }

        function renderGraph() {
            if (!state.graph.svg) {
                return;
            }
            const rows = visibleTrailRows(state.snapshot?.rows);
            mergeOriginalRecordCatalog(rows);
            const { nodes, links } = buildGraphData(rows);

            state.graph.nodes = nodes;
            state.graph.links = links;

            elements.rowPill.textContent = nodes.length + " nodes";
            elements.linkPill.textContent = links.length + " links";
            elements.graphEmpty.hidden = nodes.length > 0;

            if (!nodes.length) {
                state.selectedNodeId = null;
                renderSelection();
                state.graph.linkLayer.selectAll("*").remove();
                state.graph.nodeLayer.selectAll("*").remove();
                state.graph.labelLayer.selectAll("*").remove();
                if (state.graph.simulation) {
                    state.graph.simulation.stop();
                }
                return;
            }

            if (state.selectedNodeId && !nodes.some((node) => node.id === Number(state.selectedNodeId))) {
                state.selectedNodeId = null;
            }

            const simulation = d3.forceSimulation(nodes)
                .force("link", d3.forceLink(links).id((node) => node.id).distance(state.physics.distance).strength(0.9))
                .force("charge", d3.forceManyBody().strength(state.physics.charge))
                .force("collision", d3.forceCollide().radius((node) => nodeRadius(node) + state.physics.collision))
                .force("x", d3.forceX(state.graph.width / 2).strength(0.08))
                .force("y", d3.forceY(state.graph.height / 2).strength(0.08));

            const linkSelection = state.graph.linkLayer
                .selectAll("line")
                .data(links, (link) => link.id)
                .join("line")
                .attr("class", "node-link")
                .attr("marker-end", "url(#trail-arrow)");

            const nodeSelection = state.graph.nodeLayer
                .selectAll("g")
                .data(nodes, (node) => node.id)
                .join((enter) => {
                    const group = enter.append("g");
                    group.attr("data-node-id", (node) => node.id);
                    group.append("circle").attr("class", "node-circle");
                    group.on("click", (event, node) => {
                        event.stopPropagation();
                        state.selectedNodeId = node.id;
                        renderSelection();
                        updateGraphSelection();
                    });
                    return group;
                })
                .call(dragBehaviour(simulation));

            nodeSelection.select("circle")
                .attr("r", (node) => nodeRadius(node))
                .attr("fill", (node) => nodeFill(node.quantity));
            nodeSelection.attr("data-node-id", (node) => node.id);

            const labelSelection = state.graph.labelLayer
                .selectAll("text")
                .data(nodes, (node) => node.id)
                .join("text")
                .attr("class", "node-label")
                .text((node) => truncateLabel(node.head));

            simulation.on("tick", () => {
                linkSelection
                    .attr("x1", (link) => trimmedLinkCoordinates(link).x1)
                    .attr("y1", (link) => trimmedLinkCoordinates(link).y1)
                    .attr("x2", (link) => trimmedLinkCoordinates(link).x2)
                    .attr("y2", (link) => trimmedLinkCoordinates(link).y2);

                nodeSelection.attr("transform", (node) => `translate(${node.x},${node.y})`);
                labelSelection
                    .attr("x", (node) => node.x)
                    .attr("y", (node) => node.y + nodeRadius(node) + 14);
            });

            state.graph.simulation?.stop();
            state.graph.simulation = simulation;
            renderSelection();
            updateGraphSelection();
            if (state.graph.needsFit) {
                window.setTimeout(fitGraph, 80);
            }
        }

        function connectStream() {
            if (state.stream) {
                state.stream.close();
            }
            const source = new EventSource(streamUrl(), { withCredentials: true });
            state.stream = source;
            setStatus("Connecting");
            source.addEventListener("trail-sync", (event) => {
                try {
                    const payload = JSON.parse(event.data || "{}");
                    state.binding = payload.binding || state.binding;
                    state.snapshot = cloneSnapshot(payload.snapshot);
                    reconcilePendingQuantityChanges();
                    writeStoredTrailRoot(state.binding?.trailRootRecordId);
                    renderBinding();
                    renderSelectedOriginal();
                    renderGraph();
                    setStatus(state.binding?.trailRootRecordId ? "Live" : "Ready");
                } catch (error) {
                    setStatus(error.message || "Stream error");
                }
            });
            source.addEventListener("datastar-patch-signals", (event) => {
                try {
                    const patch = JSON.parse(event.data || "{}");
                    if (datastarApi?.mergePatch) {
                        datastarApi.mergePatch(patch);
                    } else {
                        applyTrailPatchToState(patch);
                    }
                    syncFromSignals(patch);
                    writeStoredTrailRoot(state.binding?.trailRootRecordId);
                    setStatus(state.binding?.trailRootRecordId ? "Live" : "Ready");
                } catch (error) {
                    setStatus(error.message || "Stream error");
                }
            });
            source.onerror = () => {
                setStatus("Stream disconnected");
            };
        }

        function bindTrailRoot(recordId) {
            const trailRootRecordId = Number(recordId);
            setStatus(Number.isFinite(trailRootRecordId) && trailRootRecordId > 0
                ? "Trail binding moved to edit mode"
                : "Invalid trail root");
        }

        function createTrail() {
            if (!state.selectedOriginal) {
                setStatus("Select an original");
                return;
            }
            const viewName = elements.viewName.value.trim();
            if (!viewName) {
                setStatus("Type a view name");
                return;
            }
            setStatus("Creating");
            postJson("create-trail", {
                sourceRecordId: nodeIdFromRow(state.selectedOriginal),
                assignee: elements.createAssignee.value.trim(),
                viewName,
            }).then((result) => {
                const detail = result.detail || null;
                state.binding = detail || null;
                state.sourceServerId = state.sourceServerId || currentFrameServerId() || null;
                const snapshot = detail?.snapshot
                    ? {
                        ...detail.snapshot,
                        rows: Array.isArray(detail.snapshot.rows)
                            ? detail.snapshot.rows.map((row) => ({ ...row }))
                            : [],
                    }
                    : null;
                state.snapshot = snapshot;
                patchTrailSignals({
                    binding: detail,
                });
                writeStoredTrailRoot(state.binding?.trailRootRecordId);
                renderBinding();
                renderGraph();
                connectStream();
                setStatus("Trail created");
            }).catch((error) => {
                setStatus(error.message);
            });
        }

        function applyQuantityChanges(changes) {
            if (!Array.isArray(state.snapshot?.rows)) {
                return;
            }
            const byId = new Map(
                (Array.isArray(changes) ? changes : [])
                    .map((entry) => [Number(entry.recordId), normalizedQuantity(entry.quantity)]),
            );
            if (!byId.size) {
                return;
            }
            state.snapshot.rows = state.snapshot.rows.map((row) => {
                const recordId = nodeIdFromRow(row);
                if (!byId.has(recordId)) {
                    return row;
                }
                return {
                    ...row,
                    quantity: byId.get(recordId),
                };
            });
        }

        function trailResetChanges(rows, trailRootRecordId) {
            return (Array.isArray(rows) ? rows : []).map((row) => ({
                recordId: nodeIdFromRow(row),
                quantity: nodeIdFromRow(row) === Number(trailRootRecordId) ? -1 : 0,
            }));
        }

        function initializeTrail() {
            setStatus("Trail reset is disabled");
        }

        function runSync() {
            setStatus("Trail sync is disabled");
        }

        function setSelectedNodeQuantity(quantity) {
            void quantity;
            setStatus("Trail progression is disabled");
        }

        function loadContract() {
            setStatus("Loading");
            return datastarReady.then(() => {
                if ((trailBootstrap || readSignalPath("trail")) && syncFromSignals()) {
                    void loadOriginalRecordCatalog();
                    if (state.binding?.trailRootRecordId) {
                        writeStoredTrailRoot(state.binding.trailRootRecordId);
                    }
                    return;
                }

                return fetch(contractUrl(), { credentials: "same-origin" })
                    .then((response) => response.json().then((data) => ({ ok: response.ok, data })))
                    .then(({ ok, data }) => {
                        if (!ok) {
                            throw new Error(data?.message || data?.error || "Failed to load contract.");
                        }
                        state.binding = data?.binding || null;
                        state.sourceServerId = data?.source?.serverId || currentFrameServerId() || null;
                        renderBinding();
                        renderSelectedOriginal();
                    })
                    .then(() => loadOriginalRecordCatalog())
                    .then(() => {
                        if (state.binding?.trailRootRecordId) {
                            writeStoredTrailRoot(state.binding.trailRootRecordId);
                            connectStream();
                        } else {
                            setStatus("Ready");
                        }
                    });
            });
        }

        window.TrailWidget = window.TrailWidget || {};
        window.TrailWidget.syncFromSignals = syncFromSignals;
        window.TrailWidget.patchTrailSignals = patchTrailSignals;

        initializeGraph();
        renderSelection();

        elements.originalRecordInput.addEventListener("input", () => {
            if (!elements.originalRecordInput.value.trim()) {
                hideOriginalRecordSuggestions();
                renderSelectedOriginal();
                return;
            }
            if (!state.originalRecordCatalogLoaded) {
                loadOriginalRecordCatalog().finally(() => {
                    renderOriginalRecordSuggestions();
                });
                return;
            }
            renderOriginalRecordSuggestions();
        });
        elements.originalRecordInput.addEventListener("keydown", (event) => {
            if (event.key !== "Enter") {
                return;
            }
            event.preventDefault();
            const query = elements.originalRecordInput.value.trim();
            if (!query) {
                return;
            }
            const exactId = Number(query.replace(/^#/, ""));
            const exactRow = Number.isFinite(exactId) && exactId > 0
                ? state.originalRecordCatalog.find((candidate) => nodeIdFromRow(candidate) === exactId) || null
                : null;
            if (exactRow) {
                selectOriginalRecord(exactRow);
                return;
            }
            const firstRow = filteredOriginalRecordSuggestions()[0] || null;
            if (firstRow) {
                selectOriginalRecord(firstRow);
            }
        });

        elements.createAssignee.addEventListener("input", () => {
            renderSelectedOriginal();
            requestAssigneeSuggestions();
        });
        elements.viewName.addEventListener("input", () => {
            renderSelectedOriginal();
        });

        elements.createAssigneeSuggestions.addEventListener("click", (event) => {
            const button = event.target.closest("[data-assignee-value]");
            if (!button) {
                return;
            }
            applyAssigneeSuggestion(button.getAttribute("data-assignee-value") || "");
        });

        elements.originalRecordSuggestions.addEventListener("click", (event) => {
            const button = event.target.closest("[data-original-record-id]");
            if (!button) {
                return;
            }
            const recordId = Number(button.getAttribute("data-original-record-id") || 0);
            const row = state.originalRecordCatalog.find((candidate) => nodeIdFromRow(candidate) === recordId) || null;
            if (row) {
                selectOriginalRecord(row);
            }
        });

        elements.createSubmit.addEventListener("click", createTrail);

        elements.physicsCharge.addEventListener("input", () => {
            state.physics.charge = Number(elements.physicsCharge.value);
            applyPhysics();
        });
        elements.physicsDistance.addEventListener("input", () => {
            state.physics.distance = Number(elements.physicsDistance.value);
            applyPhysics();
        });
        elements.physicsCollision.addEventListener("input", () => {
            state.physics.collision = Number(elements.physicsCollision.value);
            applyPhysics();
        });

        elements.fitButton.addEventListener("click", fitGraph);
        elements.centerButton.addEventListener("click", centerGraph);
        elements.zoomOutButton.addEventListener("click", () => zoomBy(1 / 1.18));
        elements.zoomInButton.addEventListener("click", () => zoomBy(1.18));

        document.addEventListener("click", (event) => {
            if (!event.target.closest(".field.autocompleteHost")) {
                hideOriginalRecordSuggestions();
            }
            if (!event.target.closest(".autocompleteHost")) {
                hideAssigneeSuggestions();
            }
        });

        loadContract().catch((error) => {
            setStatus(error.message || "Failed to load Trail Relation");
        });
    })();
    "####);
    script
}
