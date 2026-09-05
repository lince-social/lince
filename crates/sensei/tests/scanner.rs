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

use sensei::rules::no_comments::strip;

#[test]
fn a_comment_on_its_own_line_leaves_no_blank_behind() {
    let source = "fn a() {}\n// gone\nfn b() {}\n";
    assert_eq!(strip(source), "fn a() {}\nfn b() {}\n");
}

#[test]
fn a_trailing_comment_leaves_the_code_and_no_stray_space() {
    let source = "let x = 1; // why\nlet y = 2;\n";
    assert_eq!(strip(source), "let x = 1;\nlet y = 2;\n");
}

#[test]
fn a_block_comment_across_lines_takes_all_of_its_lines() {
    let source = "fn a() {}\n/* one\n   two */\nfn b() {}\n";
    assert_eq!(strip(source), "fn a() {}\nfn b() {}\n");
}

#[test]
fn a_padded_comment_does_not_leave_two_blank_lines_where_there_was_one() {
    let source = "fn a() {}\n\n// gone\n\nfn b() {}\n";
    assert_eq!(strip(source), "fn a() {}\n\nfn b() {}\n");
}

#[test]
fn a_doc_comment_at_the_top_does_not_leave_the_file_starting_blank() {
    let source = "//! the module\n//! keeps talking\n\nfn a() {}\n";
    assert_eq!(strip(source), "fn a() {}\n");
}

#[test]
fn a_string_that_looks_like_a_comment_survives_untouched() {
    let source = "fn a() -> &'static str { \"https://lince.social // not a comment\" }\n";
    assert_eq!(strip(source), source);
}

#[test]
fn a_raw_string_with_slashes_survives_untouched() {
    let source = "fn a() -> &'static str { r#\"a \" and // inside\"# }\n";
    assert_eq!(strip(source), source);
}

#[test]
fn a_file_with_nothing_to_say_is_returned_byte_for_byte() {
    let source = "fn a() {}\n\nfn b() {}\n";
    assert_eq!(strip(source), source);
}

#[test]
fn multibyte_text_beside_a_comment_is_not_mangled() {
    let source = "let s = \"café — ação\"; // gone\nlet t = \"日本\";\n";
    assert_eq!(
        strip(source),
        "let s = \"café — ação\";\nlet t = \"日本\";\n"
    );
}

#[test]
fn stripping_twice_changes_nothing_the_second_time() {
    let source = "//! head\n\nfn a() {} // tail\n\n/* block */\nfn b() {}\n";
    let once = strip(source);
    assert_eq!(strip(&once), once);
}
