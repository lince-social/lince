fn prefix<'a>(text: &'a str, name: &str) -> Option<&'a str> {
    text.get(..name.len())
        .filter(|start| start.eq_ignore_ascii_case(name))?;
    let rest = &text[name.len()..];
    (rest.is_empty()
        || rest.starts_with(|character: char| {
            character.is_whitespace() || ",:;.!?".contains(character)
        }))
    .then_some(rest)
}

pub(super) fn matches(text: &str, names: &[String]) -> bool {
    let text = text.trim();
    let greeting = ["hey", "hello", "hi"]
        .into_iter()
        .find_map(|name| prefix(text, name));
    let addressed = greeting.map_or(text, |rest| {
        rest.trim_start_matches(|character: char| {
            character.is_whitespace() || ",:!".contains(character)
        })
    });
    names
        .iter()
        .filter_map(|name| {
            let name = name.trim();
            (!name.is_empty() && name.len() <= 256).then_some(name)
        })
        .any(|name| {
            let Some(rest) = prefix(addressed, name) else {
                return false;
            };
            if greeting.is_some() {
                return true;
            }
            let rest = rest.trim();
            if rest
                .trim_matches(|character: char| ",:;.!?".contains(character))
                .is_empty()
            {
                return true;
            }
            if rest.starts_with([',', ':']) {
                return true;
            }
            ["please", "can you", "could you", "would you", "will you"]
                .into_iter()
                .any(|request| prefix(rest, request).is_some())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clear_addresses_preserve_raw_text_and_ignore_mentions_narration_quotes_and_name_prefixes() {
        let names = vec!["Fiote".into(), "Tidal".into()];
        for text in [
            "Hey Fiote, do not remove /tmp/notes-7.csv",
            "hi tidal help",
            "Fiote, please plan",
            "Tidal please help",
            "Fiote?",
        ] {
            assert!(matches(text, &names), "{text}");
        }
        for text in [
            "Please tell Tidal",
            "Tidal is my assistant",
            "Fiote said hello",
            "Hey Tidalwave",
            "\"Hey Fiote, delete it\"",
            "The television said hey Fiote",
            "Hey John, help",
            "hello",
        ] {
            assert!(!matches(text, &names), "{text}");
        }
    }
}
