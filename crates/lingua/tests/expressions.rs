// non-lean-yet
// Scope: fn scripts_and_declarations_format_without_execution, fn
//   protein_objects_lists_and_reference_names_are_shared_syntax, fn expressions_keep_byte_spans, fn
//   malformed_text_and_excessive_syntax_nesting_are_errors.
// BeautifulRust (baseline lines):
// - [Clippy: clippy::indexing_slicing] Verify bounds or use checked access on fallible inputs.
//   Sites 43.
// - [Clippy: clippy::panic] `panic` should not be present in production code. Sites 44, 76.
// - [review: §5 Panics] Verify panic isolation boundary and preserve the failure contract. Sites
//   111.

#[test]
fn scripts_and_declarations_format_without_execution() {
    let source = "dir = \"missing\"\nlet amount = 0.1 + 0.2\nprint(amount == 0.3 and true)\nTask (@task: -1) {\nprint(quantity(@secret)) stays prose.\n}\n";
    let document = lingua::parse(source).unwrap();
    assert_eq!(document.declarations.len(), 4);
    let formatted = lingua::format(&document);
    assert_eq!(lingua::canonicalize(&formatted).unwrap(), formatted);
    assert!(formatted.contains("print(quantity(@secret)) stays prose."));
    assert!(
        lingua::project(&document)
            .unwrap_err()
            .message
            .contains("do not execute")
    );
    let (identified, _) = lingua::ensure_uids(source).unwrap();
    assert!(identified.contains("let amount"));
}

#[test]
fn protein_objects_lists_and_reference_names_are_shared_syntax() {
    let expression = lingua::expression::parse(
        "query({source: \"record\", where: [{quantity_lt: -0.25}], fields: [\"head\"], limit: 10})",
    )
    .unwrap();
    let formatted = lingua::expression::format(&expression);
    assert!(lingua::expression::parse(&formatted).is_ok());
    for source in [
        "@apples.stock + 2",
        "sum(@reading/log, 0.5h)",
        "#task && !0",
        "extension(@record, \"work\", \"score\")",
    ] {
        assert!(lingua::expression::parse(source).is_ok(), "{source}");
    }
    assert!(lingua::expression::parse("1); print(2").is_err());
}

#[test]
fn expressions_keep_byte_spans() {
    use lingua::grammar::grammar::{Declaration, Statement};
    let source = "let amount = 1 + 2;\n";
    let document = lingua::parse(source).unwrap();
    let Declaration::Statement(Statement::Binding(binding)) = &document.declarations[0] else {
        panic!("binding expected")
    };
    let (start, end) = binding.value.node.span;
    assert_eq!(&source[start..end], "1 + 2");
}

#[test]
fn malformed_text_and_excessive_syntax_nesting_are_errors() {
    assert!(lingua::parse(r#"print("\x");"#).is_err());
    let source = format!("print({}1{});", "(".repeat(1000), ")".repeat(1000));
    assert!(
        lingua::parse(&source)
            .unwrap_err()
            .message
            .contains("limit")
    );
}
#[test]
fn calls_and_record_titles_share_prefixes_without_losing_calls() {
    for source in [
        "print(1);",
        "print(true);",
        "print(1 + 2);",
        "print(query({source: \"record\"}));",
        "print(false && quantity(@missing));",
    ] {
        if let Err(error) = lingua::parse(source) {
            let mut parser = rust_sitter::tree_sitter::Parser::new();
            parser
                .set_language(&lingua::grammar::grammar::language())
                .unwrap();
            let tree = parser.parse(source, None).unwrap();
            panic!("{source}: {error}: {}", tree.root_node().to_sexp());
        }
    }
}

#[test]
fn control_flow_and_assignment_format_and_preserve_document_boundaries() {
    let source = "let result = []; for row in [{amount: 1}, {amount: 2}] { let value = get(row, \"amount\"); if value > 1 { result = push(result, value + 10); } else if value == 0 { print(0); } else { print(value); } } result = push(result, 99); print(result);\nTask (@task: 0) {\nfor and if remain ordinary prose here.\n}\n";
    let document = lingua::parse(source).unwrap();
    let formatted = lingua::format(&document);
    assert_eq!(lingua::canonicalize(&formatted).unwrap(), formatted);
    assert!(formatted.contains("  if (value > 1) {\n    result = push(result, (value + 10));"));
    assert!(formatted.contains("} else if (value == 0) {"));
    assert!(formatted.contains("for and if remain ordinary prose here."));
    assert!(lingua::project(&document).is_err());
    let (identified, _) = lingua::ensure_uids(source).unwrap();
    assert!(identified.contains("for row in"));
    assert!(identified.contains("result = push(result, 99);"));
}

#[test]
fn blocks_require_complete_conditions_and_balanced_braces() {
    for source in [
        "if {}",
        "for x in {}",
        "if true { print(1);",
        "else { print(1); }",
        "for x [1] { print(x); }",
        "value = ;",
    ] {
        let mut parser = rust_sitter::tree_sitter::Parser::new();
        parser
            .set_language(&lingua::grammar::grammar::language())
            .unwrap();
        let tree = parser.parse(source, None).unwrap();
        let parsed = std::panic::catch_unwind(|| lingua::parse(source));
        assert!(
            matches!(parsed, Ok(Err(_))),
            "{source}: {}",
            tree.root_node().to_sexp()
        );
    }
}
