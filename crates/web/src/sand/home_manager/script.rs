pub(super) fn script() -> String {
    let mut script = String::from(super::knowledge_base::KNOWLEDGE_BASE_JS);
    script.push_str(
        r##"
(() => {
  const ALIMENTUM_NAMESPACE = "nutrition.alimentum.v1";
  const frame = window.frameElement;
  const instanceId = String(frame?.dataset?.packageInstanceId || "preview").trim() || "preview";
  const fallbackStorageKey = "home-manager/nutrition/v2/" + instanceId;
  const { categories, builtInFoods } = window.HomeManagerNutritionKnowledge;

  const nodes = {
    tabs: Array.from(document.querySelectorAll(".tab")),
    nutritionTab: document.getElementById("nutrition-tab"),
    billsTab: document.getElementById("bills-tab"),
    weight: document.getElementById("profile-weight"),
    height: document.getElementById("profile-height"),
    age: document.getElementById("profile-age"),
    sex: document.getElementById("profile-sex"),
    activity: document.getElementById("profile-activity"),
    days: document.getElementById("plan-days"),
    pots: document.getElementById("plan-pots"),
    potVolume: document.getElementById("plan-pot-volume"),
    meals: document.getElementById("plan-meals"),
    fiber: document.getElementById("constraint-fiber"),
    protein: document.getElementById("constraint-protein"),
    search: document.getElementById("food-search"),
    categoryFilter: document.getElementById("category-filter"),
    foodList: document.getElementById("food-list"),
    marmitaList: document.getElementById("marmita-list"),
    nutrientTotals: document.getElementById("nutrient-totals"),
    shoppingList: document.getElementById("shopping-list"),
    metricFoods: document.getElementById("metric-foods"),
    metricKcal: document.getElementById("metric-kcal"),
    metricPrice: document.getElementById("metric-price"),
    metricVolume: document.getElementById("metric-volume"),
    status: document.getElementById("status"),
    generate: document.getElementById("generate-plan"),
    optimize: document.getElementById("optimize-plan"),
    save: document.getElementById("save-plan"),
    newAlimentum: document.getElementById("new-alimentum"),
    dialog: document.getElementById("alimentum-dialog"),
    alimentumForm: document.getElementById("alimentum-form"),
    alimentumRecordId: document.getElementById("alimentum-record-id"),
    alimentumExtensionId: document.getElementById("alimentum-extension-id"),
    alimentumName: document.getElementById("alimentum-name"),
    alimentumCategory: document.getElementById("alimentum-category"),
    alimentumPrice: document.getElementById("alimentum-price"),
    alimentumDensity: document.getElementById("alimentum-density"),
    alimentumNutrients: document.getElementById("alimentum-nutrients"),
    alimentumSubmit: document.getElementById("alimentum-submit"),
  };

  const nutrientFields = [
    f("kcal", "kcal", "", 0, "energy"),
    f("proteinG", "protein", "g", 1, "macro"),
    f("carbG", "carb", "g", 1, "macro"),
    f("fatG", "fat", "g", 1, "macro"),
    f("fiberG", "fiber", "g", 1, "macro"),
    f("calciumMg", "Ca", "mg", 0, "mineral"),
    f("ironMg", "Fe", "mg", 1, "mineral"),
    f("magnesiumMg", "Mg", "mg", 0, "mineral"),
    f("potassiumMg", "K", "mg", 0, "mineral"),
    f("zincMg", "Zn", "mg", 1, "mineral"),
    f("sodiumMg", "Na", "mg", 0, "mineral"),
    f("phosphorusMg", "P", "mg", 0, "mineral"),
    f("seleniumMcg", "Se", "mcg", 1, "mineral"),
    f("copperMg", "Cu", "mg", 2, "mineral"),
    f("manganeseMg", "Mn", "mg", 2, "mineral"),
    f("vitaminCMg", "Vit C", "mg", 0, "vitamin"),
    f("vitaminAMcg", "Vit A", "mcg", 0, "vitamin"),
    f("vitaminDMcg", "Vit D", "mcg", 1, "vitamin"),
    f("vitaminEMg", "Vit E", "mg", 1, "vitamin"),
    f("vitaminKMcg", "Vit K", "mcg", 0, "vitamin"),
    f("thiaminMg", "B1", "mg", 2, "vitamin"),
    f("riboflavinMg", "B2", "mg", 2, "vitamin"),
    f("niacinMg", "B3", "mg", 1, "vitamin"),
    f("vitaminB6Mg", "B6", "mg", 2, "vitamin"),
    f("folateMcg", "B9", "mcg", 0, "vitamin"),
    f("b12Mcg", "B12", "mcg", 1, "vitamin"),
  ];
  const nutrientInputs = new Map();

  function f(key, label, unit, digits, group) {
    return { key, label, unit, digits, group };
  }

  const defaults = {
    tab: "nutrition",
    profile: { weight: 70, height: 170, age: 32, sex: "female", activity: 1.45 },
    plan: { days: 5, pots: 10, potVolume: 0.75, meals: 2, fiberMin: 25, proteinMin: 70 },
    selected: {},
    generated: { allocations: [], shopping: [], totals: zeroTotals(), message: "" },
  };

  let customFoods = [];
  let hostReady = false;
  let userTouchedState = false;
  let state = loadFallbackState();

  function clone(value, fallback = null) {
    try { return JSON.parse(JSON.stringify(value === undefined ? fallback : value)); } catch { return fallback; }
  }

  function round(value, digits = 2) {
    const factor = 10 ** digits;
    return Math.round((Number(value) || 0) * factor) / factor;
  }

  function zeroTotals() {
    return { kcal: 0, proteinG: 0, carbG: 0, fatG: 0, fiberG: 0, calciumMg: 0, ironMg: 0, magnesiumMg: 0, potassiumMg: 0, zincMg: 0, vitaminCMg: 0, vitaminAIu: 0, folateMcg: 0, b12Mcg: 0, price: 0, volumeL: 0 };
  }

  function loadFallbackState() {
    try {
      const parsed = JSON.parse(localStorage.getItem(fallbackStorageKey) || "null");
      return normalizeState(parsed);
    } catch {
      return normalizeState(null);
    }
  }

  function normalizeState(raw) {
    const next = clone(defaults, {});
    if (raw && typeof raw === "object") {
      next.tab = raw.tab === "bills" ? "bills" : "nutrition";
      next.profile = { ...next.profile, ...(raw.profile || {}) };
      next.plan = { ...next.plan, ...(raw.plan || {}) };
      next.selected = raw.selected && typeof raw.selected === "object" ? raw.selected : {};
      next.generated = raw.generated && typeof raw.generated === "object" ? { ...next.generated, ...raw.generated } : next.generated;
    }
    return next;
  }

  function saveState(statusMessage = "Saved") {
    userTouchedState = true;
    const payload = clone(state, {});
    try { localStorage.setItem(fallbackStorageKey, JSON.stringify(payload)); } catch {}
    if (window.LinceWidgetHost?.patchCardState) {
      window.LinceWidgetHost.patchCardState({ homeManagerNutrition: payload });
    }
    if (statusMessage) setStatus(statusMessage);
  }

  function allFoods() {
    return builtInFoods.concat(customFoods);
  }

  function selectedFoods() {
    return allFoods().filter((food) => state.selected[food.id]?.enabled);
  }

  function readInputs() {
    state.profile.weight = number(nodes.weight.value, 70);
    state.profile.height = number(nodes.height.value, 170);
    state.profile.age = number(nodes.age.value, 32);
    state.profile.sex = nodes.sex.value === "male" ? "male" : "female";
    state.profile.activity = number(nodes.activity.value, 1.45);
    state.plan.days = Math.max(1, Math.round(number(nodes.days.value, 5)));
    state.plan.pots = Math.max(1, Math.round(number(nodes.pots.value, 10)));
    state.plan.potVolume = number(nodes.potVolume.value, 0.75);
    state.plan.meals = Math.max(1, Math.round(number(nodes.meals.value, 2)));
    state.plan.fiberMin = number(nodes.fiber.value, 25);
    state.plan.proteinMin = number(nodes.protein.value, 70);
  }

  function writeInputs() {
    nodes.weight.value = state.profile.weight;
    nodes.height.value = state.profile.height;
    nodes.age.value = state.profile.age;
    nodes.sex.value = state.profile.sex;
    nodes.activity.value = state.profile.activity;
    nodes.days.value = state.plan.days;
    nodes.pots.value = state.plan.pots;
    nodes.potVolume.value = state.plan.potVolume;
    nodes.meals.value = state.plan.meals;
    nodes.fiber.value = state.plan.fiberMin;
    nodes.protein.value = state.plan.proteinMin;
  }

  function number(value, fallback) {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : fallback;
  }

  function calorieTarget() {
    const p = state.profile;
    const bmr = p.sex === "male"
      ? 10 * p.weight + 6.25 * p.height - 5 * p.age + 5
      : 10 * p.weight + 6.25 * p.height - 5 * p.age - 161;
    return Math.max(1200, Math.round(bmr * p.activity));
  }

  function nutrientTimes(food, grams) {
    const totals = zeroTotals();
    const factor = grams / 100;
    for (const [key, value] of Object.entries(food.nutrients || {})) {
      if (totals[key] !== undefined && value != null) totals[key] += Number(value) * factor;
    }
    totals.price = (Number(food.pricePerKg) || 0) * grams / 1000;
    totals.volumeL = grams / Math.max(0.1, Number(food.densityGPerMl) || 0.8) / 1000;
    return totals;
  }

  function addTotals(a, b) {
    for (const key of Object.keys(a)) a[key] += Number(b[key]) || 0;
    return a;
  }

  function buildShopping(allocations) {
    const byFood = new Map();
    for (const item of allocations) {
      const current = byFood.get(item.foodId) || { ...item, grams: 0, price: 0 };
      current.grams += item.grams;
      current.price += item.price;
      byFood.set(item.foodId, current);
    }
    return Array.from(byFood.values()).sort((a, b) => a.name.localeCompare(b.name));
  }

  function generatePlan(useOptimizer) {
    readInputs();
    const foods = selectedFoods();
    if (!foods.length) {
      for (const food of defaultFoodsForPlan()) {
        state.selected[food.id] = { enabled: true, minG: 0, maxG: state.plan.days * 600, forcedG: 0, pricePerKg: food.pricePerKg };
      }
    }
    const active = selectedFoods();
    const allocations = useOptimizer ? simplexOptimize(active) : defaultAllocation(active);
    const totals = allocations.reduce((sum, item) => addTotals(sum, item.totals), zeroTotals());
    const shopping = buildShopping(allocations);
    state.generated = {
      allocations,
      shopping,
      totals,
      message: allocations.length ? "Plan generated" : "No feasible allocation",
    };
    if (!useOptimizer) setStatus("Plan generated");
    saveState(null);
    render();
  }

  function defaultAllocation(foods) {
    const perFood = Math.max(80, Math.round((state.plan.pots * state.plan.potVolume * 1000 * 0.65) / Math.max(1, foods.length)));
    return foods.map((food) => allocationFor(food, clamp(perFood, selection(food).minG, selection(food).maxG || 2000)));
  }

  function defaultFoodsForPlan() {
    const preferred = [
      "Feijao carioca cozido",
      "Lentilha cozida",
      "Arroz integral cozido",
      "Batata doce cozida",
      "Frango peito grelhado",
      "Ovo cozido",
      "Carne bovina magra",
      "Iogurte natural",
      "Couve",
      "Brocolis",
      "Banana prata",
      "Castanha-do-para",
      "Azeite de oliva",
    ];
    const foods = allFoods();
    const picked = [];
    for (const name of preferred) {
      const item = foods.find((food) => food.name === name);
      if (item && !picked.includes(item)) picked.push(item);
    }
    for (const category of ["legumes", "cereals", "meat_eggs", "vegetables", "fruits"]) {
      const item = foods.find((food) => food.category === category && !picked.includes(food));
      if (item) picked.push(item);
    }
    return picked.slice(0, 14);
  }

  function simplexOptimize(foods) {
    const targetKcal = calorieTarget() * state.plan.days;
    const targetProtein = state.plan.proteinMin * state.plan.days;
    const targetFiber = state.plan.fiberMin * state.plan.days;
    const maxVolumeL = state.plan.pots * state.plan.potVolume;
    const baseAllocations = [];
    const baseTotals = zeroTotals();
    const variables = [];
    const infeasible = [];

    for (const food of foods) {
      const forced = Math.max(0, selection(food).forcedG || 0);
      const min = Math.max(0, selection(food).minG || 0);
      const max = Math.max(0, selection(food).maxG || state.plan.days * 900);
      const grams = Math.max(forced, min);
      if (grams > max) {
        infeasible.push(`${food.name} forced/min grams exceed max`);
      }
      if (grams > 0) {
        const item = allocationFor(food, grams);
        baseAllocations.push(item);
        addTotals(baseTotals, item.totals);
      }
      const room = Math.max(0, max - grams);
      if (room > 0) {
        variables.push({ food, room });
      }
    }

    const remainingVolume = maxVolumeL - baseTotals.volumeL;
    if (remainingVolume < -0.0001) {
      infeasible.push("forced foods exceed total marmita volume");
    }

    const constraints = [];
    variables.forEach((variable, index) => {
      const coeffs = Array(variables.length).fill(0);
      coeffs[index] = 1;
      constraints.push({ coeffs, type: "<=", b: variable.room });
    });
    constraints.push({
      coeffs: variables.map(({ food }) => 1 / Math.max(0.1, food.densityGPerMl || 0.8) / 1000),
      type: "<=",
      b: Math.max(0, remainingVolume),
    });
    addMinimumConstraint(constraints, variables, "kcal", targetKcal - baseTotals.kcal);
    addMinimumConstraint(constraints, variables, "proteinG", targetProtein - baseTotals.proteinG);
    addMinimumConstraint(constraints, variables, "fiberG", targetFiber - baseTotals.fiberG);

    const objective = variables.map(({ food }) => -((selection(food).pricePerKg || food.pricePerKg || 0) / 1000));
    const solution = infeasible.length
      ? { ok: false, error: infeasible.join("; ") }
      : solveLinearProgram(objective, constraints);
    if (!solution.ok) {
      setStatus("No feasible simplex solution: " + solution.error);
      return baseAllocations.length ? baseAllocations : defaultAllocation(foods);
    }

    const optimized = [...baseAllocations];
    solution.values.forEach((grams, index) => {
      if (grams > 0.01) {
        optimized.push(allocationFor(variables[index].food, grams));
      }
    });
    const totals = optimized.reduce((sum, item) => addTotals(sum, item.totals), zeroTotals());
    const misses = [];
    if (totals.kcal + 1 < targetKcal) misses.push("calories");
    if (totals.proteinG + 0.1 < targetProtein) misses.push("protein");
    if (totals.fiberG + 0.1 < targetFiber) misses.push("fiber");
    if (totals.volumeL - 0.01 > maxVolumeL) misses.push("volume");
    if (misses.length) {
      setStatus("Simplex solution violates: " + misses.join(", "));
    } else {
      setStatus("Simplex optimized lowest price");
    }
    return optimized;
  }

  function addMinimumConstraint(constraints, variables, nutrientKey, needed) {
    if (needed <= 0) return;
    constraints.push({
      coeffs: variables.map(({ food }) => (Number(food.nutrients?.[nutrientKey]) || 0) / 100),
      type: ">=",
      b: needed,
    });
  }

  function solveLinearProgram(objective, constraints) {
    const eps = 1e-8;
    const originalCount = objective.length;
    if (!originalCount) {
      return constraints.some((constraint) => constraint.b > eps && constraint.type === ">=")
        ? { ok: false, error: "no variable can satisfy remaining minimums" }
        : { ok: true, values: [] };
    }

    const rows = [];
    const basis = [];
    const artificial = new Set();
    let width = originalCount;

    for (const raw of constraints) {
      let coeffs = raw.coeffs.slice();
      let type = raw.type;
      let b = Number(raw.b) || 0;
      if (b < 0) {
        coeffs = coeffs.map((value) => -value);
        b = -b;
        type = type === "<=" ? ">=" : type === ">=" ? "<=" : "=";
      }
      const row = Array(width).fill(0);
      for (let i = 0; i < coeffs.length; i++) row[i] = Number(coeffs[i]) || 0;
      if (type === "<=") {
        extendRows(rows, 1);
        row.push(1);
        basis.push(width);
        width += 1;
      } else if (type === ">=") {
        extendRows(rows, 2);
        row.push(-1, 1);
        artificial.add(width + 1);
        basis.push(width + 1);
        width += 2;
      } else {
        extendRows(rows, 1);
        row.push(1);
        artificial.add(width);
        basis.push(width);
        width += 1;
      }
      row.length = width;
      row.push(b);
      rows.push(row);
    }

    const phaseObjective = Array(width).fill(0);
    for (const index of artificial) phaseObjective[index] = -1;
    const phase = simplexTableau(rows, basis, phaseObjective, width, eps);
    if (!phase.ok || phase.value < -1e-6) {
      return { ok: false, error: "constraints are infeasible" };
    }

    const fullObjective = Array(width).fill(0);
    for (let i = 0; i < objective.length; i++) fullObjective[i] = objective[i];
    const final = simplexTableau(rows, basis, fullObjective, width, eps, artificial);
    if (!final.ok) return final;
    const values = Array(originalCount).fill(0);
    for (let rowIndex = 0; rowIndex < basis.length; rowIndex++) {
      const variableIndex = basis[rowIndex];
      if (variableIndex < originalCount) {
        values[variableIndex] = rows[rowIndex][width];
      }
    }
    return { ok: true, values, value: final.value };
  }

  function extendRows(rows, amount) {
    for (const row of rows) {
      const rhs = row.pop();
      for (let i = 0; i < amount; i++) row.push(0);
      row.push(rhs);
    }
  }

  function simplexTableau(rows, basis, objective, width, eps, bannedEntering = new Set()) {
    const objectiveRow = Array(width + 1).fill(0);
    for (let i = 0; i < width; i++) objectiveRow[i] = -Number(objective[i] || 0);
    for (let rowIndex = 0; rowIndex < rows.length; rowIndex++) {
      const basic = basis[rowIndex];
      const cost = Number(objective[basic] || 0);
      if (Math.abs(cost) > eps) {
        for (let col = 0; col <= width; col++) {
          objectiveRow[col] += cost * rows[rowIndex][col];
        }
      }
    }

    for (let iter = 0; iter < 2000; iter++) {
      let entering = -1;
      let mostNegative = -eps;
      for (let col = 0; col < width; col++) {
        if (bannedEntering.has(col)) continue;
        if (objectiveRow[col] < mostNegative) {
          mostNegative = objectiveRow[col];
          entering = col;
        }
      }
      if (entering < 0) {
        return { ok: true, value: objectiveRow[width] };
      }

      let leaving = -1;
      let bestRatio = Number.POSITIVE_INFINITY;
      for (let rowIndex = 0; rowIndex < rows.length; rowIndex++) {
        const coefficient = rows[rowIndex][entering];
        if (coefficient <= eps) continue;
        const ratio = rows[rowIndex][width] / coefficient;
        if (ratio < bestRatio - eps) {
          bestRatio = ratio;
          leaving = rowIndex;
        }
      }
      if (leaving < 0) {
        return { ok: false, error: "objective is unbounded" };
      }
      pivot(rows, objectiveRow, basis, leaving, entering, width);
    }
    return { ok: false, error: "simplex iteration limit reached" };
  }

  function pivot(rows, objectiveRow, basis, leaving, entering, width) {
    const row = rows[leaving];
    const divisor = row[entering];
    for (let col = 0; col <= width; col++) row[col] /= divisor;
    for (let rowIndex = 0; rowIndex < rows.length; rowIndex++) {
      if (rowIndex === leaving) continue;
      const factor = rows[rowIndex][entering];
      if (!factor) continue;
      for (let col = 0; col <= width; col++) {
        rows[rowIndex][col] -= factor * row[col];
      }
    }
    const objectiveFactor = objectiveRow[entering];
    for (let col = 0; col <= width; col++) {
      objectiveRow[col] -= objectiveFactor * row[col];
    }
    basis[leaving] = entering;
  }

  function selection(food) {
    const current = state.selected[food.id] || {};
    if (!state.selected[food.id]) state.selected[food.id] = current;
    if (!current.pricePerKg) current.pricePerKg = food.pricePerKg;
    return current;
  }

  function allocationFor(food, grams) {
    const pricedFood = { ...food, pricePerKg: selection(food).pricePerKg || food.pricePerKg };
    const totals = nutrientTimes(pricedFood, grams);
    return {
      foodId: food.id,
      name: food.name,
      category: food.category,
      grams: round(grams, 1),
      price: round(totals.price, 2),
      volumeL: round(totals.volumeL, 2),
      totals,
    };
  }

  function clamp(value, min, max) {
    const low = Number(min) || 0;
    const high = Number(max) || Number.POSITIVE_INFINITY;
    return Math.min(high, Math.max(low, value));
  }

  function render() {
    writeInputs();
    renderTabs();
    renderCategories();
    renderFoodList();
    renderGenerated();
  }

  function renderTabs() {
    nodes.tabs.forEach((button) => {
      const active = button.dataset.tab === state.tab;
      button.classList.toggle("isActive", active);
    });
    nodes.nutritionTab.hidden = state.tab !== "nutrition";
    nodes.billsTab.hidden = state.tab !== "bills";
  }

  function renderCategories() {
    if (!nodes.categoryFilter.options.length) {
      for (const category of categories) {
        const option = document.createElement("option");
        option.value = category;
        option.textContent = category.replace("_", " ");
        nodes.categoryFilter.append(option);
      }
    }
  }

  function renderFoodList() {
    const query = nodes.search.value.trim().toLowerCase();
    const category = nodes.categoryFilter.value || "all";
    const visible = allFoods().filter((food) => {
      const matchesCategory = category === "all" || food.category === category;
      const matchesQuery = !query || food.name.toLowerCase().includes(query) || food.category.includes(query);
      return matchesCategory && matchesQuery;
    });
    nodes.foodList.replaceChildren();
    for (const food of visible) {
      const sel = selection(food);
      const item = document.createElement("article");
      item.className = "foodItem";
      item.innerHTML = `
        <div class="foodTop">
          <label class="row"><input class="check" type="checkbox" data-field="enabled" ${sel.enabled ? "checked" : ""}><span class="foodName">${escapeHtml(food.name)}</span></label>
          <button class="button subtle" type="button" data-edit="${escapeHtml(food.id)}">${food.builtin ? "Copy" : "Edit"}</button>
        </div>
        <div class="foodMeta">${escapeHtml(food.category)} - ${Math.round(food.nutrients.kcal || 0)} kcal - ${round(food.nutrients.proteinG || 0, 1)}g protein - ${round(food.nutrients.fiberG || 0, 1)}g fiber - ${round(sel.pricePerKg || food.pricePerKg, 2)}/kg</div>
        <div class="foodControls">
          <label><span>min g</span><input type="number" data-field="minG" value="${sel.minG || 0}" min="0" step="10"></label>
          <label><span>max g</span><input type="number" data-field="maxG" value="${sel.maxG || state.plan.days * 600}" min="0" step="10"></label>
          <label><span>forced g</span><input type="number" data-field="forcedG" value="${sel.forcedG || 0}" min="0" step="10"></label>
          <label><span>price/kg</span><input type="number" data-field="pricePerKg" value="${sel.pricePerKg || food.pricePerKg}" min="0" step=".01"></label>
        </div>`;
      item.addEventListener("input", (event) => {
        const field = event.target?.dataset?.field;
        if (!field) return;
        if (field === "enabled") sel.enabled = event.target.checked;
        else sel[field] = number(event.target.value, field === "maxG" ? 900 : 0);
        saveState();
        renderGenerated();
      });
      item.querySelector("[data-edit]")?.addEventListener("click", () => openAlimentum(food));
      nodes.foodList.append(item);
    }
    nodes.metricFoods.textContent = `${selectedFoods().length} / ${allFoods().length}`;
  }

  function renderGenerated() {
    const totals = state.generated.totals || zeroTotals();
    nodes.metricKcal.textContent = String(Math.round((totals.kcal || 0) / Math.max(1, state.plan.days)));
    nodes.metricPrice.textContent = round(totals.price || 0, 2).toString();
    nodes.metricVolume.textContent = round(totals.volumeL || 0, 2) + " L";
    renderNutrientTotals(totals);
    nodes.marmitaList.replaceChildren();
    const perPot = groupPerPot(state.generated.allocations || []);
    for (let i = 0; i < Math.min(state.plan.pots, perPot.length); i++) {
      const item = document.createElement("article");
      item.className = "marmitaItem";
      item.innerHTML = `<div class="row"><strong>Marmita ${i + 1}</strong><span class="small">${round(perPot[i].volumeL, 2)} L</span></div><div class="small">${perPot[i].lines.map(escapeHtml).join("<br>")}</div>`;
      nodes.marmitaList.append(item);
    }
    nodes.shoppingList.replaceChildren();
    for (const row of state.generated.shopping || []) {
      const item = document.createElement("div");
      item.className = "shoppingItem";
      item.innerHTML = `<div class="row"><strong>${escapeHtml(row.name)}</strong><span>${round(row.price, 2)}</span></div><div class="small">${round(row.grams / 1000, 2)} kg - ${escapeHtml(row.category)}</div>`;
      nodes.shoppingList.append(item);
    }
  }

  function renderNutrientTotals(totals) {
    const days = Math.max(1, state.plan.days);
    const cells = [
      ["kcal/d", Math.round((totals.kcal || 0) / days)],
      ["protein/d", round((totals.proteinG || 0) / days, 1) + "g"],
      ["carb/d", round((totals.carbG || 0) / days, 1) + "g"],
      ["fat/d", round((totals.fatG || 0) / days, 1) + "g"],
      ["fiber/d", round((totals.fiberG || 0) / days, 1) + "g"],
      ["Ca/d", round((totals.calciumMg || 0) / days, 0) + "mg"],
      ["Fe/d", round((totals.ironMg || 0) / days, 1) + "mg"],
      ["Mg/d", round((totals.magnesiumMg || 0) / days, 0) + "mg"],
      ["K/d", round((totals.potassiumMg || 0) / days, 0) + "mg"],
      ["Zn/d", round((totals.zincMg || 0) / days, 1) + "mg"],
      ["Vit C/d", round((totals.vitaminCMg || 0) / days, 0) + "mg"],
      ["B12/d", round((totals.b12Mcg || 0) / days, 1) + "mcg"],
    ];
    nodes.nutrientTotals.replaceChildren();
    for (const [label, value] of cells) {
      const cell = document.createElement("div");
      cell.className = "totalCell";
      cell.innerHTML = `<span>${escapeHtml(label)}</span><strong>${escapeHtml(value)}</strong>`;
      nodes.nutrientTotals.append(cell);
    }
  }

  function groupPerPot(allocations) {
    const pots = Array.from({ length: state.plan.pots }, () => ({ lines: [], volumeL: 0 }));
    allocations.forEach((allocation, index) => {
      const gramsPerPot = allocation.grams / state.plan.pots;
      for (let i = 0; i < pots.length; i++) {
        pots[i].lines.push(`${allocation.name}: ${round(gramsPerPot, 0)}g`);
        pots[i].volumeL += allocation.volumeL / state.plan.pots;
      }
    });
    if (!allocations.length) return [{ lines: ["Generate a plan to allocate foods."], volumeL: 0 }];
    return pots;
  }

  function escapeHtml(value) {
    return String(value ?? "").replace(/[&<>"']/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[char]));
  }

  function setStatus(message) {
    nodes.status.textContent = message;
  }

  function openAlimentum(food = null) {
    const source = food || {};
    nodes.alimentumRecordId.value = source.recordId || "";
    nodes.alimentumExtensionId.value = source.extensionId || "";
    nodes.alimentumName.value = source.builtin ? source.name + " custom" : source.name || "";
    nodes.alimentumCategory.value = source.category || "staples";
    nodes.alimentumPrice.value = source.pricePerKg || 10;
    nodes.alimentumDensity.value = source.densityGPerMl || 0.8;
    nodes.alimentumKcal.value = source.nutrients?.kcal || 100;
    nodes.alimentumProtein.value = source.nutrients?.proteinG || 0;
    nodes.alimentumCarb.value = source.nutrients?.carbG || 0;
    nodes.alimentumFat.value = source.nutrients?.fatG || 0;
    nodes.alimentumFiber.value = source.nutrients?.fiberG || 0;
    nodes.dialog.showModal();
  }

  async function saveAlimentum(event) {
    event.preventDefault();
    const recordId = number(nodes.alimentumRecordId.value, 0);
    const extensionId = number(nodes.alimentumExtensionId.value, 0);
    const food = {
      id: recordId ? "record:" + recordId : "record:pending:" + Date.now(),
      name: nodes.alimentumName.value.trim(),
      category: nodes.alimentumCategory.value.trim() || "staples",
      nova: "minimally_processed",
      densityGPerMl: number(nodes.alimentumDensity.value, 0.8),
      pricePerKg: number(nodes.alimentumPrice.value, 10),
      portionG: 100,
      nutrients: {
        ...zeroNutrients(),
        kcal: number(nodes.alimentumKcal.value, 0),
        proteinG: number(nodes.alimentumProtein.value, 0),
        carbG: number(nodes.alimentumCarb.value, 0),
        fatG: number(nodes.alimentumFat.value, 0),
        fiberG: number(nodes.alimentumFiber.value, 0),
      },
      source: "Custom alimentum record",
      builtin: false,
    };
    const action = recordId && extensionId ? "home-manager-update-alimentum" : "home-manager-create-alimentum";
    const payload = { recordId: recordId || null, extensionId: extensionId || null, food };
    try {
      const response = await fetch(actionUrl(action), {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(payload),
      });
      if (!response.ok) throw new Error(await response.text());
      nodes.dialog.close();
      await loadCustomFoods();
      setStatus("Alimentum saved");
    } catch (error) {
      setStatus("Could not save alimentum: " + String(error?.message || error));
    }
  }

  function zeroNutrients() {
    const totals = zeroTotals();
    delete totals.price;
    delete totals.volumeL;
    return totals;
  }

  function actionUrl(action) {
    return "/host/widgets/" + encodeURIComponent(instanceId) + "/actions/" + encodeURIComponent(action);
  }

  async function loadCustomFoods() {
    try {
      const response = await fetch(actionUrl("home-manager-list-alimenta"), {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: "{}",
      });
      if (!response.ok) throw new Error(await response.text());
      const payload = await response.json();
      customFoods = (payload.items || []).map(parseAlimentumRecord).filter(Boolean);
    } catch (error) {
      customFoods = [];
      setStatus("Custom alimenta unavailable in preview or without host actions");
    }
    render();
  }

  function parseAlimentumRecord(row) {
    const food = row?.extension?.food;
    if (!food || typeof food !== "object") return null;
    return {
      ...food,
      id: "record:" + row.recordId,
      recordId: row.recordId,
      extensionId: row.extensionId,
      builtin: false,
    };
  }

  for (const button of nodes.tabs) {
    button.addEventListener("click", () => {
      state.tab = button.dataset.tab === "bills" ? "bills" : "nutrition";
      saveState();
      render();
    });
  }

  for (const input of [nodes.weight, nodes.height, nodes.age, nodes.sex, nodes.activity, nodes.days, nodes.pots, nodes.potVolume, nodes.meals, nodes.fiber, nodes.protein]) {
    input.addEventListener("input", () => {
      readInputs();
      saveState();
      renderGenerated();
    });
  }
  nodes.search.addEventListener("input", renderFoodList);
  nodes.categoryFilter.addEventListener("change", renderFoodList);
  nodes.generate.addEventListener("click", () => generatePlan(false));
  nodes.optimize.addEventListener("click", () => generatePlan(true));
  nodes.save.addEventListener("click", saveState);
  nodes.newAlimentum.addEventListener("click", () => openAlimentum(null));
  nodes.alimentumForm.addEventListener("submit", saveAlimentum);
  nodes.alimentumSubmit.addEventListener("click", saveAlimentum);

  if (window.LinceWidgetHost?.subscribe) {
    window.LinceWidgetHost.subscribe((detail) => {
      const cardState = detail?.meta?.cardState?.homeManagerNutrition;
      if (cardState && !hostReady && !userTouchedState) {
        state = normalizeState(cardState);
        render();
      }
      hostReady = true;
    });
  }

  render();
  void loadCustomFoods();
})();
"##
    );
    script
}
