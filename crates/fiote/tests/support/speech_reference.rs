pub use fiote::speech::evaluation::words;

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
