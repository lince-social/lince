use engine::record_creation::Assertion;
use serde_json::Value;

fn reference(value: &str) -> Result<String, String> {
    let value = value.trim_start_matches(['#', '@']);
    let quoted = value.starts_with('"');
    let value = if quoted {
        serde_json::from_str::<String>(value).map_err(|_| "Finish the quoted term".to_string())?
    } else {
        value.to_owned()
    };
    if value.is_empty()
        || value.chars().count() > 256
        || value.chars().any(char::is_control)
        || !quoted && value.contains(['#', '@', ':'])
    {
        return Err("Enter a concept or slug after # or @".into());
    }
    Ok(value)
}

fn words(text: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        if ch == '\n' {
            out.push(";".into());
            continue;
        }
        if ch.is_whitespace() {
            continue;
        }
        if ",;:".contains(ch) {
            out.push(ch.to_string());
            continue;
        }
        let quote =
            ch == '"' || matches!(ch, '#' | '@') && chars.peek().is_some_and(|(_, ch)| *ch == '"');
        let mut end = start + ch.len_utf8();
        if quote {
            if ch != '"' {
                end = chars.next().expect("quoted reference").0 + 1;
            }
            let mut escaped = false;
            let mut closed = false;
            for (index, next) in chars.by_ref() {
                end = index + next.len_utf8();
                if next == '"' && !escaped {
                    closed = true;
                    break;
                }
                escaped = next == '\\' && !escaped;
            }
            if !closed {
                return Err("Finish the quoted term".into());
            }
        } else {
            while let Some((index, next)) = chars.peek().copied() {
                if next.is_whitespace() || ",;:".contains(next) {
                    break;
                }
                chars.next();
                end = index + next.len_utf8();
            }
        }
        out.push(text[start..end].into());
    }
    Ok(out)
}

pub(super) fn parse(text: &str) -> Result<Vec<Assertion>, String> {
    let mut assertions = Vec::new();
    let words = words(text)?;
    let mut words = words.iter().map(String::as_str).peekable();
    while let Some(predicate) = words.next() {
        if matches!(predicate, "," | ";") {
            continue;
        }
        if predicate.starts_with(['@', '^']) || predicate == ":" {
            return Err("Start with a concept, for example #depends-on @project".into());
        }
        let mut assertion = Assertion {
            predicate: reference(predicate)?,
            object: None,
            quantity: None,
            unit: None,
        };
        if words.peek().is_some_and(|word| word.starts_with('^')) {
            let uid = words.next().unwrap().trim_start_matches('^');
            if !nucleus::valid_uid(uid, "c") {
                return Err("Choose a valid Concept identity".into());
            }
            assertion.predicate = uid.into();
        }
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
    Ok(assertions)
}

pub(super) fn from_value(value: &Value) -> Assertion {
    let optional = |key| value[key].as_str().map(str::to_owned);
    Assertion {
        predicate: optional("predicate_uid")
            .or_else(|| optional("predicate"))
            .unwrap_or_default(),
        object: optional("object"),
        quantity: optional("quantity"),
        unit: optional("unit"),
    }
}

pub(super) fn format(assertion: &Assertion) -> String {
    let mut text = format!("#{}", term_text(&assertion.predicate));
    if let Some(object) = &assertion.object {
        text.push_str(&format!(" @{object}"));
    }
    if let Some(quantity) = &assertion.quantity {
        text.push_str(&format!(": {quantity}"));
    }
    if let Some(unit) = &assertion.unit {
        text.push_str(&format!(" @{}", term_text(unit)));
    }
    text
}

pub(super) fn term_text(name: &str) -> String {
    if name
        .chars()
        .any(|ch| ch.is_whitespace() || ",;:#@\"^".contains(ch))
    {
        serde_json::to_string(name).expect("term text")
    } else {
        name.into()
    }
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

    #[test]
    fn translated_terms_and_completion_identities_are_preserved() {
        let id = nucleus::vocabulary::TOPOS_UID;
        let text = format!("#\"saved geographic place\" ^{id} @street4, #tópos @street4");
        let assertions = parse(&text).unwrap();
        assert_eq!(assertions[0].predicate, id);
        assert_eq!(assertions[0].object.as_deref(), Some("street4"));
        assert_eq!(assertions[1].predicate, "tópos");
        assert!(parse("#\"unfinished").is_err());
    }
}
