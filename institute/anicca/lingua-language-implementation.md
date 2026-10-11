# Lingua expressions, configuration, and scripts

Implementation proposal, 2026-10-11. This document is a plan, not an implemented language specification. No runtime, grammar, migration, or owner-authored `.lingua` file is changed by this proposal.

## Direction and decision status

The agreed direction is to make Lingua the syntax authority for conditions, retain Nucleus as the evaluator, and retain the engine as the supplier of live readings. The next requested capability is a `.lingua` file that selects the Lince directory and runs small expressions, starting with variables and printing. Grow this as a DSL in small steps, with architecture that can accommodate functions and general computation later.

Python-like syntax, Zig's preference for visible behavior, and Rust's emphasis on checked types and errors are design influences. They do not require reproducing those languages.

The specific typing rules, execution modes, command names, and later syntax below are recommendations. They need to be distinguished from the goals already agreed. The immediate deliverable is this Markdown only.

## What exists today

- `crates/anicca/src/grammar.rs` defines the typed rust-sitter grammar. `ConditionField` contains a `Body`, so a condition's expression is currently opaque text to Lingua.
- `crates/anicca/src/lib.rs` parses, projects, and formats documents. `ProjectedRule.condition` is currently a string.
- `crates/nucleus/src/expr.rs` has a separate lexer, expression parser, expression tree, and a floating-point evaluation path.
- `crates/nucleus/src/karma/condition.rs` validates those expressions and evaluates Karma conditions using `DecimalValue`. Its domain rules include rejecting conditions with no readings.
- `crates/engine/src/actions.rs` resolves readings and applies the condition's gate and carry. File synchronization passes through `crates/engine/src/file_sync/lingua.rs` and its `automation.rs` module.
- `crates/utils/src/config.rs` provides a process-wide directory override through `OnceLock<PathBuf>`. The default uses the operating system configuration directory plus `lince`; an override also puts cache files below the selected directory.
- `crates/lince/src/main.rs` accepts `--directory`. `crates/cell/src/lib.rs` also accepts a directory through startup options.
- `crates/anicca/build.rs` generates the Tree-sitter parser through Rust tooling and compiles its generated C. There is no need to introduce Node.js or a second parser toolchain.
- Rhai was declared in the workspace manifest when this discussion began, but no crate consumed it. Recheck before removing that declaration during implementation.

These observations describe the working tree reviewed for this plan. Other developers have active changes, so recheck the named seams before implementing.

## The requested example and its ambiguity

The requested shape is:

```lingua
dir = "/home/user/.config/test_lince"
print("Quantity is: " + @my-record + 2)
```

The directory assignment is a good small starting point. The print expression needs a precise meaning: if the Record quantity is 10, is the desired output `Quantity is: 12`, or `Quantity is: 102`? With ordinary left-associative `+` and implicit text conversion, it would be the latter. Python itself rejects adding a string and a number this way.

Recommendation: numeric `+` adds numbers; text `+` joins text; mixed operands are a type error. Support the intended arithmetic explicitly:

```lingua
dir = "/home/user/.config/test_lince"
print("Quantity is: " + str(@my-record + 2))
```

With quantity 10, this prints `Quantity is: 12`. Later, interpolation can make that shorter:

```lingua
print(f"Quantity is: {@my-record + 2}")
```

Interpolation is a later proposal, not part of the first increment. It should parse embedded expressions through the same expression grammar, never through a separate mini-language.

If accepting the exact original expression becomes a requirement, specify a fixed built-in text conversion rule and document its `102` behavior. Avoid changing addition precedence based on operand types or evaluating the numeric tail specially. Those rules would make programs harder to reason about.

## One language, explicit execution modes

A `.lingua` document can contain declarations and, when explicitly run as a script, statements. The shared grammar recognizes both. The host chooses what operations are allowed.

| Mode | Purpose | Allowed behavior |
| --- | --- | --- |
| Document | Check, format, import, and synchronize Records and rules | Parse declarations and expressions; never execute statements or apply directory overrides |
| Configuration | Select the instance before startup | Read a restricted literal `dir` assignment; no Record reads, printing, calls, or database access |
| Condition | Compute a rule reading | Domain-approved expressions and readings; no output, writes, configuration, loops, or arbitrary script execution |
| Script | Explicitly run a selected file | Initially bindings, expressions, read-only Record quantities, and printing |

Do not run scripts on file discovery, editor preview, import, formatting, synchronization, or UID insertion. Report executable statements in document mode as unsupported in that mode; do not silently execute or silently discard them. Record bodies remain prose even if they contain text such as `print(...)`.

Script mode initially rejects Record/Frequency/Karma declarations with a helpful diagnostic. It does not secretly import them. Shared parsing leaves room for an explicit import operation later, after transaction behavior has been designed.

This separation matters because `.lingua` files are already ingested and rewritten by tooling. Introducing execution into those paths would create repeated effects and accidental changes to the selected instance.

## Directory selection before runtime startup

`dir` is a reserved configuration binding at the start of a script or in an explicitly selected configuration file. For the first implementation its value must be one string literal, and it may appear at most once. It is immutable and cannot be assigned inside a function, condition, or later statement. Ordinary variables must not shadow it.

Recommended precedence is explicit `--directory`, then the selected file's `dir`, then the existing operating system default. Show the effective directory and where it came from in diagnostics; a command-line override must not be surprising. No new environment-variable precedence is needed initially.

The file is selected explicitly by a path supplied to the launcher or script command. Do not scan arbitrary project directories for executable configuration. A future default config file, if wanted, must have a fixed discovery location independent of the directory it selects. Looking for the config inside an unknown selected directory creates a bootstrap cycle.

Startup should proceed in this order:

1. Parse arguments and resolve the input file's path.
2. Parse and statically check the entire file without running anything.
3. Extract the restricted configuration prelude and calculate directory precedence.
4. Resolve and validate the chosen path, then install the override once.
5. Acquire the appropriate instance ownership and initialize keys, database, cache, logs, and services using that directory.
6. Resolve Record references and run script statements, if this is script mode.

Audit the existing `init()` call and every early directory consumer: moving the assignment before database creation alone is insufficient if another subsystem has already opened a file. Configuration errors must stop startup before opening an unintended default instance.

Absolute paths are accepted. Relative paths are resolved against the selected `.lingua` file's parent directory, never the shell's changing working directory. Reject empty paths. Do not implicitly expand `~`, `$HOME`, shell substitutions, or environment variables inside strings. Normalize the path consistently before instance locking; for nonexistent destinations, validate and create through the existing startup path rather than assuming canonicalization already works.

Use the existing directory override and startup options first. A mutable global directory would let one script change the identity of a running instance halfway through execution. Longer term, pass an immutable instance context explicitly if multiple instances must coexist in one process.

The first script runner should use the existing headless ownership guard and fail clearly if it cannot safely access the chosen instance. A later client mode can talk to an already running instance. Do not open a competing runtime and bypass locks just to make printing work.

## Expression semantics

Start with decimal numbers, text, booleans, durations, Record references, local names, parentheses, unary operators, arithmetic, comparisons, boolean operators, and calls. Introduce a Unit value for statements such as `print`; it cannot participate in arithmetic.

- Preserve literal decimal text until checked conversion to `DecimalValue`. Do not pass through `f64` in the new script or Karma paths.
- Specify decimal scale, division rounding, overflow, and division-by-zero behavior using the existing exact arithmetic rules. Preserve existing Karma results while unifying syntax.
- `@my-record` reads that Record's quantity in an expression. It never means assignment, a mutable Record object, or a string containing its title.
- Resolve slugs to durable UIDs before evaluation. Reuse existing binding behavior; unknown or ambiguous references are errors, not zero.
- Preserve the existing meanings of dotted references and reading functions. Explicitly distinguish them from any future field access syntax.
- Bare variable names use a separate lexical space from `@references`. Ordinary names use Python-like letters, digits, and underscores; hyphens remain permitted in Record slugs. `@my-record - 2` must tokenize correctly.
- `and`, `or`, and `not` use boolean operands and short-circuit behavior. Do not introduce implicit truthiness for text or quantity values. Preserve existing comparison-to-numeric behavior at the Karma adapter where required; scripts get explicit Bool values.
- Calls have declared signatures and effects. Validate arity and argument types before output or host work whenever those checks are possible statically.
- Preserve domain functions such as `freq`, `sum`, and extension readings. Whether a function is valid in a condition is a domain check, not a different grammar.
- Numeric quantities carrying physical units need an explicit policy. Initially preserve existing scalar quantity semantics; do not claim that `@my-record + 2` performs unit-aware arithmetic. Add dimension checking as a separate increment.

The general parser must accept constant expressions such as `1 + 2`. The Karma validator may still reject a rule condition with no readings. Otherwise the condition-specific restriction would incorrectly prohibit useful scripts and configuration calculations.

For the first script runtime, local assignment creates a binding, and subsequent assignment must retain its inferred type. Evaluate statements left to right; reject use before initialization. Built-in names cannot be overwritten. Define lexical scopes before adding functions; avoid dynamic scoping and a global map shared by separate runs.

Read-only scripts use a consistent quantity snapshot per run, rather than seeing a Record change between repeated reads. Future multi-reading scripts need an explicit evaluation context containing a fixed timestamp and host consistency policy. Treat unavailable data as a typed error rather than silently rereading or substituting zero.

`print` writes a newline to the runner's output sink. Errors use a separate diagnostic sink. Cap output size and return an error if writing fails. Do not make `print` create Records or persist notifications automatically. Early script versions can require `print` arguments to be Text; `str` provides deliberate conversion with locale-independent decimal formatting.

## Parser and runtime architecture

Use a pipeline with explicit boundaries:

```text
source -> Lingua syntax tree with spans -> resolved and checked program
       -> Nucleus evaluation -> engine host readings/output
```

Keep syntax in Anicca and execution outside its parser. Define expression nodes for literal, name, Record reference, unary expression, binary expression, and call. Add statement nodes for assignment and expression statement. Declarations remain their own nodes. Carry source byte ranges through resolution and lowering so a runtime error can identify the original expression.

Expose a standalone expression entry point backed by the same production used inside document conditions. Verify rust-sitter's entry-point facilities early. If the generated parser only exposes a document start symbol, use a deliberately supported expression wrapper with correct span offsets, or generate another start grammar from shared productions. Do not copy operator definitions into a second handwritten parser.

Anicca currently does not depend on Nucleus and Nucleus does not depend on Anicca. Recommended first approach: Nucleus consumes Anicca's syntax-only expression API through a one-way dependency, then lowers it to a checked evaluation representation. Keep arithmetic and host services out of Anicca. Confirm the workspace dependency graph before adding this edge. If parser generation makes that dependency too expensive, extract a small shared syntax crate while retaining one authoritative grammar definition; do not create competing expression definitions.

Migrate `Expr::parse` consumers in stages. Audit conditions, promise expressions, formula evaluation, reference extraction, canonicalization, and binding resolution. Some existing consumers use floating-point evaluation: parser unification must not silently claim they have all become exact decimal evaluators. Record each adapter and retire the handwritten lexer/parser only after all consumers use Lingua.

Store condition source for editing and diagnostics; compile it through the common grammar for execution. Native conditions should become expressions rather than triple-quoted opaque bodies:

```lingua
condition 1 - @toggle-demo
gate always
carry value
```

Retain gate and carry as separate domain controls. Keep existing consequence execution and authorization in the engine. Typed consequence syntax could be a later DSL improvement, but replacing consequence JSON is outside this first feature.

There is no requirement to preserve old releases. During the transition, current fixtures and user-owned files must still be accounted for: identify old quoted conditions and provide a clear diagnostic or an owner-run conversion. Do not silently rewrite `.lingua` files. A temporary adapter can ease sequential implementation, but should not become a permanent second syntax authority.

Existing projection and formatter paths must explicitly handle new node kinds. A formatter or UID operation must preserve statements and must never drop executable source while updating declaration metadata.

## Python syntax, Zig philosophy, Rust robustness

Borrow Python's assignments, call syntax, readable boolean keywords, and eventually `def`, `if`, `while`, and `return`. Python's lexer emits indentation tokens to define blocks; that is a concrete implementation requirement, not merely a formatting preference. See the [Python lexical reference](https://docs.python.org/3/reference/lexical_analysis.html).

Lingua currently ignores all whitespace as grammar extras and uses braces for declarations. Make executable statements newline-delimited while leaving prose bodies untouched. The first increment needs meaningful newlines but no indentation-sensitive blocks. Before adding blocks, prototype scanner handling of newline, INDENT, DEDENT, continuation inside parentheses, strings, and dedents around existing brace-based declarations. [Tree-sitter external scanners](https://tree-sitter.github.io/tree-sitter/creating-parsers/4-external-scanners.html) describe the mechanism for tokens that need contextual scanning. Verify its integration with this rust-sitter build before promising full Python indentation.

Borrow Zig's small language and visible operations. Its [official overview](https://ziglang.org/learn/overview/) describes explicit control flow and allocation behavior. In Lingua, make writes and external actions visible calls, avoid user-defined operator overloading, and keep errors explicit. There is a deliberate exception: `@record` is convenient read syntax. Document it as a host read and resolve/cache it explicitly; do not pretend Lingua has Zig's no-hidden-work guarantee. A managed DSL will also allocate text and AST nodes; use visible runtime budgets rather than copying manual allocator syntax into every script.

Borrow Rust's enums, checked operations, and recoverable errors in the implementation. The [Rust Result chapter](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html) is a useful model for propagating failures instead of crashing. Rust implementation alone does not make the interpreted language type-safe: add a checker, a controlled host boundary, and resource limits. A borrow checker, lifetimes, raw pointers, and unsafe blocks are unnecessary language features for this DSL.

## Growing toward general computation

Start with a tree-walk evaluator and a small built-in registry. Keep resolution, type checking, values, execution context, and host calls separate so a bytecode VM can replace tree walking later without changing language meaning.

Recommended growth order:

1. Shared expressions and native conditions.
2. Directory prelude, local bindings, `str`, `print`, and read-only Record quantities.
3. Pure user functions with lexical scopes, parameters, return types where inference is insufficient, and explicit return.
4. Boolean branching and bounded iteration over collections.
5. General loops or recursion, collection values, and explicit error handling.
6. Modules and deliberate host writes, when concrete Lince use cases justify them.

Functions, state, and unrestricted iteration or recursion can provide a path to Turing completeness under idealized unbounded resources. Every real runtime has finite resources. Lince-hosted programs should retain fuel, memory, output, call-depth, and host-read limits, plus cancellation and deadlines. That bounded execution does not prevent designing a general computational language; it limits what one hosted run can consume.

Condition mode should remain a restricted, predictable subset even if script mode gains general loops. Pure helper functions may eventually be allowed in conditions after checking their effects and enforcing execution bounds. Do not embed arbitrary scripts in automatic rules as a shortcut.

Avoid a JIT, LLVM backend, package manager, macros, classes, async language syntax, foreign-function interface, or self-hosting project until actual scripts require them. General computation does not require those features.

Future Record writes should call existing engine actions with actor identity, authorization, validation, transactions, and history. `@record = 2` should remain invalid initially. Define atomicity before admitting writes: static checking happens before execution, but already printed output cannot be rolled back if a later runtime operation fails.

## Sequential implementation and acceptance

### 1. Define the small language contract

Inventory every current expression parser consumer. Record operator precedence, decimal behavior, reference syntax, gate/carry conversion, statement delimiters, directory precedence, and mixed text/numeric behavior. Create focused examples and expected diagnostics. Confirm the recommended explicit `str` expression before considering it finalized.

Acceptance: the quantity-10 example has one documented output; document/check/sync/run modes have distinct behavior; no ambiguous startup path is left unspecified.

### 2. Add common expression grammar

Extend `crates/anicca/src/grammar.rs`, expose expression parsing, preserve source spans, and extend formatter/checker behavior. Keep generated parser artifacts under the existing build process. Prototype ambiguity between free-form Record titles, top-level assignments, calls, and multiline condition expressions before broadening the syntax.

Acceptance: standalone and embedded expressions produce equivalent trees; precedence and associativity are correct; Record prose remains prose; invalid input is rejected without evaluating anything. Add round-trip tests for documents and statements.

### 3. Route Karma through Lingua

Adapt Nucleus parsing and validation, then engine and store consumers. Keep exact decimal evaluation, reading demand behavior, stable bindings, gates, and carry. Update current Rust test fixtures as needed; report owner-authored files requiring conversion rather than editing them. Remove the old lexer/parser only after its consumer audit is complete.

Acceptance: existing condition tests pass; standalone and embedded paths produce the same result and domain errors; no runtime path falls back to Rhai or another condition parser.

### 4. Implement startup configuration

Add explicit file selection to the launcher and a configuration prepass. Reuse `utils::config` and cell startup options, with one validated directory installed before any consumer. Preserve existing cache behavior and instance ownership. Test separately in child processes where the process-wide OnceLock would prevent isolated test cases.

Acceptance: the requested literal directory overrides the default; CLI precedence works; relative paths are stable; invalid config does not initialize the default instance; a later reassignment fails. A second instance cannot bypass the ownership guard.

### 5. Implement the read-only script runner

Add typed bindings, builtin signatures, `str`, `print`, a quantity resolver, execution budgets, and output/error sinks. Extend `cargo xtask` with language commands rather than depending on external tooling. Proposed interfaces are `cargo xtask lingua check <file>`, `cargo xtask lingua fmt <file>`, and `cargo xtask lingua run <file>`; these commands do not exist yet. Checking and formatting do not execute or select an instance.

Acceptance: the revised two-line script prints the expected value from the chosen instance; missing Records, mixed-type arithmetic, overflow, and output failures return useful errors; imports and sync never run the script. Directory configuration is applied before Record binding.

### 6. Add native interface support when approved

Recommend a Rust interface view with source diagnostics, effective directory, an explicit Run action, output, and Stop when execution supports cancellation. Reuse existing source/editor components where useful. Keep evaluation in the engine so UI and command-line results agree. This UI is a recommendation requiring the human's comments before implementation; it is not necessary for the initial CLI language increment. Do not modify the unplugged web crate.

### 7. Expand from real scripts

Add functions and block syntax only after the earlier steps work. Choose indentation after the scanner prototype. Add collections, loops, modules, and writes one feature at a time, each with defined types, effects, limits, and focused tests.

For each code increment, use `cargo check`, treat warnings as errors, and run relevant tests. Target Anicca grammar/round-trip tests, Nucleus condition tests, engine sync/script tests, and startup integration tests. Do not add code comments. No schema change is expected initially; if one becomes necessary, append a new migration and checksum and run the required store checks and migration_guard tests. Never alter an existing migration or checksum entry.

## Correctness, performance, and security checks

Test operator precedence, decimal rounding, short-circuit reads, reference tokenization, undefined names, invalid calls, span locations, output encoding, formatter preservation, and failures without panics. Add adversarial deep expressions and oversized literals to check parser and evaluator limits before exposing script execution.

Measure parsing and execution on representative documents and repeated conditions. Reuse a checked expression within an evaluation or cache by source plus language revision and binding context where measurements justify it. Budget host reads and avoid one database query per repeated reference. Any cross-run result cache must respect changing Record quantities; do not cache values as if they were immutable syntax.

The first script has selected-instance reads and an output sink only. No shell, network, arbitrary filesystem calls, imports that execute on load, or database bypass. Check modes must remain effect-free. Later capabilities belong in a host registry with explicit effects and authorization, including transitive effects of user functions. This is a direct consequence of making previously declarative files executable, not an unrelated permission framework.

## Useful references and how to use them

- [rust-sitter's official repository](https://github.com/hydro-project/rust-sitter): typed Rust grammar definitions and precedence facilities. Extend the existing toolchain instead of choosing a new parser first.
- [Tree-sitter: writing the grammar](https://tree-sitter.github.io/tree-sitter/creating-parsers/3-writing-the-grammar.html): precedence, associativity, and ambiguity resolution. Use it for expressions and the overlap between declarations and statements.
- [Tree-sitter: external scanners](https://tree-sitter.github.io/tree-sitter/creating-parsers/4-external-scanners.html): contextual tokens for the indentation prototype. Parser recognition alone does not provide typing or execution.
- [Python's lexical reference](https://docs.python.org/3/reference/lexical_analysis.html): newline handling, indentation, and line continuation. Borrow specified behavior deliberately instead of informally promising Python compatibility.
- [Crafting Interpreters, author's source: statements and state](https://github.com/munificent/craftinginterpreters/blob/master/book/statements-and-state.md), [functions](https://github.com/munificent/craftinginterpreters/blob/master/book/functions.md), and [resolving and binding](https://github.com/munificent/craftinginterpreters/blob/master/book/resolving-and-binding.md): a staged interpreter implementation and the importance of lexical name resolution. Adapt the structure; Lince's host, decimal, and effect requirements need their own design.
- [Zig's official overview](https://ziglang.org/learn/overview/): small-language design and explicit behavior. Adopt the clarity goals while being honest about managed allocation and Record reads.
- [Rust's Result chapter](https://doc.rust-lang.org/book/ch09-02-recoverable-errors-with-result.html): structured failure propagation. Apply it across parsing, validation, evaluation, and host calls.

The recommended first complete feature is a common Lingua expression grammar, native Karma conditions, and an explicitly run read-only script with a startup directory literal. It provides a useful DSL while leaving room for general computation without committing to a large language upfront.
