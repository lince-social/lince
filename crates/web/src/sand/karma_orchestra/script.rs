pub(crate) fn script() -> String {
    let mut script = String::from(include_str!("logic.js"));
    script.push_str(
        r####"
(() => {
    const d3 = window.d3;
    const Logic = globalThis.KarmaOrchestraLogic;
    const frame = window.frameElement;
    const DEFAULT_PALETTE = {
        condition: "#f1ece2",
        consequence: "#6f2e2b",
        inactive: "#9b9b9b",
    };
    const DEFAULT_PHYSICS = {
        centerExpulsion: 2.3,
        linkDistance: 240,
        nodeRepulsion: 520,
    };
    const LABEL_ROW_PADDING_X = 2;
    const LABEL_ROW_PADDING_Y = 1;
    const HUMAN_LABEL_FONT = "700 8px ui-sans-serif, system-ui";
    const META_LABEL_FONT = "6.5px ui-sans-serif, system-ui";
    const state = {
        contract: null,
        graph: null,
        layoutMode: "list",
        distinctCondition: false,
        distinctConsequence: false,
        palette: { ...DEFAULT_PALETTE },
        physics: { ...DEFAULT_PHYSICS },
        width: 0,
        height: 0,
        simulation: null,
        layout: { centerX: 0, centerY: 0, ringRadius: 0 },
        dragBehavior: null,
        draggingCondition: false,
    };

    const el = {
        svg: document.getElementById("karma-graph"),
        status: document.getElementById("karma-status"),
        viewPill: document.getElementById("karma-view-pill"),
        countPill: document.getElementById("karma-count-pill"),
        loopPill: document.getElementById("karma-loop-pill"),
        empty: document.getElementById("karma-empty"),
        viewButton: document.getElementById("karma-view-button"),
        viewModal: document.getElementById("karma-view-modal"),
        viewClose: document.getElementById("karma-view-close"),
        viewList: document.getElementById("karma-view-list"),
        viewName: document.getElementById("karma-view-name"),
        createView: document.getElementById("karma-create-view"),
        stateBall: document.getElementById("karma-state-ball"),
        adjustments: document.getElementById("karma-adjustments"),
        adjustClose: document.getElementById("karma-adjust-close"),
        physicsReset: document.getElementById("karma-physics-reset"),
        physicsCenterExpulsionField: document.getElementById("karma-physics-center-expulsion-field"),
        physicsCenterExpulsion: document.getElementById("karma-physics-center-expulsion"),
        physicsCenterExpulsionValue: document.getElementById("karma-physics-center-expulsion-value"),
        physicsLinkDistanceInput: document.getElementById("karma-physics-link-distance-input"),
        physicsNodeRepulsionInput: document.getElementById("karma-physics-node-repulsion-input"),
        physicsLinkDistance: document.getElementById("karma-physics-link-distance"),
        physicsLinkDistanceValue: document.getElementById("karma-physics-link-distance-value"),
        physicsNodeRepulsion: document.getElementById("karma-physics-node-repulsion"),
        distinctCondition: document.getElementById("karma-distinct-condition"),
        distinctConsequence: document.getElementById("karma-distinct-consequence"),
        conditionColor: document.getElementById("karma-condition-color"),
        consequenceColor: document.getElementById("karma-consequence-color"),
        inactiveColor: document.getElementById("karma-inactive-color"),
        summaryRules: document.getElementById("karma-summary-rules"),
        summaryConditions: document.getElementById("karma-summary-conditions"),
        summaryConsequences: document.getElementById("karma-summary-consequences"),
        summaryLoops: document.getElementById("karma-summary-loops"),
    };

    const svg = d3.select(el.svg);
    const root = svg.append("g");
    const linkLayer = root.append("g");
    const nodeLayer = root.append("g");
    const arrowLayer = root.append("g");
    const labelLayer = root.append("g");
    const labelMeasureCanvas = document.createElement("canvas");
    const labelMeasureContext = labelMeasureCanvas.getContext("2d");
    const zoom = d3.zoom().scaleExtent([0.25, 3]).on("zoom", (event) => {
        root.attr("transform", event.transform);
    });
    svg.call(zoom);

    function instanceId() {
        return String(frame?.dataset?.packageInstanceId || "preview").trim() || "preview";
    }

    function actionUrl(action) {
        return "/host/widgets/" + encodeURIComponent(instanceId()) + "/actions/" + encodeURIComponent(action);
    }

    async function postAction(action, payload = {}) {
        const response = await fetch(actionUrl(action), {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(payload),
        });
        const data = await response.json().catch(() => ({}));
        if (!response.ok) {
            throw new Error(data?.message || data?.error || "Action failed");
        }
        return data;
    }

    async function loadContract() {
        const response = await fetch("/host/widgets/" + encodeURIComponent(instanceId()) + "/contract");
        const data = await response.json();
        if (!response.ok) throw new Error(data?.message || "Contract failed");
        state.contract = data;
        loadDistinctness(data?.state?.distinctness || "none");
        state.layoutMode = loadLayoutMode();
        state.palette = loadPalette();
        state.physics = loadPhysics();
        setLayoutModeButtons();
        syncLayoutModePhysicsVisibility();
        syncDistinctnessInputs();
        syncPaletteInputs();
        syncPhysicsInputs();
        updateBinding(data.binding);
        if (data.binding?.viewId) {
            await loadGraph();
        } else {
            setStatus("Pick view");
        }
    }

    function setStatus(text) {
        el.status.textContent = text;
    }

    function paletteStorageKey() {
        return "karma-orchestra:palette";
    }

    function loadPalette() {
        try {
            const raw = window.localStorage.getItem(paletteStorageKey());
            if (!raw) return { ...DEFAULT_PALETTE };
            const parsed = JSON.parse(raw);
            return {
                condition: parsed?.condition || DEFAULT_PALETTE.condition,
                consequence: parsed?.consequence || DEFAULT_PALETTE.consequence,
                inactive: parsed?.inactive || DEFAULT_PALETTE.inactive,
            };
        } catch (_error) {
            return { ...DEFAULT_PALETTE };
        }
    }

    function savePalette() {
        try {
            window.localStorage.setItem(paletteStorageKey(), JSON.stringify(state.palette));
        } catch (_error) {}
    }

    function syncPaletteInputs() {
        if (el.conditionColor) el.conditionColor.value = state.palette.condition;
        if (el.consequenceColor) el.consequenceColor.value = state.palette.consequence;
        if (el.inactiveColor) el.inactiveColor.value = state.palette.inactive;
    }

    function setPaletteColor(key, value) {
        state.palette[key] = value;
        savePalette();
        render();
    }

    function layoutModeStorageKey() {
        return "karma-orchestra:layout-mode";
    }

    function loadLayoutMode() {
        try {
            const value = window.localStorage.getItem(layoutModeStorageKey());
            return value === "circle" ? "circle" : "list";
        } catch (_error) {
            return "list";
        }
    }

    function saveLayoutMode() {
        try {
            window.localStorage.setItem(layoutModeStorageKey(), state.layoutMode);
        } catch (_error) {}
    }

    function syncLayoutModePhysicsVisibility() {
        if (el.physicsCenterExpulsionField) {
            el.physicsCenterExpulsionField.hidden = state.layoutMode === "list";
        }
    }

    function loadDistinctness(value) {
        state.distinctCondition = value === "condition" || value === "both";
        state.distinctConsequence = value === "consequence" || value === "both";
    }

    function distinctnessValue() {
        if (state.distinctCondition && state.distinctConsequence) return "both";
        if (state.distinctCondition) return "condition";
        if (state.distinctConsequence) return "consequence";
        return "none";
    }

    function syncDistinctnessInputs() {
        if (el.distinctCondition) el.distinctCondition.checked = state.distinctCondition;
        if (el.distinctConsequence) el.distinctConsequence.checked = state.distinctConsequence;
    }

    function physicsStorageKey() {
        return "karma-orchestra:physics";
    }

    function loadPhysics() {
        try {
            const raw = window.localStorage.getItem(physicsStorageKey());
            if (!raw) return { ...DEFAULT_PHYSICS };
            const parsed = JSON.parse(raw);
            const centerExpulsion = Number(parsed?.centerExpulsion);
            const linkDistance = Number(parsed?.linkDistance ?? parsed?.conditionPulling);
            const nodeRepulsion = Math.abs(Number(parsed?.nodeRepulsion));
            return {
                centerExpulsion: Number.isFinite(centerExpulsion) ? centerExpulsion : DEFAULT_PHYSICS.centerExpulsion,
                linkDistance: Number.isFinite(linkDistance) ? linkDistance : DEFAULT_PHYSICS.linkDistance,
                nodeRepulsion: Number.isFinite(nodeRepulsion) ? nodeRepulsion : DEFAULT_PHYSICS.nodeRepulsion,
            };
        } catch (_error) {
            return { ...DEFAULT_PHYSICS };
        }
    }

    function savePhysics() {
        try {
            window.localStorage.setItem(physicsStorageKey(), JSON.stringify(state.physics));
        } catch (_error) {}
    }

    function syncPhysicsInputs() {
        if (el.physicsCenterExpulsion) el.physicsCenterExpulsion.value = String(state.physics.centerExpulsion);
        if (el.physicsCenterExpulsionValue) el.physicsCenterExpulsionValue.textContent = String(state.physics.centerExpulsion);
        if (el.physicsLinkDistanceInput) el.physicsLinkDistanceInput.value = String(state.physics.linkDistance);
        if (el.physicsLinkDistance) el.physicsLinkDistance.value = String(state.physics.linkDistance);
        if (el.physicsLinkDistanceValue) el.physicsLinkDistanceValue.textContent = String(state.physics.linkDistance);
        if (el.physicsNodeRepulsionInput) el.physicsNodeRepulsionInput.value = String(Math.abs(state.physics.nodeRepulsion));
        if (el.physicsNodeRepulsion) el.physicsNodeRepulsion.value = String(state.physics.nodeRepulsion);
    }

    function setPhysicsField(key, value) {
        state.physics[key] = key === "nodeRepulsion" ? Math.max(0, Math.abs(value)) : value;
        savePhysics();
        syncPhysicsInputs();
        if (state.layoutMode === "list") {
            render();
            return;
        }
        applyPhysicsToSimulation();
    }

    function resetPhysics() {
        state.physics = { ...DEFAULT_PHYSICS };
        savePhysics();
        syncPhysicsInputs();
        if (state.layoutMode === "list") {
            render();
            return;
        }
        applyPhysicsToSimulation();
    }

    function updateBinding(binding) {
        el.viewPill.textContent = binding?.viewName ? binding.viewName : "No view";
    }

    async function openViewModal() {
        el.viewModal.hidden = false;
        el.viewList.innerHTML = "<div class='muted'>Scanning Views...</div>";
        try {
            const data = await postAction("list-views");
            const views = data.views || [];
            if (!views.length) {
                el.viewList.innerHTML = "<div class='muted'>No Karma Orchestra Views found.</div>";
                return;
            }
            el.viewList.innerHTML = "";
            for (const view of views) {
                const button = document.createElement("button");
                button.type = "button";
                button.className = "button viewRow";
                button.textContent = "#" + view.id + "  " + view.name;
                button.addEventListener("click", async () => {
                    const data = await postAction("use-view", { viewId: view.id });
                    updateBinding(data.binding);
                    el.viewModal.hidden = true;
                    await loadGraph();
                });
                el.viewList.appendChild(button);
            }
        } catch (error) {
            el.viewList.innerHTML = "<div class='muted'>" + escapeHtml(error.message) + "</div>";
        }
    }

    async function createView() {
        const name = el.viewName.value.trim() || "Karma Orchestra";
        const data = await postAction("create-view", { name });
        updateBinding(data.binding);
        el.viewModal.hidden = true;
        await loadGraph();
    }

    async function loadGraph() {
        setStatus("Loading");
        const data = await postAction("load-graph");
        state.graph = data.graph;
        setStatus("Ready");
        render();
    }

    function renderedGraph() {
        const graph = state.graph || { nodes: [], links: [], loops: [], karmaRows: [] };
        const baseNodes = graph.nodes || [];
        const baseById = new Map(baseNodes.map((node) => [node.id, node]));
        const nodes = [];
        const seenNodes = new Set();
        for (const node of baseNodes) {
            if (state.distinctCondition || node.kind !== "condition") {
                if (state.distinctConsequence || node.kind !== "consequence") {
                    nodes.push(node);
                    seenNodes.add(node.id);
                }
            }
        }
        let links = addVisualDirectLinks(graph, graph.links || []);
        const inactiveRuleIds = new Set((graph.karmaRows || []).filter(ruleHasZeroQuantity).map((rule) => Number(rule.karmaId)));
        links = links.map((link) => expandLinkDistinctness(link, graph, baseById, nodes, seenNodes));
        links = links.map((link) => ({
            ...link,
            active: !(link.ruleIds || []).some((id) => inactiveRuleIds.has(Number(id))),
        }));
        return { ...graph, nodes: Logic.uniqueBy(nodes, (node) => node.id), links };
    }

    function expandLinkDistinctness(link, graph, baseById, nodes, seenNodes) {
        const next = { ...link };
        const rule = firstRuleForLink(link, graph);
        if (!rule) return next;
        if (!state.distinctCondition && next.source.startsWith("condition:")) {
            next.source = rowNodeId("condition", rule.karmaId);
            addRowNode(next.source, "condition:" + rule.conditionId, rule.karmaId, baseById, nodes, seenNodes);
        }
        if (!state.distinctCondition && next.target.startsWith("condition:")) {
            next.target = rowNodeId("condition", rule.karmaId);
            addRowNode(next.target, "condition:" + rule.conditionId, rule.karmaId, baseById, nodes, seenNodes);
        }
        if (!state.distinctConsequence && next.source.startsWith("consequence:")) {
            next.source = rowNodeId("consequence", rule.karmaId);
            addRowNode(next.source, "consequence:" + rule.consequenceId, rule.karmaId, baseById, nodes, seenNodes);
        }
        if (!state.distinctConsequence && next.target.startsWith("consequence:")) {
            next.target = rowNodeId("consequence", rule.karmaId);
            addRowNode(next.target, "consequence:" + rule.consequenceId, rule.karmaId, baseById, nodes, seenNodes);
        }
        return next;
    }

    function firstRuleForLink(link, graph) {
        const ruleId = Number((link.ruleIds || [])[0]);
        if (Number.isFinite(ruleId)) {
            const rule = (graph.karmaRows || []).find((row) => Number(row.karmaId) === ruleId);
            if (rule) return rule;
        }
        const conditionId = Number(String(link.source).split(":")[1]);
        const consequenceId = Number(String(link.target).split(":")[1]);
        return (graph.karmaRows || []).find((row) => Number(row.conditionId) === conditionId && Number(row.consequenceId) === consequenceId) || null;
    }

    function rowNodeId(kind, ruleId) {
        return kind + "-row:" + ruleId;
    }

    function addRowNode(id, baseId, ruleId, baseById, nodes, seenNodes) {
        if (seenNodes.has(id)) return;
        const base = baseById.get(baseId);
        if (!base) return;
        nodes.push({ ...base, id, ruleIds: [ruleId] });
        seenNodes.add(id);
    }

    function addVisualDirectLinks(graph, links) {
        const out = [...links];
        const seenRules = new Set(out.flatMap((link) => link.kind === "direct" ? (link.ruleIds || []) : []));
        for (const rule of graph.karmaRows || []) {
            if (seenRules.has(rule.karmaId)) continue;
            out.push({
                id: "visual-direct:" + rule.karmaId,
                source: "condition:" + rule.conditionId,
                target: "consequence:" + rule.consequenceId,
                kind: "direct",
                active: !ruleHasZeroQuantity(rule),
                ruleIds: [rule.karmaId],
                potentiallyUnreachable: false,
            });
        }
        return out;
    }

    function render() {
        const graph = renderedGraph();
        const nodes = graph.nodes || [];
        const links = (graph.links || []).map((link) => ({ ...link }));
        const width = el.svg.clientWidth || 800;
        const height = el.svg.clientHeight || 600;
        state.width = width;
        state.height = height;
        el.empty.hidden = nodes.length > 0;
        el.countPill.textContent = (graph.karmaRows?.length || 0) + " rules";
        el.loopPill.textContent = (graph.loops?.length || 0) + " loops";
        el.summaryRules.textContent = graph.karmaRows?.length || 0;
        el.summaryConditions.textContent = nodes.filter((node) => node.kind === "condition").length;
        el.summaryConsequences.textContent = nodes.filter((node) => node.kind !== "condition").length;
        el.summaryLoops.textContent = graph.loops?.length || 0;

        const firstConditionOrder = new Map();
        for (const rule of graph.karmaRows || []) {
            if (!firstConditionOrder.has(Number(rule.conditionId))) {
                firstConditionOrder.set(Number(rule.conditionId), Number(rule.karmaId));
            }
        }
        const conditionNodes = nodes
            .filter((node) => node.kind === "condition")
            .sort((a, b) => conditionOrder(a, firstConditionOrder) - conditionOrder(b, firstConditionOrder));
        const otherNodes = nodes.filter((node) => node.kind !== "condition");
        const radius = ringRadius(conditionNodes.length, width, height);
        state.layout = {
            centerX: width / 2,
            centerY: height / 2,
            ringRadius: radius,
        };
        const byId = new Map();
        conditionNodes.forEach((node, index) => {
            const angle = -Math.PI / 2 + (index / Math.max(1, conditionNodes.length)) * Math.PI * 2;
            node.angle = angle;
            node.color = nodeHasActiveQuantity(node, graph) ? state.palette.condition : state.palette.inactive;
            byId.set(node.id, node);
        });
        const bySource = new Map();
        for (const link of links) {
            if (!bySource.has(link.source)) bySource.set(link.source, []);
            bySource.get(link.source).push(link);
        }
        otherNodes.forEach((node) => {
            node.color = nodeHasActiveQuantity(node, graph) ? state.palette.consequence : state.palette.inactive;
            byId.set(node.id, node);
        });

        if (state.layoutMode === "list") {
            layoutList(conditionNodes, otherNodes, links, byId, width, height);
            drawFrame(nodes, links, graph.loops || []);
        } else {
            conditionNodes.forEach(lockConditionNode);
            otherNodes.forEach((node, index) => {
                node.fx = null;
                node.fy = null;
                const incoming = links.find((link) => link.target === node.id);
                const source = incoming ? byId.get(incoming.source) : null;
                const angle = source?.angle ?? (-Math.PI / 2 + index);
                const fanIndex = source ? (bySource.get(incoming.source) || []).findIndex((link) => link.target === node.id) : 0;
                const spread = (fanIndex - 1) * 0.22;
                node.x = state.layout.centerX + Math.cos(angle + spread) * (radius + 150);
                node.y = state.layout.centerY + Math.sin(angle + spread) * (radius + 150);
            });
            runPhysics(nodes, links, byId, graph.loops || []);
        }
    }

    function layoutList(conditionNodes, otherNodes, links, byId, width, height) {
        if (state.simulation) state.simulation.stop();
        const leftX = Math.max(90, width * 0.18);
        const linkDistance = Math.max(80, Number(state.physics.linkDistance) || DEFAULT_PHYSICS.linkDistance);
        const rightX = Math.min(width - 90, leftX + linkDistance);
        state.layout.listConditionRightX = leftX;
        state.layout.listConsequenceLeftX = rightX;
        const top = 72;
        const bottom = 72;
        const verticalSpacing = Math.max(24, Math.abs(Number(state.physics.nodeRepulsion) || DEFAULT_PHYSICS.nodeRepulsion) / 3);
        const groupGap = childGap;
        const childGap = verticalSpacing;
        const groups = conditionNodes.map((condition) => {
            const outgoing = links
                .filter((link) => link.source === condition.id && byId.has(link.target))
                .sort((a, b) => String(a.target).localeCompare(String(b.target)));
            const count = Math.max(1, outgoing.length);
            const span = (count - 1) * childGap;
            return { condition, outgoing, count, span, blockHeight: Math.max(56, span + 28) };
        });
        const placed = new Set();
        let y = top;
        for (const group of groups) {
            const condition = group.condition;
            const outgoing = group.outgoing;
            const count = group.count;
            const span = group.span;
            const blockHeight = group.blockHeight;
            const centerY = y + blockHeight / 2;
            condition.angle = 0;
            condition.x = leftX;
            condition.y = centerY;
            condition.fx = leftX;
            condition.fy = centerY;
            outgoing.forEach((link, index) => {
                const target = byId.get(link.target);
                if (!target) return;
                target.x = rightX;
                target.y = centerY - span / 2 + index * (count > 1 ? span / (count - 1) : 0);
                target.fx = rightX;
                target.fy = target.y;
                placed.add(target.id);
            });
            y += blockHeight + groupGap;
        }
        let orphanY = Math.max(top, y);
        for (const node of otherNodes) {
            if (placed.has(node.id)) continue;
            node.x = rightX;
            node.y = orphanY;
            node.fx = rightX;
            node.fy = orphanY;
            orphanY += childGap;
        }
    }

    function conditionOrder(node, firstConditionOrder) {
        const ownRule = Number((node.ruleIds || [])[0]);
        if (Number.isFinite(ownRule) && node.id.startsWith("condition-row:")) return ownRule;
        return firstConditionOrder.get(Number(node.entityId)) ?? Number(node.entityId) ?? 0;
    }

    function ringRadius(count, width, height) {
        const slotSize = 56;
        const contentRadius = Math.max(1, count) * slotSize / (2 * Math.PI);
        const viewportRadius = Math.min(width, height) * 0.42;
        return Math.max(140, Math.min(viewportRadius, contentRadius));
    }

    function ruleHasZeroQuantity(rule) {
        return Number(rule.karmaQuantity) === 0
            || Number(rule.conditionQuantity) === 0
            || Number(rule.consequenceQuantity) === 0;
    }

    function nodeHasActiveQuantity(node, graph) {
        const ruleIds = new Set((node.ruleIds || []).map((id) => Number(id)));
        return (graph.karmaRows || []).some((rule) => ruleIds.has(Number(rule.karmaId)) && !ruleHasZeroQuantity(rule));
    }

    function runPhysics(nodes, links, byId, loops) {
        if (state.simulation) state.simulation.stop();
        const forceLinks = links
            .filter((link) => byId.has(link.source) && byId.has(link.target))
            .map((link) => ({ ...link }));
        const physics = state.physics || DEFAULT_PHYSICS;
        const repulsion = -Math.abs(physics.nodeRepulsion);
        state.simulation = d3.forceSimulation(nodes)
            .force("link", d3.forceLink(forceLinks).id((node) => node.id).distance((link) => link.kind === "fulfillment" ? Math.max(80, physics.linkDistance * 0.85) : physics.linkDistance).strength((link) => link.kind === "fulfillment" ? 0.9 : 0.65))
            .force("charge", d3.forceManyBody().strength((node) => node.kind === "condition" ? -12 : repulsion))
            .force("collide", d3.forceCollide((node) => node.kind === "condition" ? 38 : 98).iterations(3))
            .force("radial", d3.forceRadial((node) => node.kind === "condition" ? state.layout.ringRadius : state.layout.ringRadius + 320, state.layout.centerX, state.layout.centerY).strength((node) => node.kind === "condition" ? 0.95 : physics.centerExpulsion))
            .alpha(0.9)
            .alphaDecay(0.07)
            .on("tick", () => {
                redistributeConditionRing(nodes, draggedCondition(nodes));
                drawFrame(nodes, links, loops);
            });
        redistributeConditionRing(nodes, draggedCondition(nodes));
        drawFrame(nodes, links, loops);
    }

    function applyPhysicsToSimulation() {
        if (!state.simulation || state.layoutMode !== "circle") {
            return;
        }
        const physics = state.physics || DEFAULT_PHYSICS;
        const repulsion = -Math.abs(physics.nodeRepulsion);
        const linkForce = state.simulation.force("link");
        if (linkForce) {
            linkForce
                .distance((link) => link.kind === "fulfillment" ? Math.max(80, physics.linkDistance * 0.85) : physics.linkDistance)
                .strength((link) => link.kind === "fulfillment" ? 0.9 : 0.65);
        }
        state.simulation
            .force("charge", d3.forceManyBody().strength((node) => node.kind === "condition" ? -12 : repulsion))
            .force("radial", d3.forceRadial((node) => node.kind === "condition" ? state.layout.ringRadius : state.layout.ringRadius + 320, state.layout.centerX, state.layout.centerY).strength((node) => node.kind === "condition" ? 0.95 : physics.centerExpulsion))
            .alpha(0.55)
            .restart();
    }

    function lockConditionNode(node) {
        node.fx = state.layout.centerX + Math.cos(node.angle || 0) * state.layout.ringRadius;
        node.fy = state.layout.centerY + Math.sin(node.angle || 0) * state.layout.ringRadius;
        node.x = node.fx;
        node.y = node.fy;
    }

    function draggedCondition(nodes) {
        return nodes.find((node) => node.kind === "condition" && node.dragging) || null;
    }

    function redistributeConditionRing(nodes, anchorNode) {
        const conditions = nodes.filter((node) => node.kind === "condition");
        if (conditions.length < 2) {
            for (const node of conditions) {
                lockConditionNode(node);
            }
            return;
        }
        const spacing = Math.PI * 2 / conditions.length;
        const ordered = conditions
            .map((node) => ({
                node,
                angle: normalizeAngle(node.angle || 0),
            }))
            .sort((a, b) => a.angle - b.angle);
        const anchor = anchorNode ? ordered.findIndex((item) => item.node.id === anchorNode.id) : 0;
        const startAngle = anchorNode ? normalizeAngle(anchorNode.angle || 0) : ordered[0].angle;
        for (let index = 0; index < ordered.length; index += 1) {
            const item = ordered[(anchor + index) % ordered.length];
            item.node.angle = normalizeAngle(startAngle + index * spacing);
            lockConditionNode(item.node);
        }
    }

    function normalizeAngle(angle) {
        let value = angle;
        while (value <= -Math.PI) value += Math.PI * 2;
        while (value > Math.PI) value -= Math.PI * 2;
        return value;
    }

    function installNodeDrag() {
        if (state.dragBehavior) {
            return state.dragBehavior;
        }
        state.dragBehavior = d3.drag()
            .on("start", (event, node) => {
                if (node.kind !== "condition" || state.layoutMode !== "circle") return;
                if (!event.active) state.simulation.alphaTarget(0.12).restart();
                node.dragging = true;
                if (node.kind === "condition") {
                    state.draggingCondition = true;
                    node.fx = node.x;
                    node.fy = node.y;
                }
            })
            .on("drag", (event, node) => {
                if (node.kind !== "condition" || state.layoutMode !== "circle") return;
                if (node.kind === "condition") {
                    node.angle = Math.atan2(event.y - state.layout.centerY, event.x - state.layout.centerX);
                    redistributeConditionRing(state.graphNodes || [], node);
                }
                drawFrame(state.graphNodes || [], state.graphLinks || [], state.graphLoops || []);
            })
            .on("end", (event, node) => {
                if (node.kind !== "condition" || state.layoutMode !== "circle") return;
                node.dragging = false;
                if (!event.active) state.simulation.alphaTarget(0);
                state.draggingCondition = false;
                node.fx = null;
                node.fy = null;
                redistributeConditionRing(state.graphNodes || [], node);
                drawFrame(state.graphNodes || [], state.graphLinks || [], state.graphLoops || []);
            });
        return state.dragBehavior;
    }

    function drawFrame(nodes, links, loops) {
        state.graphNodes = nodes;
        state.graphLinks = links;
        state.graphLoops = loops;
        const byId = new Map(nodes.map((node) => [node.id, node]));
        drawNodes(nodes);
        drawLabels(nodes);
        drawLinks(links, byId, loops);
        drawArrowheads(links, byId, loops);
    }

    function measureLabelRow(text, font) {
        const value = String(text || "");
        const fontSize = Number(font.match(/(\d+(?:\.\d+)?)px/)?.[1]) || 8;
        if (!labelMeasureContext) {
            return {
                width: Math.ceil(value.length * fontSize * 0.58 + LABEL_ROW_PADDING_X * 2),
                height: Math.ceil(fontSize + LABEL_ROW_PADDING_Y * 2),
            };
        }
        labelMeasureContext.font = font;
        const metrics = labelMeasureContext.measureText(value);
        const ascent = metrics.actualBoundingBoxAscent || fontSize * 0.8;
        const descent = metrics.actualBoundingBoxDescent || fontSize * 0.2;
        return {
            width: Math.ceil(metrics.width + LABEL_ROW_PADDING_X * 2),
            height: Math.ceil(ascent + descent + LABEL_ROW_PADDING_Y * 2),
        };
    }

    function cardLayout(node) {
        const label = node.label || {};
        const human = label.human || label.code || node.id;
        const code = label.code || "";
        const value = label.value?.text || "";
        const valueText = value && value !== code ? value : "";
        const metaText = valueText ? `${code} ${valueText}` : code;
        const humanRow = measureLabelRow(human, HUMAN_LABEL_FONT);
        const metaRow = measureLabelRow(metaText, META_LABEL_FONT);
        const humanWidth = humanRow.width;
        const metaWidth = metaRow.width;
        const humanHeight = humanRow.height;
        const metaHeight = metaRow.height;
        const gap = 0;
        const width = Math.max(humanWidth, metaWidth) + 22;
        const height = humanHeight + gap + metaHeight + 16;
        const pos = labelPosition(node, width, height);
        return {
            x: pos.x,
            y: pos.y,
            width,
            height,
            human,
            code,
            valueText,
            humanWidth,
            metaWidth,
            humanHeight,
            metaHeight,
        };
    }

    function drawLinks(links, byId, loops) {
        const loopLinks = loopLinkMap(loops);
        linkLayer.selectAll("path").data(links, (d) => d.id).join("path")
            .attr("class", (d) => "link " + d.kind + (d.active ? " active" : " inactive") + (loopLinks.has(d.id) ? " loop" : ""))
            .attr("d", (d) => {
                const a = byId.get(d.source);
                const b = byId.get(d.target);
                if (!a || !b) return "";
                const loop = loopLinks.get(d.id);
                if (loop) return loopArcPath(a, b, loop, byId);
                if (state.layoutMode === "list") return listLinkPath(a, b, d);
                const sourcePoint = cardAnchorPoint(a, b);
                const targetPoint = cardAnchorPoint(b, a);
                return `M${sourcePoint.x},${sourcePoint.y} L${targetPoint.x},${targetPoint.y}`;
            });
    }

    function drawArrowheads(links, byId, loops) {
        const loopLinks = loopLinkMap(loops);
        arrowLayer.selectAll("path").data(links, (d) => d.id).join("path")
            .attr("class", (d) => "arrowHead " + d.kind + (d.active ? " active" : " inactive") + (loopLinks.has(d.id) ? " loop" : ""))
            .attr("fill", (d) => linkColor(d))
            .attr("d", (d) => {
                const a = byId.get(d.source);
                const b = byId.get(d.target);
                if (!a || !b) return "";
                const loop = loopLinks.get(d.id);
                if (loop) return "";
                const sourcePoint = state.layoutMode === "list" ? listArrowSource(a, b, d) : cardAnchorPoint(a, b);
                const targetPoint = state.layoutMode === "list" ? cardSidePoint(b, "left") : cardAnchorPoint(b, a);
                return arrowHeadPath(sourcePoint, targetPoint);
            });
    }

    function listLinkPath(source, target, link) {
        const sourcePoint = cardSidePoint(source, "right");
        const targetPoint = cardSidePoint(target, "left");
        if (listOutgoingCount(link.source) <= 1) {
            return `M${sourcePoint.x},${sourcePoint.y} L${targetPoint.x},${targetPoint.y}`;
        }
        const branchX = listBranchX(source, target);
        return `M${sourcePoint.x},${sourcePoint.y} L${branchX},${sourcePoint.y} L${branchX},${targetPoint.y} L${targetPoint.x},${targetPoint.y}`;
    }

    function listArrowSource(source, target, link) {
        if (state.layoutMode !== "list" || listOutgoingCount(link.source) <= 1) return cardSidePoint(source, "right");
        return { x: listBranchX(source, target), y: target.y };
    }

    function listOutgoingCount(sourceId) {
        return (state.graphLinks || []).filter((link) => link.source === sourceId).length;
    }

    function listBranchX(source, target) {
        return source.x + Math.max(58, Math.min(140, (target.x - source.x) * 0.45));
    }

    function linkColor(link) {
        return link.active ? "rgba(255,255,255,.84)" : "rgba(155,155,155,.45)";
    }

    function arrowHeadPath(source, target) {
        const dx = source.x - target.x;
        const dy = source.y - target.y;
        const length = Math.hypot(dx, dy) || 1;
        const ux = dx / length;
        const uy = dy / length;
        const tipX = target.x;
        const tipY = target.y;
        const back = 12;
        const width = 6;
        const baseX = tipX + ux * back;
        const baseY = tipY + uy * back;
        const leftX = baseX + uy * width;
        const leftY = baseY - ux * width;
        const rightX = baseX - uy * width;
        const rightY = baseY + ux * width;
        return `M${tipX},${tipY} L${leftX},${leftY} L${rightX},${rightY} Z`;
    }

    function loopLinkMap(loops) {
        const map = new Map();
        for (const loop of loops || []) {
            for (const linkId of loop.linkIds || []) map.set(linkId, loop);
        }
        return map;
    }

    function loopArcPath(a, b, loop, byId) {
        const ids = Logic.uniqueBy(loop.nodeIds || [], (id) => id);
        const points = ids.map((id) => byId.get(id)).filter(Boolean);
        if (points.length < 2) return `M${a.x},${a.y} L${b.x},${b.y}`;
        const cx = d3.mean(points, (point) => point.x);
        const cy = d3.mean(points, (point) => point.y);
        const radius = Math.max(52, d3.max(points, (point) => Math.hypot(point.x - cx, point.y - cy)) + 26);
        const start = Math.atan2(a.y - cy, a.x - cx);
        let end = Math.atan2(b.y - cy, b.x - cx);
        while (end <= start) end += Math.PI * 2;
        const large = end - start > Math.PI ? 1 : 0;
        return `M${a.x},${a.y} A${radius},${radius} 0 ${large},1 ${b.x},${b.y}`;
    }

    function drawNodes(nodes) {
        const joined = nodeLayer.selectAll("rect").data(nodes, (d) => d.id).join("rect");
        joined.call(installNodeDrag());
        joined
            .attr("x", (d) => cardLayout(d).x)
            .attr("y", (d) => cardLayout(d).y)
            .attr("width", (d) => cardLayout(d).width)
            .attr("height", (d) => cardLayout(d).height)
            .attr("rx", 2)
            .attr("ry", 2)
            .attr("fill", "rgba(0,0,0,0.001)")
            .attr("stroke", "rgba(0,0,0,0)")
            .attr("stroke-width", 0)
            .attr("pointer-events", "all");
    }

    function drawLabels(nodes) {
        const joined = labelLayer.selectAll("foreignObject").data(nodes, (d) => d.id).join((enter) => {
            const fo = enter.append("foreignObject").attr("class", "nodeLabel");
            const outer = fo.append("xhtml:div").attr("class", "nodeCard");
            return fo;
        });
        joined
            .attr("x", (d) => cardLayout(d).x)
            .attr("y", (d) => cardLayout(d).y)
            .attr("width", (d) => cardLayout(d).width)
            .attr("height", (d) => cardLayout(d).height);
        joined.select(".nodeCard")
            .attr("class", (d) => `nodeCard kind-${d.kind}`)
            .attr("style", (d) => `color:${d.color || "#6f2e2b"}`)
            .html((d) => {
                const box = cardLayout(d);
                return `${nodeBadgeHtml(d)}<div class="human" style="width:${box.width - 22}px">${escapeHtml(box.human)}</div><div class="meta" style="width:${box.width - 22}px">${escapeHtml(box.valueText ? `${box.code} ${box.valueText}` : box.code)}</div>`;
            });
    }

    function labelPosition(node, width = 0, height = 0) {
        if (state.layoutMode === "list") {
            if (node.kind === "condition") {
                return {
                    x: (state.layout.listConditionRightX ?? node.x) - width,
                    y: node.y - height / 2,
                };
            }
            return {
                x: state.layout.listConsequenceLeftX ?? node.x,
                y: node.y - height / 2,
            };
        }
        if (node.kind === "condition") {
            const angle = node.angle || 0;
            const radius = 17 + 18 + (height / 2);
            return {
                x: node.x + Math.cos(angle) * radius - width / 2,
                y: node.y + Math.sin(angle) * radius - height / 2,
            };
        }
        return { x: node.x - width / 2, y: node.y - height / 2 };
    }

    function setLayoutModeButtons() {
        document.querySelectorAll("[data-layout-mode]").forEach((button) => {
            button.classList.toggle("is-active", button.dataset.layoutMode === state.layoutMode);
        });
    }

    function cardCenter(node) {
        const box = cardLayout(node);
        return {
            x: box.x + box.width / 2,
            y: box.y + box.height / 2,
        };
    }

    function cardSidePoint(node, side) {
        const box = cardLayout(node);
        const centerY = box.y + box.height / 2;
        if (side === "left") {
            return { x: box.x, y: centerY };
        }
        if (side === "right") {
            return { x: box.x + box.width, y: centerY };
        }
        return cardCenter(node);
    }

    function cardAnchorPoint(node, other) {
        const box = cardLayout(node);
        const center = {
            x: box.x + box.width / 2,
            y: box.y + box.height / 2,
        };
        const target = cardCenter(other);
        const dx = target.x - center.x;
        const dy = target.y - center.y;
        if (!dx && !dy) {
            return center;
        }
        const halfW = box.width / 2;
        const halfH = box.height / 2;
        const scaleX = dx ? halfW / Math.abs(dx) : Infinity;
        const scaleY = dy ? halfH / Math.abs(dy) : Infinity;
        const scale = Math.min(scaleX, scaleY);
        return {
            x: center.x + dx * scale,
            y: center.y + dy * scale,
        };
    }

    function nodeBadgeHtml(node) {
        if (node.kind === "bridge") {
            return `
                <span class="nodeBadge" style="color:${escapeHtml(node.color || "#6f2e2b")}">
                    <svg viewBox="0 0 12 12" aria-hidden="true">
                        <path class="nodeBadgeStroke" d="M1 10 L6 1 L11 10 Z"></path>
                        <circle class="nodeBadgeStroke" cx="6" cy="6.5" r="3.5"></circle>
                    </svg>
                </span>`;
        }
        if (node.kind === "condition") {
            return `
                <span class="nodeBadge" style="color:${escapeHtml(node.color || "#f1ece2")}">
                    <svg viewBox="0 0 12 12" aria-hidden="true">
                        <path class="nodeBadgeStroke" d="M1 10 L6 1 L11 10 Z"></path>
                    </svg>
                </span>`;
        }
        return `
            <span class="nodeBadge" style="color:${escapeHtml(node.color || "#6f2e2b")}">
                <svg viewBox="0 0 12 12" aria-hidden="true">
                    <circle class="nodeBadgeStroke" cx="6" cy="6" r="4.5"></circle>
                </svg>
            </span>`;
    }

    function escapeHtml(value) {
        return String(value ?? "").replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;").replaceAll("\"", "&quot;");
    }

    el.viewButton.addEventListener("click", openViewModal);
    el.viewClose.addEventListener("click", () => el.viewModal.hidden = true);
    el.createView.addEventListener("click", () => createView().catch((error) => setStatus(error.message)));
    el.stateBall.addEventListener("click", () => el.adjustments.hidden = !el.adjustments.hidden);
    el.adjustClose.addEventListener("click", () => el.adjustments.hidden = true);
    el.physicsReset?.addEventListener("click", resetPhysics);
    el.physicsCenterExpulsion?.addEventListener("input", (event) => setPhysicsField("centerExpulsion", Number(event.currentTarget.value)));
    el.physicsLinkDistanceInput?.addEventListener("input", (event) => setPhysicsField("linkDistance", Number(event.currentTarget.value)));
    el.physicsLinkDistance?.addEventListener("input", (event) => setPhysicsField("linkDistance", Number(event.currentTarget.value)));
    el.physicsNodeRepulsionInput?.addEventListener("input", (event) => setPhysicsField("nodeRepulsion", Number(event.currentTarget.value)));
    el.physicsNodeRepulsion?.addEventListener("input", (event) => setPhysicsField("nodeRepulsion", Number(event.currentTarget.value)));
    el.conditionColor?.addEventListener("input", (event) => setPaletteColor("condition", event.target.value));
    el.consequenceColor?.addEventListener("input", (event) => setPaletteColor("consequence", event.target.value));
    el.inactiveColor?.addEventListener("input", (event) => setPaletteColor("inactive", event.target.value));
    el.distinctCondition?.addEventListener("change", async (event) => {
        state.distinctCondition = Boolean(event.target.checked);
        render();
        try { await postAction("set-distinctness", { distinctness: distinctnessValue() }); } catch (_error) {}
    });
    el.distinctConsequence?.addEventListener("change", async (event) => {
        state.distinctConsequence = Boolean(event.target.checked);
        render();
        try { await postAction("set-distinctness", { distinctness: distinctnessValue() }); } catch (_error) {}
    });
    document.querySelectorAll("[data-layout-mode]").forEach((button) => {
        button.addEventListener("click", () => {
            state.layoutMode = button.dataset.layoutMode === "circle" ? "circle" : "list";
            saveLayoutMode();
            setLayoutModeButtons();
            syncLayoutModePhysicsVisibility();
            render();
        });
    });
    window.addEventListener("resize", render);

    loadContract().catch((error) => setStatus(error.message));
})();
"####,
    );
    script
}
