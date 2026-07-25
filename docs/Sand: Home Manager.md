# Home Manager Nutrition

Implementation checklist for the Home Manager nutrition tab.

- [x] Replace the old Home Manager surface with thin top tabs: Nutrition and Bills.
- [x] Keep Bills as a thin placeholder tab for this pass.
- [x] Base nutrition rules on Brazil's current Ministry of Health food guide: prefer in natura and minimally processed foods, use culinary ingredients in small amounts, limit processed foods, avoid ultraprocessed foods.
- [x] Use TBCA-style per-100g food-composition fields for built-in frontend data.
- [x] Embed the initial food knowledge base in the Home Manager sand as JavaScript objects, not Lince tables.
- [x] Include roughly 100 common food objects across fruit, vegetables, legumes, grains, roots, meat, eggs, dairy, nuts, seeds, oils, and Brazilian staples.
- [x] Include calories, macros, fiber, common vitamin fields, common mineral fields, category, NOVA group, density, and generic-currency price fields where known.
- [x] Store unknown custom-food micronutrients as `null`, never silently converting an empty edit field to zero.
- [x] Add a custom alimentum record workflow using `record` plus `record_extension`.
- [x] Use `record_extension.namespace = "nutrition.alimentum.v1"` for custom alimenta.
- [x] Merge built-in foods and custom alimentum records in the frontend catalog.
- [x] Keep marmita plans, optimizer inputs, allocations, prices, and shopping lists as widget/card state.
- [x] Let the user configure weight, height, sex, age, activity factor, days, pot count, pot volume, meals per day, food min/max, and forced grams.
- [x] Generate marmita allocation, nutrient totals, shopping list, total price, and price per marmita.
- [x] Add a lowest-price optimizer using a frontend two-phase simplex linear optimizer with infeasibility reporting.
- [x] Add visual workflow coverage for tab switching, custom alimentum creation, price editing, plan generation, optimizer run, and shopping-list display.

## Storage Architecture

The built-in nutrition knowledge base is package data. It is compiled into the official Home Manager sand and is never written to Lince persistence. This keeps researched base values deterministic, reviewable, and portable with the widget.

Custom foods are the only nutrition records created by this pass. They are stored as `record` rows with a single `record_extension` payload in the `nutrition.alimentum.v1` namespace. The frontend reads those records through dedicated Home Manager widget actions and merges them with the built-in catalog at render time.

Marmita plans are operational state, not records. Profile settings, food selection, min/max/forced grams, prices, generated allocations, optimizer output, and shopping lists are saved in widget/card state through the bridge, with localStorage only as a preview fallback.

## Data Shape

Built-in and custom foods share this object shape:

```json
{
  "id": "builtin:arroz-integral",
  "name": "Arroz integral cozido",
  "category": "cereals",
  "nova": "minimally_processed",
  "densityGPerMl": 0.78,
  "pricePerKg": 7.5,
  "portionG": 100,
  "nutrients": {
    "kcal": 124,
    "proteinG": 2.6,
    "carbG": 25.8,
    "fatG": 1.0,
    "fiberG": 2.7,
    "calciumMg": 5,
    "ironMg": 0.3,
    "magnesiumMg": 43,
      "potassiumMg": 86,
      "zincMg": 0.6,
      "sodiumMg": 1,
      "phosphorusMg": 83,
      "seleniumMcg": 5.1,
      "copperMg": 0.1,
      "manganeseMg": 0.7,
      "vitaminCMg": 0,
      "vitaminAMcg": 0,
      "vitaminDMcg": 0,
      "vitaminEMg": 0.2,
      "vitaminKMcg": 1,
      "thiaminMg": 0.1,
      "riboflavinMg": 0,
      "niacinMg": 1.3,
      "vitaminB6Mg": 0.1,
      "folateMcg": 4,
      "b12Mcg": 0
  },
  "source": "Brazil food-guide category and TBCA-compatible planning value per 100g"
}
```

Custom records store the same object, without the `builtin:` identity, in:

```json
{
  "schema": "nutrition.alimentum.v1",
  "food": { "...": "same shape" }
}
```

## Optimizer

The optimizer minimizes total generic-currency price over food gram variables. It uses a two-phase simplex tableau with these constraints:

- per-food min, max, and forced grams
- total marmita volume from pot count and pot volume
- minimum calories from Mifflin-St Jeor estimated expenditure
- minimum daily protein
- minimum daily fiber

If the LP is infeasible, the UI reports the violated class of constraint and keeps a generated fallback rather than silently producing a broken plan.

## Sources

- Ministry of Health, `Guia Alimentar para a Populacao Brasileira`, 2nd edition, official Gov.br listing updated 2021-07-29: https://www.gov.br/saude/pt-br/assuntos/saude-brasil/publicacoes-para-promocao-a-saude/guia_alimentar_populacao_brasileira_2ed.pdf/view
- Ministry of Health PDF mirror in BVS: https://bvsms.saude.gov.br/bvs/publicacoes/guia_alimentar_populacao_brasileira_2ed.pdf
- TBCA/USP food composition database: https://www.tbca.net.br/

The UI must not claim the current official guide is a food pyramid. It may present a practical hierarchy based on NOVA processing groups. The embedded catalog is a planning database shaped from food-guide categories and TBCA-style per-100g fields; it is not a clinical or labeling-grade copy of TBCA records.
