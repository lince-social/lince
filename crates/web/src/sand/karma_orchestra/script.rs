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
        conditionPulling: 0.25,
        nodeRepulsion: -640,
    };
    const LABEL_ROW_PADDING_X = 2;
    const LABEL_ROW_PADDING_Y = 1;
    const HUMAN_LABEL_FONT = "700 8px ui-sans-serif, system-ui";
    const META_LABEL_FONT = "6.5px ui-sans-serif, system-ui";
    const state = {
        contract: null,
        graph: null,
        distinctness: "none",
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
        physicsCenterExpulsion: document.getElementById("karma-physics-center-expulsion"),
        physicsCenterExpulsionValue: document.getElementById("karma-physics-center-expulsion-value"),
        physicsConditionPulling: document.getElementById("karma-physics-condition-pulling"),
        physicsConditionPullingValue: document.getElementById("karma-physics-condition-pulling-value"),
        physicsNodeRepulsion: document.getElementById("karma-physics-node-repulsion"),
        physicsNodeRepulsionValue: document.getElementById("karma-physics-node-repulsion-value"),
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
    const defs = svg.append("defs");
    const linkMask = defs.append("mask")
        .attr("id", "karma-link-mask")
        .attr("maskUnits", "userSpaceOnUse")
        .attr("maskContentUnits", "userSpaceOnUse")
        .attr("x", -5000)
        .attr("y", -5000)
        .attr("width", 10000)
        .attr("height", 10000);
    const nodeMask = defs.append("mask")
        .attr("id", "karma-node-mask")
        .attr("maskUnits", "userSpaceOnUse")
        .attr("maskContentUnits", "userSpaceOnUse")
        .attr("x", -5000)
        .attr("y", -5000)
        .attr("width", 10000)
        .attr("height", 10000);
    const linkLayer = root.append("g");
    const nodeLayer = root.append("g");
    const arrowLayer = root.append("g");
    const labelLayer = root.append("g");
    linkLayer.attr("mask", "url(#karma-link-mask)");
    nodeLayer.attr("mask", "url(#karma-node-mask)");
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
        state.distinctness = data?.state?.distinctness || "none";
        state.palette = loadPalette();
        state.physics = loadPhysics();
        setDistinctnessButtons();
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

    function physicsStorageKey() {
        return "karma-orchestra:physics";
    }

    function loadPhysics() {
        try {
            const raw = window.localStorage.getItem(physicsStorageKey());
            if (!raw) return { ...DEFAULT_PHYSICS };
            const parsed = JSON.parse(raw);
            const centerExpulsion = Number(parsed?.centerExpulsion);
            const conditionPulling = Number(parsed?.conditionPulling);
            const nodeRepulsion = Number(parsed?.nodeRepulsion);
            return {
                centerExpulsion: Number.isFinite(centerExpulsion) ? centerExpulsion : DEFAULT_PHYSICS.centerExpulsion,
                conditionPulling: Number.isFinite(conditionPulling) ? conditionPulling : DEFAULT_PHYSICS.conditionPulling,
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
        if (el.physicsConditionPulling) el.physicsConditionPulling.value = String(state.physics.conditionPulling);
        if (el.physicsConditionPullingValue) el.physicsConditionPullingValue.textContent = String(state.physics.conditionPulling);
        if (el.physicsNodeRepulsion) el.physicsNodeRepulsion.value = String(state.physics.nodeRepulsion);
        if (el.physicsNodeRepulsionValue) el.physicsNodeRepulsionValue.textContent = String(state.physics.nodeRepulsion);
    }

    function setPhysicsField(key, value) {
        state.physics[key] = value;
        savePhysics();
        syncPhysicsInputs();
        applyPhysicsToSimulation();
    }

    function resetPhysics() {
        state.physics = { ...DEFAULT_PHYSICS };
        savePhysics();
        syncPhysicsInputs();
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
        let nodes = graph.nodes || [];
        if (state.distinctness === "none" || state.distinctness === "consequence") {
            const conditions = [];
            for (const rule of graph.karmaRows || []) {
                const base = nodes.find((node) => node.id === "condition:" + rule.conditionId);
                if (base) {
                    conditions.push({ ...base, id: "condition-row:" + rule.karmaId, entityId: rule.conditionId, ruleIds: [rule.karmaId] });
                }
            }
            const others = nodes.filter((node) => node.kind !== "condition");
            nodes = [...conditions, ...others];
        }
        let links = addVisualDirectLinks(graph, graph.links || []);
        const inactiveRuleIds = new Set((graph.karmaRows || []).filter(ruleHasZeroQuantity).map((rule) => Number(rule.karmaId)));
        const directRuleIds = new Set(links.flatMap((link) => link.kind === "direct" ? (link.ruleIds || []) : []));
        if (state.distinctness === "none" || state.distinctness === "consequence") {
            links = links.map((link) => {
                const next = { ...link };
                if (next.source.startsWith("condition:")) {
                    const ruleId = (next.ruleIds || []).find((id) => directRuleIds.has(id));
                    if (ruleId) next.source = "condition-row:" + ruleId;
                }
                if (next.target.startsWith("condition:")) {
                    const conditionId = Number(next.target.split(":")[1]);
                    const rule = (graph.karmaRows || []).find((row) => Number(row.conditionId) === conditionId);
                    if (rule) next.target = "condition-row:" + rule.karmaId;
                }
                return next;
            });
        }
        links = links.map((link) => ({
            ...link,
            active: !(link.ruleIds || []).some((id) => inactiveRuleIds.has(Number(id))),
        }));
        if (state.distinctness === "condition" || state.distinctness === "both") {
            nodes = Logic.uniqueBy(nodes, (node) => node.id);
            links = Logic.uniqueBy(links, (link) => link.source + ">" + link.target + ">" + link.kind);
        }
        if (state.distinctness === "consequence" || state.distinctness === "both") {
            nodes = Logic.uniqueBy(nodes, (node) => node.kind === "consequence" || node.kind === "bridge" ? node.id : node.id);
        }
        return { ...graph, nodes, links };
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
            lockConditionNode(node);
            node.color = nodeHasActiveQuantity(node, graph) ? state.palette.condition : state.palette.inactive;
            byId.set(node.id, node);
        });
        const bySource = new Map();
        for (const link of links) {
            if (!bySource.has(link.source)) bySource.set(link.source, []);
            bySource.get(link.source).push(link);
        }
        otherNodes.forEach((node, index) => {
            const incoming = links.find((link) => link.target === node.id);
            const source = incoming ? byId.get(incoming.source) : null;
            const angle = source?.angle ?? (-Math.PI / 2 + index);
            const fanIndex = source ? (bySource.get(incoming.source) || []).findIndex((link) => link.target === node.id) : 0;
            const spread = (fanIndex - 1) * 0.22;
            node.x = state.layout.centerX + Math.cos(angle + spread) * (radius + 150);
            node.y = state.layout.centerY + Math.sin(angle + spread) * (radius + 150);
            node.color = nodeHasActiveQuantity(node, graph) ? state.palette.consequence : state.palette.inactive;
            byId.set(node.id, node);
        });

        runPhysics(nodes, links, byId, graph.loops || []);
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
        state.simulation = d3.forceSimulation(nodes)
            .force("link", d3.forceLink(forceLinks).id((node) => node.id).distance((link) => link.kind === "fulfillment" ? 110 : 150).strength((link) => link.kind === "fulfillment" ? Math.min(1, physics.conditionPulling * 1.15) : physics.conditionPulling))
            .force("charge", d3.forceManyBody().strength((node) => node.kind === "condition" ? -12 : physics.nodeRepulsion))
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
        if (!state.simulation) {
            return;
        }
        const physics = state.physics || DEFAULT_PHYSICS;
        const linkForce = state.simulation.force("link");
        if (linkForce) {
            linkForce
                .strength((link) => link.kind === "fulfillment" ? Math.min(1, physics.conditionPulling * 1.15) : physics.conditionPulling);
        }
        state.simulation
            .force("charge", d3.forceManyBody().strength((node) => node.kind === "condition" ? -12 : physics.nodeRepulsion))
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
                if (!event.active) state.simulation.alphaTarget(0.12).restart();
                node.dragging = true;
                if (node.kind === "condition") {
                    state.draggingCondition = true;
                    node.fx = node.x;
                    node.fy = node.y;
                }
                if (node.kind !== "condition") {
                    node.fx = node.x;
                    node.fy = node.y;
                }
            })
            .on("drag", (event, node) => {
                if (node.kind === "condition") {
                    node.angle = Math.atan2(event.y - state.layout.centerY, event.x - state.layout.centerX);
                    redistributeConditionRing(state.graphNodes || [], node);
                } else {
                    node.fx = event.x;
                    node.fy = event.y;
                }
                drawFrame(state.graphNodes || [], state.graphLinks || [], state.graphLoops || []);
            })
            .on("end", (event, node) => {
                node.dragging = false;
                if (!event.active) state.simulation.alphaTarget(0);
                if (node.kind !== "condition") {
                    node.fx = null;
                    node.fy = null;
                } else {
                    state.draggingCondition = false;
                    node.fx = null;
                    node.fy = null;
                    redistributeConditionRing(state.graphNodes || [], node);
                    drawFrame(state.graphNodes || [], state.graphLinks || [], state.graphLoops || []);
                }
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
        drawMasks(nodes);
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

    function drawMasks(nodes) {
        const baseRect = (mask) => {
            mask.selectAll("*").remove();
            mask.append("rect")
                .attr("x", -2000)
                .attr("y", -2000)
                .attr("width", 6000)
                .attr("height", 6000)
                .attr("fill", "white");
        };
        baseRect(linkMask);
        baseRect(nodeMask);

        for (const node of nodes) {
            if (!Number.isFinite(node.x) || !Number.isFinite(node.y)) continue;
            linkMask.append("circle")
                .attr("cx", node.x)
                .attr("cy", node.y)
                .attr("r", 19)
                .attr("fill", "black");
        }

        for (const box of measuredLabelTextBoxes()) {
            for (const mask of [linkMask, nodeMask]) {
                mask.append("rect")
                    .attr("x", box.x)
                    .attr("y", box.y)
                    .attr("width", box.width)
                    .attr("height", box.height)
                    .attr("rx", 2)
                    .attr("ry", 2)
                    .attr("fill", "black");
            }
        }
    }

    function measuredLabelTextBoxes() {
        const boxes = [];
        const transform = d3.zoomTransform(el.svg);
        const svgRect = el.svg.getBoundingClientRect();
        labelLayer.selectAll(".human,.meta").each(function () {
            const textNode = this.firstChild;
            if (!textNode) return;
            const range = document.createRange();
            range.selectNodeContents(this);
            const rect = range.getBoundingClientRect();
            range.detach();
            if (!rect.width || !rect.height) return;
            const pad = 2;
            boxes.push({
                x: (rect.left - svgRect.left - transform.x) / transform.k - pad,
                y: (rect.top - svgRect.top - transform.y) / transform.k - pad,
                width: rect.width / transform.k + pad * 2,
                height: rect.height / transform.k + pad * 2,
            });
        });
        return boxes;
    }

    function labelLayout(node) {
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
        const width = Math.max(humanWidth, metaWidth);
        const height = humanHeight + gap + metaHeight;
        const pos = labelPosition(node, width, height);
        return {
            x: pos.x,
            y: pos.y,
            width,
            height,
            human,
            code,
            valueText,
            humanBox: {
                x: pos.x + (width - humanWidth) / 2,
                y: pos.y,
                width: humanWidth,
                height: humanHeight,
            },
            metaBox: {
                x: pos.x + (width - metaWidth) / 2,
                y: pos.y + humanHeight + gap,
                width: metaWidth,
                height: metaHeight,
            },
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
                const sourcePoint = anchorPoint(a, b, true);
                const targetPoint = anchorPoint(b, a, false);
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
                return arrowHeadPath(a, b);
            });
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
        const radius = target.kind === "bridge" ? 9 : 17;
        const tipGap = 1;
        const tipX = target.x + ux * (radius + tipGap);
        const tipY = target.y + uy * (radius + tipGap);
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

    function anchorPoint(node, other, isSource) {
        if (!node || !other) {
            return node || { x: 0, y: 0 };
        }
        if (node.kind === "condition") {
            const angle = isSource
                ? node.angle || 0
                : Math.atan2(other.y - node.y, other.x - node.x);
            const radius = 17;
            return {
                x: node.x + Math.cos(angle) * radius,
                y: node.y + Math.sin(angle) * radius,
            };
        }
        const dx = other.x - node.x;
        const dy = other.y - node.y;
        const length = Math.hypot(dx, dy) || 1;
        const radius = node.kind === "bridge" ? 9 : 17;
        const ux = dx / length;
        const uy = dy / length;
        return { x: node.x + ux * radius, y: node.y + uy * radius };
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
        const joined = nodeLayer.selectAll("g").data(nodes, (d) => d.id).join((enter) => {
            const g = enter.append("g");
            g.append("path").attr("class", "nodeShape");
            g.append("path").attr("class", "bridgeInner").attr("display", "none");
            return g;
        });
        joined.call(installNodeDrag());
        joined.attr("transform", (d) => `translate(${d.x},${d.y})`);
        joined.select("path")
            .attr("fill", (d) => d.color || "#6f2e2b")
            .attr("d", (d) => d.kind === "condition" ? trianglePath(15) : circlePath(17));
        joined.select(".bridgeInner")
            .attr("display", (d) => d.kind === "bridge" ? null : "none")
            .attr("d", trianglePath(9));
        joined.filter((d) => d.kind === "condition").attr("transform", (d) => `translate(${d.x},${d.y}) rotate(${(d.angle || 0) * 180 / Math.PI + 90})`);
    }

    function drawLabels(nodes) {
        const joined = labelLayer.selectAll("foreignObject").data(nodes, (d) => d.id).join((enter) => {
            const fo = enter.append("foreignObject").attr("class", "nodeLabel");
            const outer = fo.append("xhtml:div").attr("class", "nodeLabelBox");
            outer.append("div").attr("class", "human");
            outer.append("div").attr("class", "meta");
            return fo;
        });
        joined
            .attr("x", (d) => labelLayout(d).x)
            .attr("y", (d) => labelLayout(d).y)
            .attr("width", (d) => labelLayout(d).width)
            .attr("height", (d) => labelLayout(d).height);
        joined.select(".nodeLabelBox")
            .html((d) => {
                const box = labelLayout(d);
                return `<div class="human" style="width:${box.humanBox.width}px">${escapeHtml(box.human)}</div><div class="meta" style="width:${box.metaBox.width}px">${escapeHtml(box.valueText ? `${box.code} ${box.valueText}` : box.code)}</div>`;
            });
    }

    function labelPosition(node, width = 0, height = 0) {
        if (node.kind === "condition") {
            const angle = node.angle || 0;
            const radius = 17 + 18 + (height / 2);
            return {
                x: node.x + Math.cos(angle) * radius - width / 2,
                y: node.y + Math.sin(angle) * radius - height / 2,
            };
        }
        return { x: node.x - width / 2, y: node.y - 17 - height - 18 };
    }

    function trianglePath(size) {
        return `M0,${-size} L${size},${size} L${-size},${size} Z`;
    }

    function circlePath(radius) {
        return `M${-radius},0 a${radius},${radius} 0 1,0 ${radius * 2},0 a${radius},${radius} 0 1,0 ${-radius * 2},0`;
    }

    function setDistinctnessButtons() {
        document.querySelectorAll("[data-distinctness]").forEach((button) => {
            button.classList.toggle("is-active", button.dataset.distinctness === state.distinctness);
        });
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
    el.physicsConditionPulling?.addEventListener("input", (event) => setPhysicsField("conditionPulling", Number(event.currentTarget.value)));
    el.physicsNodeRepulsion?.addEventListener("input", (event) => setPhysicsField("nodeRepulsion", Number(event.currentTarget.value)));
    el.conditionColor?.addEventListener("input", (event) => setPaletteColor("condition", event.target.value));
    el.consequenceColor?.addEventListener("input", (event) => setPaletteColor("consequence", event.target.value));
    el.inactiveColor?.addEventListener("input", (event) => setPaletteColor("inactive", event.target.value));
    document.querySelectorAll("[data-distinctness]").forEach((button) => {
        button.addEventListener("click", async () => {
            state.distinctness = button.dataset.distinctness || "none";
            setDistinctnessButtons();
            render();
            try { await postAction("set-distinctness", { distinctness: state.distinctness }); } catch (_error) {}
        });
    });
    window.addEventListener("resize", render);

    loadContract().catch((error) => setStatus(error.message));
})();
"####,
    );
    script
}
