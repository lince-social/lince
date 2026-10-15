use nucleus::script::{Evaluator, Host, Value};

#[derive(Debug, Default)]
struct MemoryHost {
    output: String,
    reads: usize,
}

impl Host for MemoryHost {
    fn call(&mut self, name: &str, _: &[Value]) -> Result<Value, String> {
        self.reads += 1;
        Err(format!("No database for {name}"))
    }
    fn print(&mut self, text: &str) -> Result<(), String> {
        self.output.push_str(text);
        Ok(())
    }
}

fn run(source: &str) -> Result<MemoryHost, nucleus::script::Error> {
    let document = anicca::parse(source).unwrap();
    let mut host = MemoryHost::default();
    Evaluator::new(&mut host).run(&document)?;
    Ok(host)
}

#[test]
fn pure_calculation_conditions_and_text_need_no_database() {
    let host = run("let amount = 0.1 + 0.2; print(amount); print(amount == 0.3); print(9007199254740993 + 2); print(2 + 3 * 4 > 10); print(\"Amount: \" + str(amount));").unwrap();
    assert_eq!(
        host.output,
        "0.3\ntrue\n9007199254740995\ntrue\nAmount: 0.3\n"
    );
    assert_eq!(host.reads, 0);
}

#[test]
fn boolean_short_circuit_never_requests_data() {
    let host =
        run("print(false && quantity(@missing)); print(true || quantity(@missing));").unwrap();
    assert_eq!(host.output, "false\ntrue\n");
    assert_eq!(host.reads, 0);
}

#[test]
fn local_records_print_and_supply_quantities_without_a_database() {
    let host = run(
        "Apples (@apples: 10) {\nFruit.\n}\nprint(record(@apples)); print(quantity(@apples) + 2);",
    )
    .unwrap();
    assert!(host.output.contains("Apples (@apples: 10)"));
    assert!(host.output.ends_with("12\n"));
    assert_eq!(host.reads, 0);
}

#[test]
fn local_durable_identities_resolve_without_database_reads() {
    let host = run("Apples (@apples: 3) { r_01JA0000000000000000000010\nFruit.\n} r_01JA0000000000000000000010\nprint(quantity(@r_01JA0000000000000000000010)); print(record(@r_01JA0000000000000000000010));").unwrap();
    assert!(host.output.starts_with("3\nApples"));
    assert_eq!(host.reads, 0);
}

#[test]
fn printing_local_feature_declarations_never_executes_them() {
    let host = run("Frequency daily { title \"Every day\" quantity 0 definition \"unused\" }\nKarma tasks { Rules { Rule first { quantity 1 record @task condition \"\"\"query_command(@unavailable) > 0\"\"\" consequences \"\"\"[]\"\"\" } } }\nExtension @task \"work\" { score 2 }\nprint(frequency(@daily)); print(karma(\"tasks\")); print(rule(@first)); print(extension(@task, \"work\"));").unwrap();
    assert!(host.output.contains("Frequency daily"));
    assert!(host.output.contains("query_command(@unavailable)"));
    assert!(host.output.contains("Extension @task \"work\""));
    assert_eq!(host.reads, 0);
}

#[test]
fn invalid_types_unknown_bindings_and_duplicate_bindings_fail() {
    for source in [
        "print(\"x\" + 1);",
        "print(unknown);",
        "let x = 1; let x = 2;",
        "print(1 / 0);",
        "print({x: 1, x: 2});",
    ] {
        assert!(run(source).is_err(), "{source}");
    }
}

#[test]
fn offline_validation_finds_binding_errors_before_any_host_effect() {
    let document = anicca::parse("print(quantity(@apples)); print(undefined);").unwrap();
    let mut host = MemoryHost::default();
    assert!(
        Evaluator::new(&mut host)
            .run(&document)
            .unwrap_err()
            .message
            .contains("undefined")
    );
    assert_eq!(host.reads, 0);
    assert!(host.output.is_empty());
}

#[test]
fn collections_and_exact_json_operands_are_bounded_values() {
    let host = run("let data = {limit: 10, where: [{quantity_lt: -0.25}]}; print(get(data, \"limit\")); print(get([1, 2], 1));").unwrap();
    assert_eq!(host.output, "10\n2\n");
    let number = Value::Number(nucleus::DecimalValue::parse_inferred("0.123456789").unwrap());
    assert_eq!(number.to_json().unwrap(), serde_json::json!("0.123456789"));
}

#[test]
fn output_errors_propagate_and_deep_expressions_fail_cleanly() {
    struct ClosedOutput;
    impl Host for ClosedOutput {
        fn call(&mut self, _: &str, _: &[Value]) -> Result<Value, String> {
            unreachable!()
        }
        fn print(&mut self, _: &str) -> Result<(), String> {
            Err("closed output".into())
        }
    }
    let document = anicca::parse("print(1);").unwrap();
    let mut host = ClosedOutput;
    assert!(
        Evaluator::new(&mut host)
            .run(&document)
            .unwrap_err()
            .message
            .contains("closed output")
    );
    let source = format!("print({}1{});", "(".repeat(70), ")".repeat(70));
    assert!(run(&source).unwrap_err().message.contains("limit"));
}

#[test]
fn loops_filter_transform_and_accumulate_exact_values() {
    let host = run("let result = []; for amount in [0, 0.1, 0.2, 0.3] { let value = amount; if value >= 0.2 { value = value + 0.1; result = push(result, value); } else if value == 0 { result = push(result, -1); } } print(result);").unwrap();
    assert_eq!(host.output, "[-1, 0.3, 0.4]\n");
    assert_eq!(host.reads, 0);
}

#[test]
fn block_bindings_are_local_and_outer_assignments_accumulate() {
    let host = run("let x = 10; let result = []; for x in [1, 2] { let next = x + 1; if x == 1 { let x = 99; result = push(result, x); } else { result = push(result, next); } x = x + 20; } print(x); print(result);").unwrap();
    assert_eq!(host.output, "10\n[99, 3]\n");
}

#[test]
fn for_uses_one_snapshot_of_its_iterable() {
    let host = run("let values = [1, 2, 3]; let result = []; for value in values { values = []; result = push(result, value); } print(result); print(values);").unwrap();
    assert_eq!(host.output, "[1, 2, 3]\n[]\n");
}

#[test]
fn untaken_branches_and_empty_loops_never_request_data() {
    let host = run("if false { print(quantity(@missing)); } else if true { print(7); } else { print(quantity(@missing)); } for value in [] { print(quantity(@missing)); } print(8);").unwrap();
    assert_eq!(host.output, "7\n8\n");
    assert_eq!(host.reads, 0);
}

#[test]
fn all_block_scopes_are_checked_before_host_effects() {
    for source in [
        "print(quantity(@missing)); if false { typo = 1; }",
        "print(quantity(@missing)); if true { let hidden = 1; } print(hidden);",
        "print(quantity(@missing)); for item in [] {} print(item);",
        "print(quantity(@missing)); for item in [] { let value = 1; let value = 2; }",
        "print(quantity(@missing)); if false { dir = \"ignored\"; }",
        "print(quantity(@missing)); dir = quantity(@missing);",
        "print(quantity(@missing)); for dir in [] {}",
        "print(quantity(@missing)); missing = 1;",
    ] {
        let document = anicca::parse(source).unwrap();
        let mut host = MemoryHost::default();
        assert!(
            Evaluator::new(&mut host).run(&document).is_err(),
            "{source}"
        );
        assert_eq!(host.reads, 0, "{source}");
        assert!(host.output.is_empty(), "{source}");
    }
}

#[test]
fn control_flow_checks_types_and_preserves_array_value_semantics() {
    for source in [
        "for item in 1 {}",
        "for item in \"abc\" {}",
        "for item in null {}",
        "if \"yes\" {}",
        "let result = []; result = push(1, 2);",
    ] {
        assert!(run(source).is_err(), "{source}");
    }
    let host =
        run("let original = [1]; let result = push(original, 2); print(original); print(result);")
            .unwrap();
    assert_eq!(host.output, "[1]\n[1, 2]\n");
}

#[test]
fn nested_empty_loops_are_charged_against_execution_limits() {
    let source = format!(
        "let values = [{}]; for a in values {{ for b in values {{ for c in values {{ for d in values {{}} }} }} }}",
        vec!["0"; 20].join(",")
    );
    let error = run(&source).unwrap_err();
    assert!(error.message.contains("limit"), "{error}");
    let source = format!(
        "{}print(quantity(@missing));{}",
        "if true {".repeat(70),
        "}".repeat(70)
    );
    let error = run(&source).unwrap_err();
    assert!(error.message.contains("limit"), "{error}");
}

#[test]
fn cancellation_stops_loop_iterations() {
    #[derive(Default)]
    struct StoppingHost {
        output: String,
        lines: usize,
    }
    impl Host for StoppingHost {
        fn call(&mut self, _: &str, _: &[Value]) -> Result<Value, String> {
            unreachable!()
        }
        fn print(&mut self, text: &str) -> Result<(), String> {
            self.output.push_str(text);
            self.lines += 1;
            Ok(())
        }
        fn cancelled(&self) -> bool {
            self.lines >= 2
        }
    }
    let document = anicca::parse("for value in [1, 2, 3, 4] { print(value); }").unwrap();
    let mut host = StoppingHost::default();
    assert!(
        Evaluator::new(&mut host)
            .run(&document)
            .unwrap_err()
            .message
            .contains("cancelled")
    );
    assert_eq!(host.output, "1\n2\n");
}
