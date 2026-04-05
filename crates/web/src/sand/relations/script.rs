pub(super) fn script() -> String {
    r##"
(function () {
    const bridge = window.LinceWidgetHost || null;
    const frame = window.frameElement;
    const app = document.getElementById("app");
    const canvas = document.getElementById("graph");
    const ctx = canvas.getContext("2d");
    const editor = document.getElementById("editor");
    const modePill = document.getElementById("mode-pill");
    const originPill = document.getElementById("origin-pill");
    const statusPill = document.getElementById("status-pill");
    const rowPill = document.getElementById("row-pill");
    const linkPill = document.getElementById("link-pill");
    const filterPill = document.getElementById("filter-pill");
    const emptyState = document.getElementById("empty-state");
    const originText = document.getElementById("origin-text");
    const selectionPill = document.getElementById("selection-pill");
    const selectionEmpty = document.getElementById("selection-empty");
    const selectionContent = document.getElementById("selection-content");
    const selectionSummary = document.getElementById("selection-summary");
    const childList = document.getElementById("child-list");
    const parentSelect = document.getElementById("parent-select");
    const parentHeadQuery = document.getElementById("parent-head-query");
    const categoryInput = document.getElementById("category-input");
    const categoryAddButton = document.getElementById("category-add");
    const selectedCategoryList = document.getElementById("selected-category-list");
    const categoryFilterList = document.getElementById("category-filter-list");
    const chargeInput = document.getElementById("physics-charge");
    const chargeValue = document.getElementById("physics-charge-value");
    const distanceInput = document.getElementById("physics-distance");
    const distanceValue = document.getElementById("physics-distance-value");
    const collisionInput = document.getElementById("physics-collision");
    const collisionValue = document.getElementById("physics-collision-value");
    const centerInput = document.getElementById("physics-center");
    const centerValue = document.getElementById("physics-center-value");
    const applyFiltersButton = document.getElementById("apply-filters");
    const clearFiltersButton = document.getElementById("clear-filters");
    const resetPhysicsButton = document.getElementById("reset-physics");
    const connectParentButton = document.getElementById("connect-parent");
    const disconnectParentButton = document.getElementById("disconnect-parent");

    const DEFAULT_PHYSICS = {
        charge: -220,
        linkDistance: 110,
        collisionRadius: 24,
        centerForce: 0.18,
    };

    const state = {
        mode: "view",
        origin: {
            serverId: String(frame?.dataset?.linceServerId || "").trim(),
            viewId: Number(frame?.dataset?.linceViewId || 0) || null,
        },
        cardState: {},
        snapshot: null,
        rows: [],
        nodes: [],
        links: [],
        categories: [],
        availableCategories: [],
        draftFilters: {
            categories: [],
            parentHeadQuery: "",
        },
        physics: { ...DEFAULT_PHYSICS },
        selectedId: null,
        selectedParentId: null,
        reconnectTimer: null,
        stream: null,
        streamGeneration: 0,
        loading: false,
        error: "",
        width: 0,
        height: 0,
        resizeObserver: null,
        simulation: null,
        needsRedraw: false,
    };

    function instanceId() {
        return String(frame?.dataset?.packageInstanceId || "preview").trim() || "preview";
    }

    function clampNumber(value, min, max, fallback) {
        const parsed = Number(value);
        if (!Number.isFinite(parsed)) {
            return fallback;
        }
        return Math.min(max, Math.max(min, parsed));
    }

    function escapeHtml(value) {
        return String(value ?? "")
            .replaceAll("&", "&amp;")
            .replaceAll("<", "&lt;")
            .replaceAll(">", "&gt;")
            .replaceAll("\"", "&quot;")
            .replaceAll("'", "&#39;");
    }

    function uniqueStrings(values) {
        const seen = new Set();
        const out = [];
        for (const raw of Array.isArray(values) ? values : []) {
            const value = String(raw || "").trim();
            if (!value) {
                continue;
            }
            const key = value.toLowerCase();
            if (seen.has(key)) {
                continue;
            }
            seen.add(key);
            out.push(value);
        }
        return out;
    }

    function normalizePhysics(raw) {
        const next = raw && typeof raw === "object" ? raw : {};
        return {
            charge: clampNumber(next.charge, -600, -20, DEFAULT_PHYSICS.charge),
            linkDistance: clampNumber(next.linkDistance, 30, 240, DEFAULT_PHYSICS.linkDistance),
            collisionRadius: clampNumber(next.collisionRadius, 10, 60, DEFAULT_PHYSICS.collisionRadius),
            centerForce: clampNumber(next.centerForce, 0, 1, DEFAULT_PHYSICS.centerForce),
        };
    }

    function normalizeDraftFilters(raw) {
        const next = {
            categories: [],
            parentHeadQuery: "",
        };
        const rows = Array.isArray(raw) ? raw : [];
        for (const row of rows) {
            const field = String(row?.field || "");
            if (field === "categories_any_json" && Array.isArray(row?.value)) {
                next.categories = uniqueStrings(row.value);
            } else if (field === "parent_head_query") {
                next.parentHeadQuery = String(row?.value || "").trim();
            }
        }
        return next;
    }

    function buildFilterRows() {
        const rows = [];
        if (state.draftFilters.categories.length) {
            rows.push({
                field: "categories_any_json",
                operator: "any_of",
                value: state.draftFilters.categories.slice(),
            });
        }
        if (state.draftFilters.parentHeadQuery.trim()) {
            rows.push({
                field: "parent_head_query",
                operator: "contains",
                value: state.draftFilters.parentHeadQuery.trim(),
            });
        }
        return rows;
    }

    function getMeta() {
        return bridge?.getMeta?.() || {};
    }

    function syncFromBridge() {
        const meta = getMeta();
        const cardState = bridge?.getCardState?.() || {};
        state.mode = meta.mode === "edit" ? "edit" : "view";
        state.cardState = cardState && typeof cardState === "object" ? cardState : {};
        state.draftFilters = normalizeDraftFilters(state.cardState.filters || []);
        state.physics = normalizePhysics(
            state.cardState.relations_physics || state.cardState.physics || state.cardState.relationsPhysics,
        );
        state.origin.serverId = String(meta.serverId || state.origin.serverId || frame?.dataset?.linceServerId || "").trim();
        state.origin.viewId = Number(meta.viewId || state.origin.viewId || frame?.dataset?.linceViewId || 0) || null;
        renderMode();
        renderOrigin();
        renderFilterControls();
        renderPhysicsControls();
        renderFilterPill();
    }

    function renderMode() {
        document.body.dataset.mode = state.mode;
        modePill.textContent = state.mode === "edit" ? "edit" : "view";
        editor.hidden = false;
        editor.dataset.mode = state.mode;
    }

    function renderOrigin() {
        const serverId = state.origin.serverId || "local";
        const viewId = state.origin.viewId == null ? "none" : String(state.origin.viewId);
        originPill.textContent = `${serverId} / view ${viewId}`;
        originText.textContent = `serverId: ${serverId}\nviewId: ${viewId}\nmode: ${state.mode}`;
    }

    function renderStatus(label, tone, detail) {
        statusPill.textContent = label;
        statusPill.dataset.tone = tone || "neutral";
        statusPill.dataset.detail = detail || "";
    }

    function renderCounters() {
        rowPill.textContent = `${state.rows.length} nodes`;
        linkPill.textContent = `${state.links.length} links`;
        filterPill.textContent = `${buildFilterRows().length} filters`;
    }

    function renderFilterPill() {
        renderCounters();
    }

    function renderCategoryChoices() {
        const categories = state.availableCategories.length ? state.availableCategories : state.categories;
        const selected = new Set(state.draftFilters.categories.map((value) => value.toLowerCase()));
        selectedCategoryList.innerHTML = state.draftFilters.categories.length
            ? state.draftFilters.categories
                  .map((category) => {
                      const safe = escapeHtml(category);
                      return `
                          <button class="chipButton" type="button" data-category-remove="${safe}" title="Remove category">
                              <span>${safe}</span>
                              <span aria-hidden="true">×</span>
                          </button>
                      `;
                  })
                  .join("")
            : '<span class="mutedCopy">No categories selected.</span>';

        if (!categories.length) {
            categoryFilterList.innerHTML = '<span class="mutedCopy">No categories in the current view.</span>';
            return;
        }

        categoryFilterList.innerHTML = categories
            .map((category) => {
                const checked = selected.has(category.toLowerCase()) ? "checked" : "";
                const safe = escapeHtml(category);
                return `
                    <label class="checkItem">
                        <input type="checkbox" value="${safe}" ${checked}>
                        <span>${safe}</span>
                    </label>
                `;
            })
            .join("");
    }

    function renderFilterControls() {
        parentHeadQuery.value = state.draftFilters.parentHeadQuery || "";
        if (categoryInput) {
            categoryInput.value = "";
        }
        renderCategoryChoices();
    }

    function renderPhysicsControls() {
        chargeInput.value = String(state.physics.charge);
        chargeValue.textContent = String(state.physics.charge);
        distanceInput.value = String(state.physics.linkDistance);
        distanceValue.textContent = String(state.physics.linkDistance);
        collisionInput.value = String(state.physics.collisionRadius);
        collisionValue.textContent = String(state.physics.collisionRadius);
        centerInput.value = String(state.physics.centerForce);
        centerValue.textContent = String(state.physics.centerForce.toFixed(2));
    }

    function renderSelection() {
        const node = state.nodes.find((item) => item.id === state.selectedId) || null;
        if (!node) {
            selectionPill.textContent = "None";
            selectionEmpty.hidden = false;
            selectionContent.hidden = true;
            childList.innerHTML = "";
            parentSelect.innerHTML = "";
            return;
        }

        selectionPill.textContent = `#${node.id}`;
        selectionEmpty.hidden = true;
        selectionContent.hidden = false;

        const parent = state.nodes.find((item) => item.id === node.parentId) || null;
        const children = Array.isArray(node.children) ? node.children : [];
        const categories = Array.isArray(node.categories) ? node.categories : [];
        const parentLabel = parent
            ? `#${parent.id} ${parent.head}`
            : node.parentHead || "none";

        selectionSummary.textContent = [
            `id: ${node.id}`,
            `head: ${node.head || "Untitled"}`,
            `quantity: ${node.quantity}`,
            `parent: ${parentLabel}`,
            `depth: ${node.depth == null ? "n/a" : node.depth}`,
            `categories: ${categories.length ? categories.join(", ") : "none"}`,
        ].join("\n");

        childList.innerHTML = children.length
            ? children
                  .map((child) => `<span class="pill">#${escapeHtml(child.id)} ${escapeHtml(child.head || "Untitled")}</span>`)
                  .join("")
            : '<span class="mutedCopy">No children.</span>';

        const parentCandidates = state.nodes
            .filter((candidate) => candidate.id !== node.id)
            .slice()
            .sort((left, right) =>
                String(left.head || "")
                    .toLowerCase()
                    .localeCompare(String(right.head || "").toLowerCase()),
            );

        parentSelect.innerHTML = [
            `<option value="">No parent</option>`,
            ...parentCandidates.map((candidate) => {
                const selected = candidate.id === node.parentId ? "selected" : "";
                return `<option value="${candidate.id}" ${selected}>#${candidate.id} ${escapeHtml(candidate.head || "Untitled")}</option>`;
            }),
        ].join("");

        disconnectParentButton.disabled = !node.parentId;
    }

    function updateCategoryStateFromControls() {
        const next = [];
        for (const input of categoryFilterList.querySelectorAll("input[type='checkbox']")) {
            if (input.checked) {
                next.push(input.value);
            }
        }
        state.draftFilters.categories = uniqueStrings(next);
        renderFilterPill();
        renderCategoryChoices();
    }

    function addCategoryFromInput() {
        const value = String(categoryInput?.value || "").trim();
        if (!value) {
            return;
        }
        state.draftFilters.categories = uniqueStrings([
            ...state.draftFilters.categories,
            value,
        ]);
        if (categoryInput) {
            categoryInput.value = "";
        }
        renderCategoryChoices();
        renderFilterPill();
    }

    function removeCategory(value) {
        const needle = String(value || "").trim().toLowerCase();
        if (!needle) {
            return;
        }
        state.draftFilters.categories = state.draftFilters.categories.filter(
            (category) => category.toLowerCase() !== needle,
        );
        renderCategoryChoices();
        renderFilterPill();
    }

    function applyPhysicsToUi() {
        chargeValue.textContent = String(state.physics.charge);
        distanceValue.textContent = String(state.physics.linkDistance);
        collisionValue.textContent = String(state.physics.collisionRadius);
        centerValue.textContent = String(state.physics.centerForce.toFixed(2));
    }

    function persistPhysics() {
        bridge?.patchCardState?.({
            relations_physics: { ...state.physics },
        });
    }

    function setPhysics(nextPhysics) {
        state.physics = normalizePhysics(nextPhysics);
        renderPhysicsControls();
        persistPhysics();
        rebuildSimulation();
    }

    function graphUrl() {
        return "/host/widgets/" + encodeURIComponent(instanceId()) + "/stream";
    }

    function actionUrl(action) {
        return "/host/widgets/" + encodeURIComponent(instanceId()) + "/actions/" + encodeURIComponent(action);
    }

    function postAction(action, payload) {
        return fetch(actionUrl(action), {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(payload || {}),
        }).then(async (response) => {
            const text = await response.text();
            if (!response.ok) {
                throw new Error(text || response.statusText);
            }
            try {
                return JSON.parse(text);
            } catch {
                return { ok: true, raw: text };
            }
        });
    }

    function clearStream() {
        if (state.stream) {
            state.stream.close();
            state.stream = null;
        }
        state.streamGeneration += 1;
    }

    function restartStream() {
        clearStream();
        connectStream();
    }

    function parseCell(row, key) {
        const value = row && Object.prototype.hasOwnProperty.call(row, key) ? row[key] : undefined;
        if (value == null) {
            return "";
        }
        const text = String(value).trim();
        return /^null$/i.test(text) ? "" : text;
    }

    function parseOptionalInteger(value) {
        const text = String(value || "").trim();
        if (!text) {
            return null;
        }
        const parsed = Number.parseInt(text, 10);
        return Number.isFinite(parsed) ? parsed : null;
    }

    function parseJsonArray(raw) {
        const text = String(raw || "").trim();
        if (!text) {
            return [];
        }
        try {
            const parsed = JSON.parse(text);
            return Array.isArray(parsed) ? parsed : [];
        } catch {
            return [];
        }
    }

    function parseRow(row) {
        const id = Number(parseCell(row, "id"));
        const quantity = Number(parseCell(row, "quantity"));
        if (!Number.isFinite(id) || !Number.isFinite(quantity)) {
            return null;
        }
        const categories = uniqueStrings([
            ...parseJsonArray(parseCell(row, "categories_json")),
            ...parseJsonArray(parseCell(row, "categories")),
            parseCell(row, "primary_category"),
        ]);
        const declaredChildren = parseJsonArray(parseCell(row, "children_json"))
            .map((child) => ({
                id: Number(child?.id),
                head: String(child?.head || ""),
                quantity: Number(child?.quantity || 0),
                taskType: child?.task_type == null ? null : String(child.task_type),
            }))
            .filter((child) => Number.isFinite(child.id));
        const parsedParentId = parseOptionalInteger(parseCell(row, "parent_id"));
        const parentId = parsedParentId && parsedParentId > 0 ? parsedParentId : null;
        const parentHead = parseCell(row, "parent_head") || null;

        return {
            id,
            quantity,
            head: parseCell(row, "head").trim() || "Untitled",
            body: parseCell(row, "body"),
            taskType: parseCell(row, "task_type") || null,
            categories,
            parentId,
            parentHead,
            declaredChildren,
            children: declaredChildren.slice(),
            childrenCount: parseOptionalInteger(parseCell(row, "children_count")) || declaredChildren.length,
            depth: parseOptionalInteger(parseCell(row, "depth")),
            x: 0,
            y: 0,
            vx: 0,
            vy: 0,
        };
    }

    function collectCategories(rows) {
        const seen = new Set();
        const out = [];
        for (const row of rows) {
            for (const category of row.categories || []) {
                const key = category.toLowerCase();
                if (seen.has(key)) {
                    continue;
                }
                seen.add(key);
                out.push(category);
            }
        }
        return out;
    }

    function mergeChild(parent, child) {
        if (!parent || !child) {
            return;
        }
        if (!Array.isArray(parent.children)) {
            parent.children = [];
        }
        if (parent.children.some((entry) => Number(entry?.id) === child.id)) {
            return;
        }
        parent.children.push({
            id: child.id,
            head: child.head,
            quantity: child.quantity,
            taskType: child.taskType,
        });
    }

    function inferParentIdFromHead(node, nodesById) {
        if (node.parentId || !node.parentHead) {
            return node.parentId;
        }
        const needle = String(node.parentHead || "").trim().toLowerCase();
        if (!needle) {
            return null;
        }

        let matchId = null;
        for (const candidate of nodesById.values()) {
            if (candidate.id === node.id) {
                continue;
            }
            if (String(candidate.head || "").trim().toLowerCase() !== needle) {
                continue;
            }
            if (matchId != null) {
                return null;
            }
            matchId = candidate.id;
        }
        return matchId;
    }

    function resolveDepths(nodes, nodesById) {
        const memo = new Map();
        const visiting = new Set();

        function visit(node) {
            if (!node) {
                return null;
            }
            if (node.depth != null) {
                return node.depth;
            }
            if (memo.has(node.id)) {
                return memo.get(node.id);
            }
            if (visiting.has(node.id)) {
                return null;
            }

            visiting.add(node.id);
            let nextDepth = 0;
            if (node.parentId && nodesById.has(node.parentId)) {
                const parentDepth = visit(nodesById.get(node.parentId));
                nextDepth = parentDepth == null ? null : parentDepth + 1;
            }
            visiting.delete(node.id);
            memo.set(node.id, nextDepth);
            return nextDepth;
        }

        for (const node of nodes) {
            node.depth = visit(node);
        }
    }

    function buildGraphData(snapshot) {
        const rows = Array.isArray(snapshot?.rows) ? snapshot.rows : [];
        const parsedRows = rows.map(parseRow).filter(Boolean);
        const nodesById = new Map(parsedRows.map((row) => [row.id, row]));
        const links = [];

        for (const node of parsedRows) {
            node.children = Array.isArray(node.declaredChildren) ? node.declaredChildren.slice() : [];
            node.parentId = inferParentIdFromHead(node, nodesById);
        }

        for (const node of parsedRows) {
            if (node.parentId && nodesById.has(node.parentId)) {
                const parent = nodesById.get(node.parentId);
                links.push({
                    source: node,
                    target: parent,
                });
                mergeChild(parent, node);
            }
        }

        for (const node of parsedRows) {
            node.children.sort((left, right) =>
                String(left?.head || "")
                    .toLowerCase()
                    .localeCompare(String(right?.head || "").toLowerCase()),
            );
            node.childrenCount = Math.max(node.childrenCount || 0, node.children.length);
        }
        resolveDepths(parsedRows, nodesById);

        return {
            rows: parsedRows,
            nodes: parsedRows,
            links,
            categories: collectCategories(parsedRows),
        };
    }

    function syncSnapshot(snapshot) {
        state.snapshot = snapshot;
        const graph = buildGraphData(snapshot);
        state.rows = graph.rows;
        state.nodes = graph.nodes;
        state.links = graph.links;
        state.categories = graph.categories;
        state.availableCategories = graph.categories;
        if (!state.nodes.some((node) => node.id === state.selectedId)) {
            state.selectedId = state.nodes.length ? state.nodes[0].id : null;
        }
        renderCounters();
        renderCategoryChoices();
        renderSelection();
        updateOriginFromSnapshot(snapshot);
        updateStatusFromSnapshot();
        rebuildSimulation();
        updateEmptyState();
    }

    function updateOriginFromSnapshot(snapshot) {
        const viewId = snapshot?.view_id ?? snapshot?.viewId ?? state.origin.viewId;
        const name = snapshot?.name ? String(snapshot.name) : "Relations";
        const query = snapshot?.query ? String(snapshot.query) : "";
        originText.textContent = [
            `serverId: ${state.origin.serverId || "local"}`,
            `viewId: ${viewId == null ? "none" : viewId}`,
            `view: ${name}`,
            `query: ${query || "(none)"}`,
        ].join("\n");
    }

    function updateStatusFromSnapshot() {
        if (!state.error) {
            renderStatus("Live", "live", `${state.rows.length} nodes`);
        }
    }

    function updateEmptyState() {
        emptyState.hidden = state.rows.length > 0;
    }

    function rebuildSimulation() {
        if (!window.d3) {
            renderStatus("Missing D3", "error", "Vendored D3 script did not load.");
            return;
        }
        if (!state.rows.length) {
            if (state.simulation) {
                state.simulation.stop();
                state.simulation = null;
            }
            drawGraph();
            return;
        }

        const width = Math.max(1, canvas.clientWidth);
        const height = Math.max(1, canvas.clientHeight);
        state.width = width;
        state.height = height;
        canvas.width = Math.round(width * window.devicePixelRatio);
        canvas.height = Math.round(height * window.devicePixelRatio);
        ctx.setTransform(window.devicePixelRatio, 0, 0, window.devicePixelRatio, 0, 0);

        const nodes = state.nodes.map((node) => ({ ...node }));
        const nodesById = new Map(nodes.map((node) => [node.id, node]));
        const links = state.links
            .map((link) => {
                const source = nodesById.get(link.source.id || link.source);
                const target = nodesById.get(link.target.id || link.target);
                if (!source || !target) {
                    return null;
                }
                return { source, target };
            })
            .filter(Boolean);

        if (state.simulation) {
            state.simulation.stop();
        }

        state.simulation = window.d3
            .forceSimulation(nodes)
            .force(
                "link",
                window.d3
                    .forceLink(links)
                    .id((d) => d.id)
                    .distance(state.physics.linkDistance)
                    .strength(0.9),
            )
            .force("charge", window.d3.forceManyBody().strength(state.physics.charge))
            .force("collide", window.d3.forceCollide(state.physics.collisionRadius))
            .force("x", window.d3.forceX(width / 2).strength(state.physics.centerForce))
            .force("y", window.d3.forceY(height / 2).strength(state.physics.centerForce))
            .alpha(1)
            .alphaDecay(0.06)
            .on("tick", drawGraph);

        state.simulation.on("tick", drawGraph);
        state.simulation.alphaTarget(0.12).restart();
    }

    function resizeCanvas() {
        const rect = canvas.getBoundingClientRect();
        if (!rect.width || !rect.height) {
            return;
        }
        const dpr = window.devicePixelRatio || 1;
        canvas.width = Math.round(rect.width * dpr);
        canvas.height = Math.round(rect.height * dpr);
        ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
        state.width = rect.width;
        state.height = rect.height;
        if (state.simulation) {
            state.simulation
                .force("x", window.d3.forceX(rect.width / 2).strength(state.physics.centerForce))
                .force("y", window.d3.forceY(rect.height / 2).strength(state.physics.centerForce))
                .alpha(0.3)
                .restart();
        } else {
            drawGraph();
        }
    }

    function drawArrow(from, to, color) {
        const angle = Math.atan2(to.y - from.y, to.x - from.x);
        const size = 7;
        ctx.save();
        ctx.translate(to.x, to.y);
        ctx.rotate(angle);
        ctx.beginPath();
        ctx.moveTo(-size, -size * 0.45);
        ctx.lineTo(0, 0);
        ctx.lineTo(-size, size * 0.45);
        ctx.strokeStyle = color;
        ctx.lineWidth = 2;
        ctx.stroke();
        ctx.restore();
    }

    function drawDirectedLink(link, highlight) {
        const source = link.source;
        const target = link.target;
        if (!source || !target) {
            return;
        }
        const sourceRadius = nodeRadius(source) + 2;
        const targetRadius = nodeRadius(target) + 4;
        const dx = target.x - source.x;
        const dy = target.y - source.y;
        const distance = Math.hypot(dx, dy);
        if (!distance) {
            return;
        }
        const ux = dx / distance;
        const uy = dy / distance;
        const startX = source.x + ux * sourceRadius;
        const startY = source.y + uy * sourceRadius;
        const endX = target.x - ux * targetRadius;
        const endY = target.y - uy * targetRadius;
        const color = highlight ? "rgba(120, 215, 255, 0.56)" : "rgba(120, 215, 255, 0.34)";

        ctx.beginPath();
        ctx.moveTo(startX, startY);
        ctx.lineTo(endX, endY);
        ctx.strokeStyle = color;
        ctx.lineWidth = highlight ? 2.6 : 1.8;
        ctx.stroke();
        drawArrow(
            { x: startX, y: startY },
            { x: endX, y: endY },
            color,
        );
    }

    function nodeRadius(node) {
        const categories = Array.isArray(node.categories) ? node.categories.length : 0;
        return 11 + Math.min(8, categories * 1.5);
    }

    function nodeFill(node) {
        if (node.id === state.selectedId) {
            return "#d8f5e8";
        }
        if (node.parentId) {
            return "#12212a";
        }
        return "#182733";
    }

    function nodeStroke(node) {
        if (node.id === state.selectedId) {
            return "#78d7ff";
        }
        if (node.quantity > 0) {
            return "#7ef0c6";
        }
        if (node.quantity < 0) {
            return "#f2bb78";
        }
        return "rgba(120, 215, 255, 0.4)";
    }

    function drawNodeLabel(node, radius) {
        const label = String(node.head || "Untitled");
        const maxLength = Math.max(8, Math.floor((radius * 2.4) / 7));
        const text = label.length > maxLength ? label.slice(0, maxLength - 1) + "…" : label;
        ctx.fillStyle = "#e6edf3";
        ctx.font = "600 12px " + getComputedStyle(document.body).fontFamily;
        ctx.textBaseline = "middle";
        ctx.fillText(text, node.x + radius + 8, node.y + 1);
    }

    function drawGraph() {
        const width = canvas.clientWidth;
        const height = canvas.clientHeight;
        if (!width || !height) {
            return;
        }

        ctx.clearRect(0, 0, width, height);
        ctx.save();

        const simulation = state.simulation;
        const nodes = simulation ? simulation.nodes() : state.nodes;
        const links = simulation ? simulation.force("link")?.links() || [] : state.links;

        ctx.lineWidth = 1.4;
        for (const link of links) {
            const source = link.source;
            const target = link.target;
            if (!source || !target) {
                continue;
            }
            const highlight =
                source.id === state.selectedId || target.id === state.selectedId;
            drawDirectedLink(link, highlight);
        }

        for (const node of nodes) {
            const radius = nodeRadius(node);
            ctx.beginPath();
            ctx.arc(node.x, node.y, radius, 0, Math.PI * 2);
            ctx.fillStyle = nodeFill(node);
            ctx.fill();
            ctx.lineWidth = node.id === state.selectedId ? 2.4 : 1.3;
            ctx.strokeStyle = nodeStroke(node);
            ctx.stroke();

            if (node.id === state.selectedId) {
                ctx.beginPath();
                ctx.arc(node.x, node.y, radius + 5, 0, Math.PI * 2);
                ctx.strokeStyle = "rgba(120, 215, 255, 0.18)";
                ctx.lineWidth = 2;
                ctx.stroke();
            }
            drawNodeLabel(node, radius);
        }

        ctx.restore();
    }

    function findNodeAtPoint(x, y) {
        if (state.simulation && typeof state.simulation.find === "function") {
            return state.simulation.find(x, y, 20) || null;
        }
        for (const node of state.nodes) {
            const dx = node.x - x;
            const dy = node.y - y;
            if (Math.hypot(dx, dy) <= nodeRadius(node) + 4) {
                return node;
            }
        }
        return null;
    }

    function selectNode(node) {
        if (!node) {
            state.selectedId = null;
            state.selectedParentId = null;
            renderSelection();
            drawGraph();
            return;
        }

        state.selectedId = node.id;
        state.selectedParentId = node.parentId || null;
        renderSelection();
        drawGraph();
    }

    function connectParent() {
        const node = state.nodes.find((item) => item.id === state.selectedId) || null;
        if (!node) {
            return;
        }
        const nextParentId = String(parentSelect.value || "").trim();
        const parentId = nextParentId ? Number(nextParentId) : null;
        if (parentId === node.id) {
            renderStatus("Invalid parent", "error", "A node cannot parent itself.");
            return;
        }
        postAction("set-parent", {
            recordId: node.id,
            parentId: parentId || null,
        })
            .then(() => {
                renderStatus("Saved", "live", "Parent relation updated.");
                restartStream();
            })
            .catch((error) => {
                renderStatus("Relation error", "error", error instanceof Error ? error.message : String(error));
            });
    }

    function disconnectParent() {
        const node = state.nodes.find((item) => item.id === state.selectedId) || null;
        if (!node) {
            return;
        }
        postAction("set-parent", {
            recordId: node.id,
            parentId: null,
        })
            .then(() => {
                renderStatus("Saved", "live", "Parent relation removed.");
                restartStream();
            })
            .catch((error) => {
                renderStatus("Relation error", "error", error instanceof Error ? error.message : String(error));
            });
    }

    function applyFilters() {
        updateCategoryStateFromControls();
        state.draftFilters.parentHeadQuery = String(parentHeadQuery.value || "").trim();
        const rows = buildFilterRows();
        postAction("apply-filters", { filters: rows })
            .then((outcome) => {
                const filtersVersion = Number(outcome?.detail?.filters_version || 0);
                bridge?.patchCardState?.({
                    filters: rows,
                    filters_version: filtersVersion,
                });
                state.cardState.filters = rows;
                state.cardState.filters_version = filtersVersion;
                renderFilterPill();
                renderStatus("Saved", "live", "View filters updated.");
                restartStream();
            })
            .catch((error) => {
                renderStatus("Filter error", "error", error instanceof Error ? error.message : String(error));
            });
    }

    function clearFilters() {
        state.draftFilters = { categories: [], parentHeadQuery: "" };
        renderFilterControls();
        applyFilters();
    }

    function resetPhysics() {
        setPhysics({ ...DEFAULT_PHYSICS });
    }

    function handleSnapshotEvent(data) {
        try {
            const snapshot = JSON.parse(data);
            syncSnapshot(snapshot);
            renderStatus("Live", "live", `${state.rows.length} nodes`);
        } catch (error) {
            renderStatus("Snapshot error", "error", error instanceof Error ? error.message : String(error));
        }
    }

    function connectStream() {
        if (!state.origin.serverId) {
            renderStatus("Missing origin", "error", "The widget needs a server binding.");
            return;
        }

        clearStream();
        renderStatus("Connecting", "status", "Opening the widget stream...");
        const source = new EventSource(graphUrl());
        state.stream = source;

        source.addEventListener("snapshot", (event) => {
            handleSnapshotEvent(event.data);
        });

        source.addEventListener("kanban-error", (event) => {
            try {
                const payload = JSON.parse(event.data);
                renderStatus("Stream error", "error", payload?.message || "The host rejected the stream payload.");
            } catch (error) {
                renderStatus("Stream error", "error", error instanceof Error ? error.message : String(error));
            }
        });

        source.addEventListener("error", () => {
            renderStatus("Stream error", "error", "The widget stream disconnected.");
        });
    }

    function bindEvents() {
        categoryFilterList.addEventListener("change", updateCategoryStateFromControls);
        categoryAddButton?.addEventListener("click", addCategoryFromInput);
        categoryInput?.addEventListener("keydown", (event) => {
            if (event.key === "Enter") {
                event.preventDefault();
                addCategoryFromInput();
            }
        });
        selectedCategoryList.addEventListener("click", (event) => {
            const button = event.target?.closest?.("[data-category-remove]");
            if (!button) {
                return;
            }
            removeCategory(button.getAttribute("data-category-remove"));
        });
        parentHeadQuery.addEventListener("input", () => {
            state.draftFilters.parentHeadQuery = String(parentHeadQuery.value || "").trim();
            renderFilterPill();
        });
        applyFiltersButton.addEventListener("click", applyFilters);
        clearFiltersButton.addEventListener("click", clearFilters);
        resetPhysicsButton.addEventListener("click", resetPhysics);
        connectParentButton.addEventListener("click", connectParent);
        disconnectParentButton.addEventListener("click", disconnectParent);

        for (const [input, key] of [
            [chargeInput, "charge"],
            [distanceInput, "linkDistance"],
            [collisionInput, "collisionRadius"],
            [centerInput, "centerForce"],
        ]) {
            input.addEventListener("input", () => {
                state.physics[key] = Number(input.value);
                applyPhysicsToUi();
            });
            input.addEventListener("change", () => {
                state.physics[key] = Number(input.value);
                persistPhysics();
                rebuildSimulation();
            });
        }

        canvas.addEventListener("click", (event) => {
            const rect = canvas.getBoundingClientRect();
            const node = findNodeAtPoint(event.clientX - rect.left, event.clientY - rect.top);
            if (node) {
                selectNode(node);
            }
        });

        canvas.addEventListener("pointerdown", (event) => {
            if (!state.simulation) {
                return;
            }
            const rect = canvas.getBoundingClientRect();
            const node = findNodeAtPoint(event.clientX - rect.left, event.clientY - rect.top);
            if (!node) {
                return;
            }
            state.draggingNode = node;
            state.dragStart = { x: event.clientX, y: event.clientY };
            node.fx = node.x;
            node.fy = node.y;
            state.simulation.alphaTarget(0.2).restart();
        });

        canvas.addEventListener("pointermove", (event) => {
            if (!state.draggingNode) {
                return;
            }
            const rect = canvas.getBoundingClientRect();
            state.draggingNode.fx = event.clientX - rect.left;
            state.draggingNode.fy = event.clientY - rect.top;
            drawGraph();
        });

        canvas.addEventListener("pointerup", () => {
            if (!state.draggingNode) {
                return;
            }
            state.draggingNode.fx = null;
            state.draggingNode.fy = null;
            state.draggingNode = null;
            if (state.simulation) {
                state.simulation.alphaTarget(0);
            }
        });

        window.addEventListener("resize", resizeCanvas);
    }

    function start() {
        renderMode();
        renderStatus("Booting", "status", "Waiting for the widget stream.");
        bindEvents();
        syncFromBridge();
        bridge?.subscribe?.(() => syncFromBridge());
        bridge?.requestState?.();
        renderPhysicsControls();
        renderFilterControls();
        renderSelection();
        connectStream();
        state.resizeObserver = new ResizeObserver(() => {
            resizeCanvas();
        });
        state.resizeObserver.observe(canvas);
        resizeCanvas();
    }

    start();
    window.RelationsWidget = {
        state,
        restartStream,
        applyFilters,
        connectParent,
        disconnectParent,
    };
})();
"##.to_string()
}
