use std::collections::BTreeSet;

fn publication(source: &str) -> anicca::ProjectedDocument {
    anicca::project(&anicca::parse(source).unwrap()).unwrap()
}

#[test]
fn adult_reference_cases_are_unambiguous_and_scoped() {
    let doc = publication(include_str!(
        "../../../institute/trails/health/Nutrition-needs.lingua"
    ));
    let references: Vec<_> = doc
        .extensions
        .iter()
        .filter(|e| e.namespace == "demeter.reference")
        .collect();
    assert!(references.len() >= 35);
    for reference in &references {
        let data = &reference.fields;
        assert!(data["source_url"].as_str().unwrap().starts_with("https://"));
        assert!(["g", "mg", "ug", "kcal"].contains(&data["unit"].as_str().unwrap()));
        for sex in ["male", "female"] {
            for age in 19..=120 {
                let count = data["cases"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|case| {
                        case["sex"] == sex
                            && case["age_min"].as_u64().unwrap() <= age
                            && case["age_max"].as_u64().unwrap() >= age
                    })
                    .count();
                assert_eq!(count, 1, "{sex} {age}: {}", reference.target_slug);
            }
        }
    }
    let magnesium = references
        .iter()
        .find(|r| r.fields["nutrient"] == "magnesium_mg")
        .unwrap();
    assert!(magnesium.fields["cases"][0].get("upper").is_none());
    let supplemental = references
        .iter()
        .find(|r| r.fields["nutrient"] == "magnesium_supplemental_mg")
        .unwrap();
    assert_eq!(supplemental.fields["scope"], "supplemental");
    let folic = references
        .iter()
        .find(|r| r.fields["nutrient"] == "folic_acid_ug")
        .unwrap();
    assert_eq!(folic.fields["scope"], "fortified_or_supplemental");
}

#[test]
fn production_examples_link_real_species_and_require_confirmation() {
    let species: BTreeSet<_> = [
        include_str!("../../../institute/trails/health/farming/Crops.lingua"),
        include_str!("../../../institute/trails/health/farming/Orchards.lingua"),
        include_str!("../../../institute/trails/health/farming/Livestock.lingua"),
    ]
    .into_iter()
    .flat_map(|s| publication(s).records)
    .filter_map(|r| r.slug)
    .collect();
    let production = publication(include_str!(
        "../../../institute/trails/health/farming/Production.lingua"
    ));
    let templates: Vec<_> = production
        .extensions
        .iter()
        .filter(|e| e.namespace == "demeter.production")
        .collect();
    assert_eq!(templates.len(), 6);
    for template in templates {
        let data = &template.fields;
        assert!(species.contains(data["species"].as_str().unwrap()));
        assert_eq!(data["requires_confirmation"], true);
        assert!(
            data["usable_g_per_harvest"]
                .as_str()
                .unwrap()
                .parse::<f64>()
                .unwrap()
                > 0.0
        );
        assert!(!data["stages"].as_array().unwrap().is_empty());
        assert!(!data["inputs"].as_array().unwrap().is_empty());
    }
    let meal = publication(include_str!(
        "../../../institute/trails/health/Meal-preparation.lingua"
    ));
    assert_eq!(
        meal.extensions[0].fields["dependencies"]
            .as_array()
            .unwrap()
            .len(),
        4
    );
}
