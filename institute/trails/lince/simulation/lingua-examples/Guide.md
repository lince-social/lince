# Lingua syntax examples

These files cover the current grammar and built-in script functions. Each file runs on its own; declarations and variables are local to that execution. Running a file prints output and leaves its source and database data unchanged.

| File | Shows | Database needed |
| --- | --- | --- |
| [01-values.lingua](01-values.lingua) | Every expression operator, literal type, lists, objects, access, conversion, printing, short-circuit conditions | No |
| [02-control-flow.lingua](02-control-flow.lingua) | Assignment, `for`, nested loops, `if` / `else if` / `else`, block scopes, collecting an array | No |
| [03-records.lingua](03-records.lingua) | Record headers and bodies, identities, assertions, quantities, units, links, slugs, UIDs, local Record reads | No |
| [04-automation-and-extensions.lingua](04-automation-and-extensions.lingua) | Both Frequency forms, Karma Rules, conditions, cadence, anchors, Extension values, printing declarations | No |
| [05-database-report.lingua](05-database-report.lingua) | Protein objects and JSON, filters, ordering, aggregation, Record reads, query → condition → loop → array → print | Yes |
| [06-database-declarations.lingua](06-database-declarations.lingua) | Printing saved Frequency, Rule, and Extension declarations | Yes |

## Run, check, format, evaluate

From the repository root:

```sh
cargo xtask lingua institute/trails/lingua-examples/01-values.lingua
cargo xtask lingua institute/trails/lingua-examples/02-control-flow.lingua
cargo xtask lingua institute/trails/lingua-examples/03-records.lingua
cargo xtask lingua institute/trails/lingua-examples/04-automation-and-extensions.lingua
cargo xtask lingua check institute/trails/lingua-examples/
cargo xtask lingua fmt institute/trails/lingua-examples/02-control-flow.lingua
cargo xtask lingua eval '0.1 + 0.2 == 0.3 and !false'
```

Passing a file executes it; there is no `run` subcommand. `check` and `fmt` never execute the file or open a database. `fmt` prints formatted source; add `--write` only when you want to save that formatting. With an installed binary, replace `cargo xtask lingua` with `lince lingua`.

The database examples require an existing, initialized Lince directory containing `lince.db`:

```sh
cargo xtask lingua institute/trails/lingua-examples/05-database-report.lingua --directory /path/to/lince
cargo xtask lingua institute/trails/lingua-examples/06-database-declarations.lingua --directory /path/to/lince
cargo xtask lingua eval 'quantity(@my-record) > 0' --directory /path/to/lince
```

Directory selection uses `--directory`, then a script's `dir = "..."`, then `LINCE_DATA_DIR`, then Lince's default data location. The report includes `dir = "./instance"` as an example; that path is relative to the script, and the command above overrides it. Without an override or `dir`, the declaration reader uses the environment or default location. A database opens only when a read actually executes. A missing, invalid, or incompatible database produces an error naming its path; Lingua does not create or migrate it.

## Expressions and control flow

Numbers use decimal notation with exact decimal arithmetic. Arithmetic operators are `+ - * / %`; ordering and equality use `< <= > >= == !=`. Unary operators are `- + !`. Boolean operators are `&& ||` or `and or`; they short-circuit. Parentheses group expressions. Conditions accept Booleans and numbers: zero is false and any other number is true. Text concatenation uses `+`; use `str(value)` when joining text to a number or another value.

Strings use JSON escapes inside double quotes, or literal text inside triple quotes. References such as `@syntax-apples` and Concepts such as `#task` are values; writing a reference alone does not read a database quantity in a script. Use `quantity(reference)` explicitly. Durations use `s`, `m`, `h`, or `d`, accept decimals such as `0.5h`, and print as seconds. Duration arithmetic is not implemented.

Lists use `[value, value]`; objects use `{key: value, "other key": value}`. `get(list, index)` uses zero-based indexes; `get(object, "key")` reads a field. Missing indexes or fields are errors. `len` accepts lists or text and counts Unicode characters in text. `push(array, value)` returns a new array; assign it back with `result = push(result, value)`. `print` accepts any number of arguments, separates them with spaces, and ends with a newline.

Declare a variable with `let name = expression;`, then change it with `name = expression;`. Assignment requires an existing variable and changes the nearest enclosing binding. `for name in array { ... }` evaluates the array once and creates fresh local names for each iteration. Branches and loop bodies have block scope; they can shadow outer names. User-defined functions, `while`, `break`, and `continue` are not implemented.

Use braces for blocks and a space after `if`. Calls have no space before the opening parenthesis: `print(value)`. Semicolons on simple statements are optional; the examples use them consistently. `dir` is reserved for one top-level assignment to a literal path. Record, Frequency, Karma, and Extension declarations belong at the top level.

## Records and declarations

A Record is `Title (@slug: quantity @unit, fields...) { body }`. Its slug, unit, fields, body, and durable UID are optional; a Record without a slug starts its header with its quantity. When present, a Record UID appears beside both braces as `r_...`, without a caret, and must match at both ends. References can use a slug or UID. A body link such as `[[Basket|r_...]]` points to a Record by UID.

Header fields include identity `is #food`, tag `#fresh`, link `#part-of @basket`, amount `#needs: 3 @units`, and link with amount `#reserved @basket: 2 @units`. A Record has at most one identity. Concept terms in headers can contain Unicode or use quoted text such as `#"saved geographic place"`; script Concept literals use simple names. Header annotations beginning with `//` are accepted only after a field-separating comma and before the next field. Script statements do not support comments. Bodies accept Markdown, including fenced examples containing closing braces.

`record(reference)` prints a Record declaration. `head`, `body`, `slug`, and `quantity` read its properties. `property(reference, "key")` also handles `uid`, and local Records support `unit`. `declaration(reference)` reads a declaration from the current file. `frequency(reference)`, `karma("group")`, `rule(reference)`, and `extension(reference, "namespace")` print local definitions. Local declarations take precedence over database reads with the same name. No file is imported implicitly.

A Frequency has a name, optional title and UID, a quantity switch (`0` or `1`), and either `every` / `timezone` / `next_at` or a JSON `definition` string. The short scheduling form uses UTC. The definition form holds the shared typed Frequency representation, including elapsed or calendar cadence and its scheduling policies. JSON is quoted text in Lingua; the inner JSON follows the Frequency schema.

Karma groups contain `Rules { Rule name { ... } }`. A Rule has a quantity switch, target Record, consequences, and optional Frequency reference, condition, gate, carry, note, JSON cadence, and anchor instant. Frequency and Rule UID suffixes use a caret, as shown in the automation example. Gates include `always`, `!=0`, and numerical comparisons such as `>=1`; carry can be `value`, `one`, or `const:2`.

Karma conditions share the expression syntax but run with the engine's condition readings. Bare `@references`, windowed readings such as `sum(@counter, 30d)`, and numeric Extension readings are demonstrated inside a stored condition. Those readings are not additional functions in the command-line script host. Printing a condition or consequence does not evaluate or execute it.

Extension declarations use `key value`, nested `{ key value }`, and `[value value]`, without the colons or commas used in script objects and lists. They support text, JSON numbers including exponent notation, Booleans, null, lists, and objects. Their namespace is a quoted string.

## Database queries

`query` takes an ordinary Protein descriptor object or JSON text. Protein supplies its sources, filters, includes, fields, ordering, and aggregation; Lingua does not define a second query language. Numeric quantity operands in descriptor objects can be numbers, such as `{quantity_gt: 0}`. In JSON text use exact decimal strings, such as `"quantity_gt":"0"`. Record quantity fields in query results are decimal strings; use `quantity(get(row, "uid"))` to obtain a number for arithmetic.

`records()` prints Record declarations; `records(#task)` selects by Concept, and `records("apple")` searches text. To transform data, use the row objects from `query`. A list of declarations prints as consecutive declarations; a list of ordinary values prints as an array. Reads share one consistent database snapshot. Changing a script variable or collecting an array changes only that execution's local values.

The saved-declaration example skips rows without a Frequency definition and Records without a `work` Extension. Recurrence queries include both Rules and projected occurrences; the loop checks `kind` before passing a Rule UID to `rule`. A Frequency whose active revision differs from its saved head, or whose active parameters differ from its definition's defaults, cannot be represented by one printed declaration and produces an error; query the underlying revision data instead. `karma("group")` and `declaration` select declarations from the current file; the database readers are `frequency`, `rule`, `extension`, and the Record functions. Sources needing a running Cell's live context are unavailable here.

Scripts have execution, nesting, output, and database-read limits. Queries default to 100 rows and accept a limit from 1 to 1024. The database examples request small result sets; `records` uses the default. These examples demonstrate reading and local computation, with no database writes or source-file filling.

The syntax authority is [the shared grammar](../../../crates/anicca/src/grammar.rs); the script behavior is in [the evaluator](../../../crates/nucleus/src/script.rs) and [the Lince data host](../../../crates/lince/src/lingua/data.rs).
