use engine::record_creation::Assertion;
use serde_json::Value;

fn reference(value: &str) -> Result<String, String> {
    let value = value.trim_start_matches(['#', '@']);
    if value.is_empty() || value.len() > 128 || value.contains(['#', '@', ':']) {
        return Err("Enter a concept or slug after # or @".into());
    }
    Ok(value.into())
}

pub(super) fn parse(text: &str) -> Result<Vec<Assertion>, String> {
    let mut assertions = Vec::new();
    for clause in text.split([',', ';', '\n']) {
        let spaced = clause.replace(':', " : ");
        let mut words = spaced.split_whitespace().peekable();
        while let Some(predicate) = words.next() {
            if predicate.starts_with('@') || predicate == ":" {
                return Err("Start with a concept, for example #depends-on @project".into());
            }
            let mut assertion = Assertion {
                predicate: reference(predicate)?,
                object: None,
                quantity: None,
                unit: None,
            };
            if words.peek().is_some_and(|word| word.starts_with('@')) {
                assertion.object = Some(reference(words.next().unwrap())?);
            }
            if words.peek() == Some(&":") {
                words.next();
                let quantity = words.next().ok_or("Enter a quantity after :")?;
                nucleus::DecimalValue::parse_inferred(quantity)
                    .map_err(|_| "Enter a decimal quantity after :")?;
                assertion.quantity = Some(quantity.into());
                if words.peek().is_some_and(|word| word.starts_with('@')) {
                    assertion.unit = Some(reference(words.next().unwrap())?);
                }
            }
            assertions.push(assertion);
            if assertions.len() > 40 {
                return Err("Add at most 40 assertions at a time".into());
            }
        }
    }
    Ok(assertions)
}

pub(super) fn from_value(value: &Value) -> Assertion {
    let optional = |key| value[key].as_str().map(str::to_owned);
    Assertion {
        predicate: optional("predicate").unwrap_or_default(),
        object: optional("object"),
        quantity: optional("quantity"),
        unit: optional("unit"),
    }
}

pub(super) fn format(assertion: &Assertion) -> String {
    let mut text = format!("#{}", assertion.predicate);
    if let Some(object) = &assertion.object {
        text.push_str(&format!(" @{object}"));
    }
    if let Some(quantity) = &assertion.quantity {
        text.push_str(&format!(": {quantity}"));
    }
    if let Some(unit) = &assertion.unit {
        text.push_str(&format!(" @{unit}"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concepts_links_and_amounts_share_one_input_without_rounding() {
        let assertions =
            parse("#planned #depends-on @project, #cost: 9007199254740993.125 @real").unwrap();
        assert_eq!(assertions.len(), 3);
        assert_eq!(assertions[1].object.as_deref(), Some("project"));
        assert_eq!(
            assertions[2].quantity.as_deref(),
            Some("9007199254740993.125")
        );
        assert_eq!(assertions[2].unit.as_deref(), Some("real"));
        let text = assertions.iter().map(format).collect::<Vec<_>>().join(", ");
        assert_eq!(
            serde_json::to_value(parse(&text).unwrap()).unwrap(),
            serde_json::to_value(assertions).unwrap()
        );
    }

    #[test]
    fn incomplete_and_invalid_entries_are_not_silently_dropped() {
        for text in [
            "#",
            "@project",
            "#cost:",
            "#cost: NaN",
            "#cost: 2 @",
            "#planned @project @other",
        ] {
            assert!(parse(text).is_err(), "{text}");
        }
        assert!(parse(&"#planned ".repeat(41)).is_err());
    }
}
