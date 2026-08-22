A Trail is not a new storage mechanism, and not a new concept in Lince's model. It is a name for a *purpose*, not a thing: a bundle of ordinary table rows — Records, Assertions, Concepts, Transfers, Karma rules, referencing each other however the activity they describe requires — that someone can import as one unit because together they happen to be a trail for doing X. Calling a bundle a Trail, or DNA, is a convention over data already fully described by this document (§1-§7), the same way "tag" and "link" are interface words over Assertion rather than separate models.

- A Trail lives in an Organ the way any Records do. An Organ that specializes in keeping Trails well-formed and current is an Alexandria Organ — curated the way the historical library was, by people who care for it, not by a ruling schema.
- **Importing a Trail is copying its rows into your own Organ's data.** There is no package it stays wrapped in and no lineage it keeps once imported — the moment it lands, it is simply more of your data, exactly as if you had entered it yourself. There is nothing to unpack: the referencing rows already form whatever structure the Trail needs, the same way any other set of Records, Assertions, and Concepts does.
- **Studying** a Trail is reading it — its Records and their bodies are the documentation, nothing separate to render.
- **Doing what it says** is Karma reading the imported data exactly like any other data: a Rule's Condition matches against the newly-arrived Records the same way it matches anything else, and its Consequence acts on them — changes a Record, schedules a Transfer — the same way it acts on data a person entered by hand. Nothing about "this came from a Trail" is special to Karma; the import is what was special, execution is ordinary.
- **Changing it** is ordinary editing. Once imported, a Trail's Records are your data, not a tracked copy of someone else's — there is no fork-with-provenance step to perform, because there is no package boundary left to fork away from. Where a piece of your data originally came from is the same open question any other imported or adopted content already has, not something Trails need to solve specially.

### Trails that need more than one Organ

Some Trails cannot be enacted by one Organ alone. Ride-sharing is the clean example: the DNA of "a ride happened" needs a Record in the driver's Organ and a Record in the rider's Organ, because the Transfer that is the actual ride is a condition/need match between the two — each side brings their own half. Today, making that happen requires a person to manually recreate the right Records in each Organ by hand before the Transfer that connects them can exist.

**This is tree sync, and it is no longer specified here.** Resolved
2026-08-09: the primitive this section asked for — taking a selection of
one's data and placing it, by copy or by move, in a chosen Organ — is §11
"Tree sync: syncing from a root Record, and the Move verb". A Trail naming
its cross-Organ half names a root Record and a Protein; the placing is an
ordinary consented sync of that tree. Nothing Trail-specific is needed, and
specifying it twice is how the two definitions drift apart. What remains true
and belongs here is only the line below.
- This stays a thin, explicit act — a Person choosing to place specific data in a specific Organ — never an automatic replication triggered by adopting a Trail alone; adoption is data at rest, placement is a deliberate step on top of it.

Below are earlier open cases for the Alexandria abstraction generally, kept for the questions they still ask rather than answers already found: knowledge can be inbued into components, when it is activated it creates Records with that content. Or maybe knowledge can be data in one specific server, but then you would need to contact such server to access it, if it's gated you loose access. Would it be best if it where inside a binary, inside a sand, in seed? We must find out which one is best, and support ourselves with past work, that made available to all a vast amount of knowledge in the internet, free, maybe we can import it, integrate with it, to jumpstart Alexandria.

## Nutrition - Home Manager.

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

### Optimizer

The optimizer minimizes total generic-currency price over food gram variables. It uses a two-phase simplex tableau with these constraints:

- per-food min, max, and forced grams
- total marmita volume from pot count and pot volume
- minimum calories from Mifflin-St Jeor estimated expenditure
- minimum daily protein
- minimum daily fiber

If the LP is infeasible, the UI reports the violated class of constraint and keeps a generated fallback rather than silently producing a broken plan.

### Sources

- Ministry of Health, `Guia Alimentar para a Populacao Brasileira`, 2nd edition, official Gov.br listing updated 2021-07-29: https://www.gov.br/saude/pt-br/assuntos/saude-brasil/publicacoes-para-promocao-a-saude/guia_alimentar_populacao_brasileira_2ed.pdf/view
- Ministry of Health PDF mirror in BVS: https://bvsms.saude.gov.br/bvs/publicacoes/guia_alimentar_populacao_brasileira_2ed.pdf
- TBCA/USP food composition database: https://www.tbca.net.br/

The UI must not claim the current official guide is a food pyramid. It may present a practical hierarchy based on NOVA processing groups. The embedded catalog is a planning database shaped from food-guide categories and TBCA-style per-100g fields; it is not a clinical or labeling-grade copy of TBCA records.

## What is left

### Trail — what is left

#### Tree sync: taking a Record and everything under it

#### Other subjects — listed, not scheduled

**Open work is not listed here.** Every task lives in one place — [[Ontology|r_S8PQ17MQ3WBWM53ZN700K89V9F]], under "What we work on next" and "-- Not Planned For Now --". This file is the specification; that list is the plan.
