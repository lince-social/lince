use sensei::rules::no_comments::findings_in;

fn lines(source: &str) -> Vec<usize> {
    findings_in(source).into_iter().map(|f| f.line).collect()
}

#[test]
fn every_shape_of_comment_is_found_with_its_line() {
    let source = "fn a() {}\n// one\nfn b() {}\n/* two\nstill two */\nfn c() {} /// three\n";
    assert_eq!(lines(source), vec![2, 4, 6]);
}

#[test]
fn a_slash_inside_a_string_is_not_a_comment() {
    let source = r#"
fn a() -> &'static str { "https://lince.social" }
fn b() -> &'static str { "/* not a comment */" }
fn c() -> &'static str { "a \" quote then // still a string" }
"#;
    assert_eq!(lines(source), Vec::<usize>::new());
}

#[test]
fn a_raw_string_holds_its_slashes_however_many_hashes_it_has() {
    let source = "fn a() -> &'static str { r\"http://x\" }\nfn b() -> &'static str { r#\"a \" and // inside\"# }\nfn c() -> &'static [u8] { br##\"//\"## }\n";
    assert_eq!(lines(source), Vec::<usize>::new());
}

#[test]
fn a_lifetime_is_not_an_unterminated_character_literal() {
    let source = "struct A<'a> { value: &'a str }\nimpl<'a> A<'a> { fn n(&self) -> char { '/' } }\n// found\n";
    assert_eq!(lines(source), vec![3]);
}

#[test]
fn an_escaped_quote_in_a_character_literal_does_not_swallow_the_rest() {
    let source = "fn a() -> char { '\\'' }\nfn b() -> char { '\\\\' }\n// found\n";
    assert_eq!(lines(source), vec![3]);
}

#[test]
fn a_nested_block_comment_is_one_finding_and_the_lines_after_it_still_count() {
    let source = "fn a() {}\n/* outer /* inner */ still outer */\n// after\n";
    assert_eq!(lines(source), vec![2, 3]);
}

#[test]
fn a_word_ending_in_r_before_a_string_is_not_a_raw_string() {
    let source = "fn a() { let colour = \"red\"; }\n// found\n";
    assert_eq!(lines(source), vec![2]);
}
