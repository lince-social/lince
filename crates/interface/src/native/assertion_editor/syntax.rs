use engine::record_creation::Assertion;
use lince_resources::reference::is_uri;
use nucleus::{DecimalValue, valid_uid};
use serde_json::{Value, from_str, to_string};
use std::borrow::Cow;

const MAX_TERM_CHARACTERS: usize = 256;
const MAX_ADDRESS_BYTES: usize = 8192;
const MAX_ASSERTIONS: usize = 40;

fn reference(value: &str) -> Result<String, &'static str> {
    let value = value.trim_start_matches(['#', '@']);
    let quoted = value.starts_with('"');
    let value = if quoted {
        Cow::Owned(from_str(value).map_err(|_| "Finish the quoted term")?)
    } else {
        Cow::Borrowed(value)
    };
    if value.is_empty()
        || value.chars().count() > MAX_TERM_CHARACTERS
        || value.chars().any(char::is_control)
        || !quoted && value.contains(['#', '@', ':'])
    {
        return Err("Enter a concept or slug after # or @");
    }
    Ok(value.into_owned())
}

fn words(text: &str) -> Result<Vec<&str>, &'static str> {
    let mut words = Vec::new();
    let mut chars = text.char_indices().peekable();
    while let Some((start, ch)) = chars.next() {
        if ch == '\n' {
            words.push(";");
            continue;
        }
        if ch.is_whitespace() {
            continue;
        }
        if ch == '"' || matches!(ch, '#' | '@') && chars.next_if(|(_, next)| *next == '"').is_some()
        {
            let mut escaped = false;
            if !chars.by_ref().any(|(_, next)| {
                if next == '"' && !escaped {
                    return true;
                }
                escaped = next == '\\' && !escaped;
                false
            }) {
                return Err("Finish the quoted term");
            }
        } else if !",;:".contains(ch) {
            while chars
                .next_if(|(_, next)| !next.is_whitespace() && !",;:".contains(*next))
                .is_some()
            {}
        }
        let end = chars.peek().map_or(text.len(), |(end, _)| *end);
        words.push(
            text.get(start..end)
                .expect("token bounds come from char_indices"),
        );
    }
    Ok(words)
}

pub(super) fn parse(text: &str) -> Result<Vec<Assertion>, String> {
    let mut assertions = Vec::new();
    let mut words = words(text)?.into_iter().peekable();
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
        if let Some(uid) = words.next_if(|word| word.starts_with('^')) {
            let uid = uid.trim_start_matches('^');
            if !valid_uid(uid, "c") {
                return Err("Choose a valid Concept identity".into());
            }
            assertion.predicate = uid.into();
        }
        if let Some(uri) = words.next_if(|word| word.starts_with('"')) {
            let uri: String = from_str(uri).map_err(|_| "Finish the quoted reference")?;
            if uri.is_empty() || uri.len() > MAX_ADDRESS_BYTES || uri.chars().any(char::is_control)
            {
                return Err("Check the reference address".into());
            }
            assertion.object = Some(uri);
        } else if let Some(object) = words.next_if(|word| word.starts_with('@')) {
            assertion.object = Some(reference(object)?);
        }
        if words.next_if_eq(&":").is_some() {
            let quantity = words.next().ok_or("Enter a quantity after :")?;
            DecimalValue::parse_inferred(quantity)
                .map_err(|_| "Enter a decimal quantity after :")?;
            assertion.quantity = Some(quantity.into());
            if let Some(unit) = words.next_if(|word| word.starts_with('@')) {
                assertion.unit = Some(reference(unit)?);
            }
        }
        assertions.push(assertion);
        if assertions.len() > MAX_ASSERTIONS {
            return Err(format!("Add at most {MAX_ASSERTIONS} assertions at a time"));
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
        object: optional("uri").or_else(|| optional("object")),
        quantity: optional("quantity"),
        unit: optional("unit"),
    }
}

pub(super) fn format(assertion: &Assertion) -> String {
    format_parts(
        &assertion.predicate,
        assertion.object.as_deref(),
        assertion.quantity.as_deref(),
        assertion.unit.as_deref(),
    )
}

pub(super) fn format_parts(
    predicate: &str,
    object: Option<&str>,
    quantity: Option<&str>,
    unit: Option<&str>,
) -> String {
    let mut text = String::from("#");
    text.push_str(&term_text(predicate));
    if let Some(object) = object {
        text.push(' ');
        if is_uri(object) || object.contains('/') {
            text.push_str(&quote(object));
        } else {
            text.push('@');
            text.push_str(object);
        }
    }
    if let Some(quantity) = quantity {
        text.push_str(": ");
        text.push_str(quantity);
    }
    if let Some(unit) = unit {
        text.push_str(" @");
        text.push_str(&term_text(unit));
    }
    text
}

fn quote(text: &str) -> String {
    to_string(text).expect("JSON string serialization is infallible")
}

pub(super) fn term_text(name: &str) -> Cow<'_, str> {
    if name
        .chars()
        .any(|ch| ch.is_whitespace() || ",;:#@\"^".contains(ch))
    {
        Cow::Owned(quote(name))
    } else {
        Cow::Borrowed(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concepts_links_and_amounts_share_one_input_without_rounding() {
        let assertions = parse("#planned #depends-on @project, #cost: 9007199254740993.125 @real")
            .expect("mixed draft assertions parse");
        let [_, dependency, cost] = assertions.as_slice() else {
            panic!("expected three assertions: {assertions:?}");
        };
        assert_eq!(dependency.object.as_deref(), Some("project"));
        assert_eq!(cost.quantity.as_deref(), Some("9007199254740993.125"));
        assert_eq!(cost.unit.as_deref(), Some("real"));
        let text = assertions.iter().map(format).collect::<Vec<_>>().join(", ");
        assert_eq!(
            parse(&text).expect("formatted assertions parse"),
            assertions
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
        let assertions = parse(&text).expect("Unicode terms and completion identity parse");
        let [saved, translated] = assertions.as_slice() else {
            panic!("expected two assertions: {assertions:?}");
        };
        assert_eq!(saved.predicate, id);
        assert_eq!(saved.object.as_deref(), Some("street4"));
        assert_eq!(translated.predicate, "tópos");
        assert!(parse("#\"unfinished").is_err());
    }
}
