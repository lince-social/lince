pub fn words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut numeric = None;
    let finish = |word: &mut String, words: &mut Vec<String>| {
        if word.is_empty() {
            return;
        }
        let normalized = match word.as_str() {
            "zero" => "0",
            "one" => "1",
            "two" => "2",
            "three" => "3",
            "four" => "4",
            "five" => "5",
            "six" => "6",
            "seven" => "7",
            "eight" => "8",
            "nine" => "9",
            "ten" => "10",
            "mister" => "mr",
            other => other,
        };
        words.push(normalized.to_owned());
        word.clear();
    };
    for character in text.to_lowercase().chars() {
        if !character.is_alphanumeric() {
            finish(&mut word, &mut words);
            numeric = None;
            continue;
        }
        let digit = character.is_ascii_digit();
        if numeric.is_some_and(|previous| previous != digit) {
            finish(&mut word, &mut words);
        }
        numeric = Some(digit);
        word.push(character);
    }
    finish(&mut word, &mut words);
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatting_equivalence_preserves_values_order_negation_and_extra_words() {
        let expected = words("go forward ten meters");
        assert_eq!(words("Go forward10 meters."), expected);
        for wrong in [
            "go forward eleven meters",
            "go forward 11 meters",
            "out go forward10 meters",
            "do not go forward ten meters",
            "ten meters go forward",
        ] {
            assert_ne!(words(wrong), expected, "{wrong}");
        }
        assert_ne!(words("eight four seven"), words("eight seven four"));
        assert_ne!(words("zero nine"), words("09"));
    }
}
