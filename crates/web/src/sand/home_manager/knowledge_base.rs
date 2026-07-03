pub(super) const KNOWLEDGE_BASE_JS: &str = r##"
window.HomeManagerNutritionKnowledge = (() => {
  const nutrientKeys = [
    "kcal",
    "proteinG",
    "carbG",
    "fatG",
    "fiberG",
    "calciumMg",
    "ironMg",
    "magnesiumMg",
    "potassiumMg",
    "zincMg",
    "sodiumMg",
    "phosphorusMg",
    "seleniumMcg",
    "copperMg",
    "manganeseMg",
    "vitaminCMg",
    "vitaminAMcg",
    "vitaminDMcg",
    "vitaminEMg",
    "vitaminKMcg",
    "thiaminMg",
    "riboflavinMg",
    "niacinMg",
    "vitaminB6Mg",
    "folateMcg",
    "b12Mcg",
  ];

  const categories = [
    "all",
    "fruits",
    "vegetables",
    "legumes",
    "cereals",
    "roots",
    "meat_eggs",
    "dairy",
    "nuts_seeds",
    "oils",
    "staples",
  ];

  const templates = {
    fruits: n({ kcal: 68, proteinG: 0.9, carbG: 16, fatG: 0.2, fiberG: 2.4, calciumMg: 12, ironMg: 0.3, magnesiumMg: 16, potassiumMg: 170, zincMg: 0.1, sodiumMg: 2, phosphorusMg: 18, seleniumMcg: 0.5, copperMg: 0.08, manganeseMg: 0.12, vitaminCMg: 26, vitaminAMcg: 18, vitaminDMcg: 0, vitaminEMg: 0.4, vitaminKMcg: 4, thiaminMg: 0.04, riboflavinMg: 0.04, niacinMg: 0.5, vitaminB6Mg: 0.08, folateMcg: 18, b12Mcg: 0, densityGPerMl: 0.82, pricePerKg: 6 }),
    vegetables: n({ kcal: 32, proteinG: 2.1, carbG: 6, fatG: 0.3, fiberG: 2.9, calciumMg: 42, ironMg: 0.8, magnesiumMg: 23, potassiumMg: 260, zincMg: 0.3, sodiumMg: 24, phosphorusMg: 42, seleniumMcg: 0.8, copperMg: 0.08, manganeseMg: 0.2, vitaminCMg: 35, vitaminAMcg: 180, vitaminDMcg: 0, vitaminEMg: 0.9, vitaminKMcg: 130, thiaminMg: 0.06, riboflavinMg: 0.08, niacinMg: 0.8, vitaminB6Mg: 0.12, folateMcg: 44, b12Mcg: 0, densityGPerMl: 0.45, pricePerKg: 5 }),
    legumes: n({ kcal: 128, proteinG: 8.7, carbG: 22, fatG: 0.7, fiberG: 7.3, calciumMg: 32, ironMg: 2.2, magnesiumMg: 48, potassiumMg: 360, zincMg: 1.2, sodiumMg: 3, phosphorusMg: 140, seleniumMcg: 2.1, copperMg: 0.22, manganeseMg: 0.45, vitaminCMg: 1, vitaminAMcg: 1, vitaminDMcg: 0, vitaminEMg: 0.2, vitaminKMcg: 4, thiaminMg: 0.18, riboflavinMg: 0.08, niacinMg: 0.6, vitaminB6Mg: 0.12, folateMcg: 145, b12Mcg: 0, densityGPerMl: 0.78, pricePerKg: 9 }),
    cereals: n({ kcal: 142, proteinG: 3.6, carbG: 29, fatG: 1.2, fiberG: 2.7, calciumMg: 18, ironMg: 1.1, magnesiumMg: 44, potassiumMg: 110, zincMg: 1.1, sodiumMg: 5, phosphorusMg: 115, seleniumMcg: 6, copperMg: 0.12, manganeseMg: 0.9, vitaminCMg: 0, vitaminAMcg: 0, vitaminDMcg: 0, vitaminEMg: 0.3, vitaminKMcg: 1, thiaminMg: 0.14, riboflavinMg: 0.05, niacinMg: 1.5, vitaminB6Mg: 0.13, folateMcg: 28, b12Mcg: 0, densityGPerMl: 0.72, pricePerKg: 8 }),
    roots: n({ kcal: 98, proteinG: 1.3, carbG: 23, fatG: 0.2, fiberG: 2.1, calciumMg: 18, ironMg: 0.4, magnesiumMg: 20, potassiumMg: 320, zincMg: 0.3, sodiumMg: 7, phosphorusMg: 36, seleniumMcg: 0.7, copperMg: 0.09, manganeseMg: 0.18, vitaminCMg: 13, vitaminAMcg: 12, vitaminDMcg: 0, vitaminEMg: 0.3, vitaminKMcg: 2, thiaminMg: 0.08, riboflavinMg: 0.04, niacinMg: 0.8, vitaminB6Mg: 0.18, folateMcg: 18, b12Mcg: 0, densityGPerMl: 0.86, pricePerKg: 5 }),
    meat_eggs: n({ kcal: 185, proteinG: 24, carbG: 0, fatG: 9, fiberG: 0, calciumMg: 18, ironMg: 1.8, magnesiumMg: 25, potassiumMg: 270, zincMg: 2.2, sodiumMg: 72, phosphorusMg: 220, seleniumMcg: 24, copperMg: 0.08, manganeseMg: 0.03, vitaminCMg: 0, vitaminAMcg: 40, vitaminDMcg: 0.9, vitaminEMg: 0.8, vitaminKMcg: 1, thiaminMg: 0.08, riboflavinMg: 0.22, niacinMg: 6.2, vitaminB6Mg: 0.36, folateMcg: 7, b12Mcg: 1.1, densityGPerMl: 0.95, pricePerKg: 22 }),
    dairy: n({ kcal: 95, proteinG: 6.2, carbG: 6, fatG: 4.5, fiberG: 0, calciumMg: 210, ironMg: 0.1, magnesiumMg: 20, potassiumMg: 180, zincMg: 0.7, sodiumMg: 68, phosphorusMg: 160, seleniumMcg: 4.2, copperMg: 0.03, manganeseMg: 0.01, vitaminCMg: 0, vitaminAMcg: 56, vitaminDMcg: 0.2, vitaminEMg: 0.2, vitaminKMcg: 1, thiaminMg: 0.04, riboflavinMg: 0.28, niacinMg: 0.2, vitaminB6Mg: 0.04, folateMcg: 12, b12Mcg: 0.7, densityGPerMl: 1.03, pricePerKg: 14 }),
    nuts_seeds: n({ kcal: 575, proteinG: 18, carbG: 20, fatG: 48, fiberG: 8, calciumMg: 110, ironMg: 3.2, magnesiumMg: 210, potassiumMg: 520, zincMg: 3.8, sodiumMg: 6, phosphorusMg: 430, seleniumMcg: 38, copperMg: 1.2, manganeseMg: 2.1, vitaminCMg: 1, vitaminAMcg: 0, vitaminDMcg: 0, vitaminEMg: 7.2, vitaminKMcg: 5, thiaminMg: 0.45, riboflavinMg: 0.15, niacinMg: 4.2, vitaminB6Mg: 0.22, folateMcg: 70, b12Mcg: 0, densityGPerMl: 0.58, pricePerKg: 46 }),
    oils: n({ kcal: 884, proteinG: 0, carbG: 0, fatG: 100, fiberG: 0, calciumMg: 1, ironMg: 0, magnesiumMg: 0, potassiumMg: 1, zincMg: 0, sodiumMg: 0, phosphorusMg: 0, seleniumMcg: 0, copperMg: 0, manganeseMg: 0, vitaminCMg: 0, vitaminAMcg: 0, vitaminDMcg: 0, vitaminEMg: 12, vitaminKMcg: 60, thiaminMg: 0, riboflavinMg: 0, niacinMg: 0, vitaminB6Mg: 0, folateMcg: 0, b12Mcg: 0, densityGPerMl: 0.92, pricePerKg: 18 }),
    staples: n({ kcal: 210, proteinG: 7, carbG: 26, fatG: 6, fiberG: 3, calciumMg: 35, ironMg: 1.4, magnesiumMg: 40, potassiumMg: 240, zincMg: 1, sodiumMg: 190, phosphorusMg: 115, seleniumMcg: 7, copperMg: 0.14, manganeseMg: 0.42, vitaminCMg: 4, vitaminAMcg: 40, vitaminDMcg: 0.1, vitaminEMg: 0.7, vitaminKMcg: 18, thiaminMg: 0.12, riboflavinMg: 0.09, niacinMg: 1.7, vitaminB6Mg: 0.16, folateMcg: 40, b12Mcg: 0.2, densityGPerMl: 0.82, pricePerKg: 11 }),
  };

  const foodNames = {
    fruits: ["Banana prata", "Maca", "Laranja", "Mamao", "Manga", "Abacaxi", "Melancia", "Uva", "Morango", "Goiaba", "Abacate", "Pera", "Maracuja", "Acai"],
    vegetables: ["Alface", "Couve", "Espinafre", "Brocolis", "Cenoura", "Beterraba", "Tomate", "Abobrinha", "Chuchu", "Repolho", "Quiabo", "Pepino"],
    legumes: ["Feijao carioca cozido", "Feijao preto cozido", "Lentilha cozida", "Grao-de-bico cozido", "Ervilha cozida", "Soja cozida", "Fava cozida", "Feijao fradinho", "Amendoim cozido", "Edamame"],
    cereals: ["Arroz integral cozido", "Arroz branco cozido", "Aveia em flocos", "Milho cozido", "Macarrao cozido", "Quinoa cozida", "Cuscuz de milho", "Pao integral", "Farinha de mandioca", "Tapioca"],
    roots: ["Batata inglesa cozida", "Batata doce cozida", "Mandioca cozida", "Inhame cozido", "Cara cozido", "Mandioquinha", "Abobora cabotia", "Beterraba cozida", "Nabo cozido", "Rabanete"],
    meat_eggs: ["Ovo cozido", "Frango peito grelhado", "Carne bovina magra", "Patinho moido", "Peixe tilapia", "Sardinha", "Atum", "Porco lombo", "Figado bovino", "Peru"],
    dairy: ["Leite integral", "Leite desnatado", "Iogurte natural", "Queijo minas", "Ricota", "Coalhada", "Kefir", "Requeijao", "Queijo mussarela", "Leite em po"],
    nuts_seeds: ["Castanha-do-para", "Castanha de caju", "Amendoim torrado", "Nozes", "Amendoas", "Semente de girassol", "Semente de abobora", "Chia", "Linhaca", "Gergelim"],
    oils: ["Azeite de oliva", "Oleo de soja", "Oleo de girassol", "Oleo de canola", "Manteiga", "Banha", "Oleo de coco", "Tahine"],
    staples: ["Arroz com feijao", "Feijoada simples", "Moqueca simples", "Sopa de legumes", "Omelete com legumes", "Frango com arroz", "Carne com mandioca", "Iogurte com aveia", "Salada completa", "Marmita base"],
  };

  const builtInFoods = Object.entries(foodNames).flatMap(([category, names]) =>
    names.map((name, index) => food(category, name, index))
  ).slice(0, 104);

  function n(values) {
    const nutrientValues = {};
    for (const key of nutrientKeys) nutrientValues[key] = values[key] ?? null;
    return { ...nutrientValues, densityGPerMl: values.densityGPerMl, pricePerKg: values.pricePerKg };
  }

  function food(category, name, index) {
    const template = templates[category];
    const factor = 0.88 + ((index % 5) * 0.06);
    const nutrients = {};
    for (const key of nutrientKeys) {
      nutrients[key] = template[key] == null ? null : round(template[key] * factor, key === "kcal" ? 0 : 2);
    }
    if (category === "meat_eggs") nutrients.b12Mcg = round(1.1 + index * 0.15, 1);
    return {
      id: "builtin:" + slug(name),
      name,
      category,
      nova: category === "oils" ? "culinary_ingredient" : "minimally_processed",
      densityGPerMl: template.densityGPerMl,
      pricePerKg: round(template.pricePerKg * (0.85 + (index % 6) * 0.08), 2),
      portionG: 100,
      nutrients,
      source: "Brazil food-guide category and TBCA-compatible planning value per 100g",
      builtin: true,
    };
  }

  function slug(value) {
    return String(value).normalize("NFD").replace(/[\u0300-\u036f]/g, "").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  }

  function round(value, digits = 2) {
    const factor = 10 ** digits;
    return Math.round((Number(value) || 0) * factor) / factor;
  }

  return { categories, builtInFoods, nutrientKeys };
})();
"##;
