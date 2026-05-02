pub(crate) fn script() -> String {
    let mut script = String::from(include_str!("logic.js"));
    script.push_str(
        r####"
(() => {
    const d3 = window.d3;
    const Logic = globalThis.KarmaOrchestraLogic;
    const frame = window.frameElement;
    const state = {
        contract: null,
        graph: null,
        distinctness: "none",
        width: 0,
        height: 0,
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
        summaryRules: document.getElementById("karma-summary-rules"),
        summaryConditions: document.getElementById("karma-summary-conditions"),
        summaryConsequences: document.getElementById("karma-summary-consequences"),
        summaryLoops: document.getElementById("karma-summary-loops"),
    };

    const svg = d3.select(el.svg);
    const root = svg.append("g");
    const linkLayer = root.append("g");
    const nodeLayer = root.append("g");
    const labelLayer = root.append("g");
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
        setDistinctnessButtons();
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
        const directRuleIds = new Set((graph.links || []).flatMap((link) => link.ruleIds || []));
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
        let links = graph.links || [];
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
        if (state.distinctness === "condition" || state.distinctness === "both") {
            nodes = Logic.uniqueBy(nodes, (node) => node.id);
            links = Logic.uniqueBy(links, (link) => link.source + ">" + link.target + ">" + link.kind);
        }
        if (state.distinctness === "consequence" || state.distinctness === "both") {
            nodes = Logic.uniqueBy(nodes, (node) => node.kind === "consequence" || node.kind === "bridge" ? node.id : node.id);
        }
        return { ...graph, nodes, links };
    }

    function render() {
        const graph = renderedGraph();
        const nodes = graph.nodes || [];
        const links = graph.links || [];
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
        const byId = new Map();
        conditionNodes.forEach((node, index) => {
            const angle = -Math.PI / 2 + (index / Math.max(1, conditionNodes.length)) * Math.PI * 2;
            node.x = width / 2 + Math.cos(angle) * radius;
            node.y = height / 2 + Math.sin(angle) * radius;
            node.angle = angle;
            node.color = gray(index, conditionNodes.length);
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
            node.x = width / 2 + Math.cos(angle + spread) * (radius + 150);
            node.y = height / 2 + Math.sin(angle + spread) * (radius + 150);
            node.color = "#6f2e2b";
            byId.set(node.id, node);
        });

        drawLinks(links, byId, graph.loops || []);
        drawNodes(nodes);
        drawLabels(nodes);
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

    function gray(index, count) {
        const value = count <= 1 ? 0 : Math.round((index / (count - 1)) * 255);
        return "rgb(" + value + "," + value + "," + value + ")";
    }

    function drawLinks(links, byId, loops) {
        const loopLinks = loopLinkMap(loops);
        linkLayer.selectAll("path").data(links, (d) => d.id).join("path")
            .attr("class", (d) => "link " + d.kind + (loopLinks.has(d.id) ? " loop" : ""))
            .attr("d", (d) => {
                const a = byId.get(d.source);
                const b = byId.get(d.target);
                if (!a || !b) return "";
                const loop = loopLinks.get(d.id);
                if (loop) return loopArcPath(a, b, loop, byId);
                const dx = b.x - a.x;
                const dy = b.y - a.y;
                const dr = d.kind === "fulfillment" ? Math.hypot(dx, dy) * 0.8 : 0;
                return dr ? `M${a.x},${a.y} A${dr},${dr} 0 0,1 ${b.x},${b.y}` : `M${a.x},${a.y} L${b.x},${b.y}`;
            });
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
        joined.attr("transform", (d) => `translate(${d.x},${d.y})`);
        joined.select("path")
            .attr("fill", (d) => d.color || "#6f2e2b")
            .attr("d", (d) => d.kind === "condition" ? trianglePath(15) : circlePath(17));
        joined.select(".bridgeInner")
            .attr("display", (d) => d.kind === "bridge" ? null : "none")
            .attr("d", trianglePath(9));
        joined.filter((d) => d.kind === "condition").attr("transform", (d) => `translate(${d.x},${d.y}) rotate(${(d.angle || 0) * 180 / Math.PI + 270})`);
    }

    function drawLabels(nodes) {
        const joined = labelLayer.selectAll("foreignObject").data(nodes, (d) => d.id).join("foreignObject")
            .attr("width", 190)
            .attr("height", 58)
            .attr("x", (d) => d.x - 95)
            .attr("y", (d) => d.y - 70);
        joined.html((d) => {
            const label = d.label || {};
            return `<div class="nodeLabel"><div class="human">${escapeHtml(label.human || label.code || d.id)}</div><div class="meta"><span>${escapeHtml(label.code || "")}</span><span>${escapeHtml(label.value?.text || "")}</span></div></div>`;
        });
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
