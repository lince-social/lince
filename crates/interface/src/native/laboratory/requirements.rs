use super::Case;
use anicca::requirements::Requirement;
use serde::Serialize;
use std::{collections::BTreeMap, io, path::Path, sync::OnceLock};

const SOURCE_PATH: &str = "institute/trails/lince/Lince.lingua";
const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../institute/trails/lince/Lince.lingua"
));
pub(super) const PROTEIN_FILTER_CASE: &str = "protein_area_filter_visible_and_selectable";
const MAPPINGS: &[(&str, &str, &str)] = &[
    (
        "protein-readable-text",
        "r_04PESBG43ASBNW30QDN8KQPARV",
        PROTEIN_FILTER_CASE,
    ),
    (
        "protein-laboratory",
        "r_04PESBG43ASBNW30QDN8KQPARV",
        PROTEIN_FILTER_CASE,
    ),
    (
        "edit-panel-geometry",
        "r_5FH4XFDNTBBBYKZNZG3GP2M7VM",
        "edit_panel_usable_bounds",
    ),
];

#[derive(Clone, Debug, Serialize)]
pub struct RuleCoverage {
    pub source: &'static str,
    #[serde(flatten)]
    pub requirement: Requirement,
}

pub(super) fn for_cases(cases: &[Case]) -> io::Result<BTreeMap<&'static str, Vec<RuleCoverage>>> {
    static DECLARED: OnceLock<Result<Vec<Requirement>, String>> = OnceLock::new();
    let declared = DECLARED
        .get_or_init(|| {
            anicca::requirements::discover(Path::new(SOURCE_PATH), SOURCE)
                .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(|error| io::Error::other(error.clone()))?;
    match_cases(declared, cases)
}

fn match_cases(
    declared: &[Requirement],
    cases: &[Case],
) -> io::Result<BTreeMap<&'static str, Vec<RuleCoverage>>> {
    let short_name = |case: &Case| case.name.rsplit("::").next().unwrap_or(case.name);
    if !cases
        .first()
        .is_some_and(|case| short_name(case) == PROTEIN_FILTER_CASE)
    {
        return Err(io::Error::other(
            "The mandatory Protein Add filter case must run first",
        ));
    }
    let mut coverage = BTreeMap::<_, Vec<_>>::new();
    for requirement in declared {
        let (_, record, case) = MAPPINGS
            .iter()
            .find(|(id, _, _)| *id == requirement.id.as_str())
            .ok_or_else(|| {
                io::Error::other(format!(
                    "{SOURCE_PATH}:{}: no Laboratory case maps to [{}]",
                    requirement.line, requirement.id
                ))
            })?;
        if *record != requirement.record_uid.as_str() {
            return Err(io::Error::other(format!(
                "{SOURCE_PATH}:{}: [{}] belongs to {}, expected {record}",
                requirement.line, requirement.id, requirement.record_uid
            )));
        }
        let mut matches = cases
            .iter()
            .filter(|candidate| short_name(candidate) == *case);
        let matched = matches
            .next()
            .filter(|_| matches.next().is_none())
            .ok_or_else(|| {
                io::Error::other(format!(
                    "[{}] requires exactly one selected Laboratory case named {case}",
                    requirement.id
                ))
            })?;
        coverage
            .entry(matched.name)
            .or_default()
            .push(RuleCoverage {
                source: SOURCE_PATH,
                requirement: requirement.clone(),
            });
    }
    for (id, _, _) in MAPPINGS {
        if !declared.iter().any(|requirement| requirement.id == *id) {
            return Err(io::Error::other(format!(
                "Stale Laboratory mapping for [{id}] in {SOURCE_PATH}"
            )));
        }
    }
    Ok(coverage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_protein_case_and_edit_geometry_cover_the_bundled_rules() {
        let cases = super::super::interface::fast_cases();
        let covered = for_cases(&cases).unwrap();
        assert_eq!(covered[cases[0].name].len(), 2);
        assert!(
            covered[cases[0].name]
                .iter()
                .any(|coverage| coverage.requirement.id == "protein-laboratory"
                    && coverage.requirement.hidden)
        );
        let edit = cases
            .iter()
            .find(|case| case.name.ends_with("::edit_panel_usable_bounds"))
            .unwrap();
        assert_eq!(covered[edit.name][0].requirement.id, "edit-panel-geometry");
        assert!(for_cases(&[]).is_err());

        let missing_edit: Vec<_> = cases
            .into_iter()
            .filter(|case| !case.name.ends_with("::edit_panel_usable_bounds"))
            .collect();
        assert!(
            for_cases(&missing_edit)
                .unwrap_err()
                .to_string()
                .contains("edit-panel-geometry")
        );
    }

    #[test]
    fn stale_mappings_and_new_unmapped_requirements_are_visible() {
        let cases = super::super::interface::fast_cases();
        let mut declared = anicca::requirements::discover(Path::new(SOURCE_PATH), SOURCE).unwrap();
        let mut missing = declared.pop().unwrap();
        assert!(
            match_cases(&declared, &cases)
                .unwrap_err()
                .to_string()
                .contains("Stale Laboratory mapping")
        );
        missing.id = "new-unmapped-requirement".into();
        declared.push(missing);
        assert!(
            match_cases(&declared, &cases)
                .unwrap_err()
                .to_string()
                .contains("[new-unmapped-requirement]")
        );
    }
}
