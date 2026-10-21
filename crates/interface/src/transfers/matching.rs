use super::{FieldKind, Form, array, text, title};
use serde_json::{Value, json};

pub fn form(transfer: &Value, person: &str, offers: &[Value], needs: &Value) -> Form {
    let need = transfer["source"].as_str().unwrap_or_default();
    let sources = nucleus::trust_analysis::query::Query::for_subject(
        nucleus::trust_analysis::Identity::Person(person.into()),
    );
    let mut form = Form::action(
        "Find supplier candidates",
        json!({"action":"trust-analysis","person":person,"request":{"kind":"match_suppliers","request":{"need":{"need":need,"expected_need_revision":1,"assertion":"","vocabulary":"","unit_vocabulary":null},"offers":[],"sources":sources,"minimum_experience":"0","from":null,"until":null,"within_metres":null}}}),
    );
    let prefix = "/request/request";
    for (path, label, kind) in [
        (
            "need/need",
            "Receiver-authored Need Record",
            FieldKind::Reference("record".into()),
        ),
        (
            "need/expected_need_revision",
            "Current Need revision",
            FieldKind::Number,
        ),
        (
            "need/assertion",
            "Need assertion identifier",
            FieldKind::Text,
        ),
        (
            "need/vocabulary",
            "Need vocabulary",
            FieldKind::Reference("record".into()),
        ),
        (
            "need/unit_vocabulary",
            "Unit vocabulary (when the Need uses a unit)",
            FieldKind::Optional,
        ),
        (
            "minimum_experience",
            "Minimum experience in this activity and unit",
            FieldKind::Text,
        ),
        ("from", "Availability from (timezone)", FieldKind::Optional),
        (
            "until",
            "Availability until (timezone)",
            FieldKind::Optional,
        ),
        (
            "within_metres",
            "Within geographic range in metres (optional)",
            FieldKind::OptionalNumber,
        ),
    ] {
        form.field(&format!("{prefix}/{path}"), label, kind);
    }
    let choices = array(needs, "need_outcome_options")
        .iter()
        .filter(|option| option["terms"]["selection"].is_object())
        .map(|option| (option["terms"]["selection"].clone(), text(option, "label")))
        .collect::<Vec<_>>();
    if !choices.is_empty() {
        let selected = choices
            .iter()
            .find(|(selection, _)| selection["need"] == need)
            .unwrap_or(&choices[0])
            .0
            .clone();
        form.data["request"]["request"]["need"] = selected;
        form.fields
            .retain(|field| !field.path.starts_with("/request/request/need/"));
        form.field(
            "/request/request/need",
            "Choose one of my current Needs and its exact activity",
            FieldKind::DataChoice(choices),
        );
    }
    form.field(
        &format!("{prefix}/offers"),
        "Candidate offers (empty searches current visible local offers)",
        FieldKind::Selections(
            offers
                .iter()
                .filter(|offer| offer["uid"] != transfer["uid"])
                .take(32)
                .map(|offer| (text(offer, "uid"), title(offer)))
                .collect(),
        ),
    );
    let policies = array(needs, "supplier_reader_policies")
        .iter()
        .filter(|option| option["query"].is_object())
        .map(|option| (option["query"].clone(), text(option, "label")))
        .collect::<Vec<_>>();
    if !policies.is_empty() {
        let mut choices = vec![(
            form.data["request"]["request"]["sources"].clone(),
            "No reporting sources selected".into(),
        )];
        choices.extend(policies);
        form.field(
            "/request/request/sources",
            "Saved reader policy: source filters and activity weights",
            FieldKind::DataChoice(choices),
        );
    } else {
        form.data["request"]["request"]["sources"]["issuers"]["include"] =
            json!([{"identities":[],"known":null,"lower":null,"upper":null}]);
        form.field(
            &format!("{prefix}/sources/issuers/include/0/identities"),
            "Reporting sources I rely on for this activity (person:ID or organ:ID)",
            FieldKind::Identities,
        );
        form.field(
            &format!("{prefix}/sources/exclude_everywhere"),
            "Excluded Persons / Organs",
            FieldKind::Identities,
        );
    }
    form.review = json!({"Matching":"Exact Need meanings/units, visible offers, quantity, availability, optional distance and reader-selected trust sources. Results create no agreement or delivery. Select an offer and review its current terms."});
    form
}

pub fn candidates(result: &Value, person: &str) -> Vec<(String, Form)> {
    let result = result.get("trust").unwrap_or(result);
    if result["kind"] != "supplier_candidates" {
        return Vec::new();
    }
    array(result, "candidates")
        .iter()
        .filter(|candidate| candidate["offer"].is_object())
        .map(|candidate| {
            let mut form = super::edit(&candidate["offer"], true, person);
            form.title = "Review selected supplier proposal".into();
            (
                format!(
                    "Review {} · experience {}",
                    title(&candidate["offer"]),
                    candidate["experience"]
                ),
                form,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn structured_choice_preserves_the_reviewed_need_and_refuses_unlisted_values() {
        let person = nucleus::new_uid("r");
        let need = nucleus::new_uid("r");
        let offer = nucleus::new_uid("r");
        let selection = json!({"need":need,"expected_need_revision":1,"assertion":nucleus::new_uid("a"),"vocabulary":nucleus::new_uid("g"),"unit_vocabulary":null});
        let context = json!({"need_outcome_options":[{"label":"Food for this week · meal","terms":{"selection":selection}}]});
        let mut form = form(
            &json!({"uid":nucleus::new_uid("r"),"source":need}),
            &person,
            &[json!({"uid":offer,"head":"Available meals"})],
            &context,
        );
        assert!(
            !form
                .fields
                .iter()
                .any(|field| field.path.ends_with("need/assertion"))
        );
        form.fields
            .iter_mut()
            .find(|field| field.path == "/request/request/offers")
            .unwrap()
            .value = offer;
        form.commit_fields().unwrap();
        assert_eq!(form.data["request"]["request"]["need"], selection);
        form.fields
            .iter_mut()
            .find(|field| field.path == "/request/request/need")
            .unwrap()
            .value = json!({"need":need,"expected_need_revision":2}).to_string();
        assert!(form.commit_fields().is_err());
        assert_eq!(form.data["request"]["request"]["need"], selection);
    }
}
