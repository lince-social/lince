use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    fmt::Write as _,
    num::NonZeroU32,
    str::FromStr,
};

use serde::{Deserialize, Serialize, de::DeserializeOwned};

use super::{
    BinaryOperator, CadenceAst, CadenceBound, CadenceStepAst, CandidateRoute, CanonicalHash,
    Capability, CapabilitySet, CivilDateTime, CivilWeekday, Confidence, DatumState,
    DecimalPrecision, DecimalValue, DeclaredUnit, DurationBinding, DurationMs, ExpressionAst,
    FoldPolicy, FrequencyAst, FrequencyCadenceAst, FrequencyParameterDefinition, FrequencySchema,
    FrequencyTimerAst, GapPolicy, InactiveGapPolicy, InputBinding, InputSource, InvalidDay,
    KarmaBoundaryError, LateEventPolicy, LiteralValue, LocalId, MissedPolicy, NodeAst,
    NodeOperation, OutputRef, OverloadPolicy, ParameterDefinition, PortContract,
    PositiveIntegerBinding, Probability, ProgramAst, ProgramSchema, ReferenceKind, RephasePolicy,
    ResolvedReference, Rounding, Sensitivity, SimulationStatePolicy, Slug, StateContract,
    StateMigrationPolicy, StatePersistence, StateResetPolicy, ThresholdDirection, TimeZoneId,
    TimestampMs, TriggerSource, TypedUid, TzdbRevision, TzdbVersion, UnaryOperator, ValueType,
    WeekdaySet,
};

pub const MAX_DSL_BYTES: usize = 1_048_576;
pub const MAX_DSL_TOKENS: usize = 100_000;
pub const MAX_DSL_STRING_BYTES: usize = 65_536;
pub const MAX_DSL_NESTING: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DslErrorKind {
    SourceTooLarge,
    TooManyTokens,
    StringTooLarge,
    NestingTooDeep,
    UnexpectedCharacter,
    UnexpectedToken,
    UnexpectedEnd,
    InvalidAtom,
    DuplicateDeclaration,
    MissingDeclaration,
    TrailingInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KarmaDslError {
    pub kind: DslErrorKind,
    pub offset: usize,
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl fmt::Display for KarmaDslError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} at {}:{} (byte {}): {}",
            enum_atom(&self.kind),
            self.line,
            self.column,
            self.offset,
            self.message
        )
    }
}

impl std::error::Error for KarmaDslError {}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Word(String),
    String(String),
    Symbol(char),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    kind: TokenKind,
    offset: usize,
    line: usize,
    column: usize,
}

pub fn format_program(program: &ProgramAst) -> String {
    let mut output = String::new();
    writeln!(&mut output, "karma 1;").expect("writing to String cannot fail");
    writeln!(&mut output, "program {} {{", program.slug).expect("String write");
    writeln!(&mut output, "  schema {};", enum_atom(&program.schema)).expect("String write");
    writeln!(&mut output, "  purpose {};", json_string(&program.purpose)).expect("String write");
    write!(&mut output, "  tags [").expect("String write");
    write_joined(&mut output, program.tags.iter().map(|slug| slug.as_str()));
    writeln!(&mut output, "];").expect("String write");
    write!(&mut output, "  capabilities [").expect("String write");
    let mut capability_names = program
        .required_capabilities
        .iter()
        .map(|capability| enum_atom(&capability))
        .collect::<Vec<_>>();
    capability_names.sort();
    write_joined(&mut output, capability_names);
    writeln!(&mut output, "];").expect("String write");

    for (id, parameter) in &program.parameters {
        writeln!(
            &mut output,
            "  param {}: {} {} = {};",
            id.as_str(),
            format_type(&parameter.value_type),
            if parameter.mutable {
                "mutable"
            } else {
                "fixed"
            },
            format_literal(&parameter.default)
        )
        .expect("String write");
    }

    for (id, node) in &program.nodes {
        writeln!(&mut output, "  node {} {{", id.as_str()).expect("String write");
        for (input_id, binding) in &node.inputs {
            writeln!(
                &mut output,
                "    bind {}: {} = source({}, {});",
                input_id.as_str(),
                format_type(&binding.expected_type),
                binding.source.node.as_str(),
                binding.source.port.as_str()
            )
            .expect("String write");
        }
        for (port_id, contract) in &node.outputs {
            let freshness = contract.freshness.map_or_else(
                || "none".to_string(),
                |value| format!("duration({})", value.get()),
            );
            writeln!(
                &mut output,
                "    port {}: {} sensitivity {} freshness {};",
                port_id.as_str(),
                format_type(&contract.value_type),
                enum_atom(&contract.sensitivity),
                freshness
            )
            .expect("String write");
        }
        format_operation(&mut output, &node.operation);
        writeln!(&mut output, "  }}").expect("String write");
    }

    for (id, source) in &program.outputs {
        writeln!(
            &mut output,
            "  output {} = source({}, {});",
            id.as_str(),
            source.node.as_str(),
            source.port.as_str()
        )
        .expect("String write");
    }
    writeln!(&mut output, "}}").expect("String write");
    output
}

pub fn parse_program(source: &str) -> Result<ProgramAst, KarmaDslError> {
    if source.len() > MAX_DSL_BYTES {
        return Err(KarmaDslError {
            kind: DslErrorKind::SourceTooLarge,
            offset: 0,
            line: 1,
            column: 1,
            message: format!("DSL source exceeds {MAX_DSL_BYTES} bytes"),
        });
    }
    let tokens = lex(source)?;
    Parser::new(tokens, source).parse_program()
}

pub fn format_frequency(frequency: &FrequencyAst) -> String {
    let mut output = String::new();
    writeln!(&mut output, "karma-frequency 1;").expect("String write");
    writeln!(&mut output, "frequency {} {{", frequency.slug).expect("String write");
    writeln!(&mut output, "  schema {};", enum_atom(&frequency.schema)).expect("String write");
    writeln!(
        &mut output,
        "  purpose {};",
        json_string(&frequency.purpose)
    )
    .expect("String write");
    write!(&mut output, "  tags [").expect("String write");
    write_joined(&mut output, frequency.tags.iter().map(|slug| slug.as_str()));
    writeln!(&mut output, "];").expect("String write");
    for (id, parameter) in &frequency.parameters {
        match parameter {
            FrequencyParameterDefinition::Duration {
                default,
                minimum,
                maximum,
            } => writeln!(
                &mut output,
                "  param {}: duration default duration({}) range [duration({}), duration({})];",
                id.as_str(),
                default.get(),
                minimum.get(),
                maximum.get()
            )
            .expect("String write"),
            FrequencyParameterDefinition::PositiveInteger {
                default,
                minimum,
                maximum,
            } => writeln!(
                &mut output,
                "  param {}: positive-integer default integer({}) range [integer({}), integer({})];",
                id.as_str(),
                default.get(),
                minimum.get(),
                maximum.get()
            )
            .expect("String write"),
        }
    }
    match &frequency.cadence {
        FrequencyCadenceAst::Elapsed { interval, anchor } => {
            writeln!(
                &mut output,
                "  every elapsed {};",
                format_duration_binding(interval)
            )
            .expect("String write");
            writeln!(
                &mut output,
                "  anchor timestamp({});",
                json_string(&anchor.to_string())
            )
            .expect("String write");
        }
        FrequencyCadenceAst::Calendar {
            cadence,
            anchor,
            timezone,
            tzdb,
            gap,
            fold,
        } => {
            writeln!(&mut output, "  every {};", format_cadence_ast(cadence))
                .expect("String write");
            writeln!(
                &mut output,
                "  anchor civil({});",
                json_string(&anchor.to_string())
            )
            .expect("String write");
            writeln!(&mut output, "  timezone {};", timezone.as_str()).expect("String write");
            writeln!(
                &mut output,
                "  tzdb version {} digest {};",
                json_string(tzdb.version.as_str()),
                json_string(tzdb.digest.as_str())
            )
            .expect("String write");
            writeln!(&mut output, "  gap {};", enum_atom(gap)).expect("String write");
            writeln!(&mut output, "  fold {};", enum_atom(fold)).expect("String write");
        }
    }
    writeln!(&mut output, "  timer {{").expect("String write");
    writeln!(
        &mut output,
        "    resolution {};",
        format_duration_binding(&frequency.timer.required_resolution)
    )
    .expect("String write");
    writeln!(
        &mut output,
        "    max-lateness {};",
        format_duration_binding(&frequency.timer.max_lateness)
    )
    .expect("String write");
    writeln!(
        &mut output,
        "    coalesce-window {};",
        format_duration_binding(&frequency.timer.coalesce_window)
    )
    .expect("String write");
    writeln!(&mut output, "  }}").expect("String write");
    writeln!(
        &mut output,
        "  missed {};",
        format_missed_policy(frequency.missed)
    )
    .expect("String write");
    writeln!(
        &mut output,
        "  inactive-gap {};",
        enum_atom(&frequency.inactive_gap)
    )
    .expect("String write");
    writeln!(&mut output, "  rephase {};", enum_atom(&frequency.rephase)).expect("String write");
    writeln!(
        &mut output,
        "  overload {};",
        enum_atom(&frequency.overload)
    )
    .expect("String write");
    writeln!(&mut output, "}}").expect("String write");
    output
}

pub fn parse_frequency(source: &str) -> Result<FrequencyAst, KarmaDslError> {
    if source.len() > MAX_DSL_BYTES {
        return Err(KarmaDslError {
            kind: DslErrorKind::SourceTooLarge,
            offset: 0,
            line: 1,
            column: 1,
            message: format!("DSL source exceeds {MAX_DSL_BYTES} bytes"),
        });
    }
    let tokens = lex(source)?;
    Parser::new(tokens, source).parse_frequency()
}

fn format_duration_binding(binding: &DurationBinding) -> String {
    match binding {
        DurationBinding::Literal { value } => format!("duration({})", value.get()),
        DurationBinding::Parameter { parameter } => {
            format!("parameter({})", parameter.as_str())
        }
    }
}

fn format_positive_integer_binding(binding: &PositiveIntegerBinding) -> String {
    match binding {
        PositiveIntegerBinding::Literal { value } => format!("integer({})", value.get()),
        PositiveIntegerBinding::Parameter { parameter } => {
            format!("parameter({})", parameter.as_str())
        }
    }
}

fn format_cadence_ast(cadence: &CadenceAst) -> String {
    let mut components = Vec::new();
    for (name, binding) in [
        ("years", &cadence.every.years),
        ("months", &cadence.every.months),
        ("weeks", &cadence.every.weeks),
        ("days", &cadence.every.days),
        ("hours", &cadence.every.hours),
        ("minutes", &cadence.every.minutes),
        ("seconds", &cadence.every.seconds),
        ("milliseconds", &cadence.every.milliseconds),
    ] {
        if let Some(binding) = binding {
            components.push(format!(
                "{name}({})",
                format_positive_integer_binding(binding)
            ));
        }
    }
    let mut output = format!("calendar(step({})", components.join(", "));
    if let Some(weekdays) = &cadence.land_on {
        let mut days = String::new();
        write_joined(&mut days, weekdays.iter().map(|day| enum_atom(&day)));
        output.push_str(&format!(", land([{days}])"));
    }
    output.push_str(&format!(
        ", invalid-day({})",
        enum_atom(&cadence.invalid_day)
    ));
    output.push_str(&format!(
        ", bound({})",
        format_cadence_bound(&cadence.bound)
    ));
    output.push(')');
    output
}

fn format_cadence_bound(bound: &CadenceBound) -> String {
    match bound {
        CadenceBound::Unbounded => "unbounded".to_string(),
        CadenceBound::Count { occurrences } => format!("count({occurrences})"),
        CadenceBound::Until { at } => format!("until({})", json_string(&at.to_string())),
    }
}

fn format_missed_policy(policy: MissedPolicy) -> String {
    match policy {
        MissedPolicy::Skip => "skip".to_string(),
        MissedPolicy::Coalesce => "coalesce".to_string(),
        MissedPolicy::Replay { max } => format!("replay({})", max.get()),
        MissedPolicy::PauseOnLag => "pause-on-lag".to_string(),
    }
}

fn format_operation(output: &mut String, operation: &NodeOperation) {
    match operation {
        NodeOperation::Trigger {
            source,
            output: port,
        } => {
            writeln!(
                output,
                "    op trigger({}, {});",
                format_trigger_source(source),
                port.as_str()
            )
            .expect("String write");
        }
        NodeOperation::Input {
            source,
            output: port,
        } => {
            writeln!(
                output,
                "    op input({}, {});",
                format_input_source(source),
                port.as_str()
            )
            .expect("String write");
        }
        NodeOperation::Derive { expressions } => {
            writeln!(output, "    op derive {{").expect("String write");
            for (port, expression) in expressions {
                writeln!(
                    output,
                    "      expr {} = {};",
                    port.as_str(),
                    format_expression(expression)
                )
                .expect("String write");
            }
            writeln!(output, "    }}").expect("String write");
        }
        NodeOperation::Delay {
            input,
            output: port,
            initial,
            state,
        } => {
            writeln!(
                output,
                "    op delay({}, {}, {}, state({}, {}, {}, {}, {}));",
                input.as_str(),
                port.as_str(),
                format_literal(initial),
                enum_atom(&state.persistence),
                enum_atom(&state.reset),
                enum_atom(&state.late_event),
                enum_atom(&state.migration),
                enum_atom(&state.simulation)
            )
            .expect("String write");
        }
        NodeOperation::Threshold {
            input,
            active,
            entered,
            left,
            direction,
            enter,
            exit,
            initial_active,
            state,
        } => writeln!(
            output,
            "    op threshold({}, {}, {}, {}, {}, {}, {}, {}, {});",
            input.as_str(),
            active.as_str(),
            entered.as_str(),
            left.as_str(),
            enum_atom(direction),
            format_literal(enter),
            format_literal(exit),
            initial_active,
            format_state_contract(state)
        )
        .expect("String write"),
        NodeOperation::Debounce {
            input,
            stable,
            entered,
            left,
            for_at_least,
            initial,
            state,
        } => writeln!(
            output,
            "    op debounce({}, {}, {}, {}, duration({}), {}, {});",
            input.as_str(),
            stable.as_str(),
            entered.as_str(),
            left.as_str(),
            for_at_least.get(),
            initial,
            format_state_contract(state)
        )
        .expect("String write"),
        NodeOperation::Cooldown {
            input,
            allowed,
            cooldown,
            state,
        } => writeln!(
            output,
            "    op cooldown({}, {}, duration({}), {});",
            input.as_str(),
            allowed.as_str(),
            cooldown.get(),
            format_state_contract(state)
        )
        .expect("String write"),
        NodeOperation::RateLimit {
            input,
            allowed,
            max,
            window,
            state,
        } => writeln!(
            output,
            "    op rate-limit({}, {}, {}, duration({}), {});",
            input.as_str(),
            allowed.as_str(),
            max,
            window.get(),
            format_state_contract(state)
        )
        .expect("String write"),
        NodeOperation::RouteCandidate {
            condition,
            output: port,
            route,
            template,
            fields,
        } => {
            write!(
                output,
                "    op route-candidate({}, {}, {}, {}, {{",
                condition.as_str(),
                port.as_str(),
                enum_atom(route),
                template.as_str()
            )
            .expect("String write");
            write_joined(
                output,
                fields
                    .iter()
                    .map(|(field, input)| format!("{} = {}", field.as_str(), input.as_str())),
            );
            writeln!(output, "}});").expect("String write");
        }
    }
}

fn format_state_contract(state: &StateContract) -> String {
    format!(
        "state({}, {}, {}, {}, {})",
        enum_atom(&state.persistence),
        enum_atom(&state.reset),
        enum_atom(&state.late_event),
        enum_atom(&state.migration),
        enum_atom(&state.simulation)
    )
}

fn format_trigger_source(source: &TriggerSource) -> String {
    match source {
        TriggerSource::Manual => "manual".to_string(),
        TriggerSource::Fact { record, concept } => format!(
            "fact({}, {})",
            format_optional_reference(record.as_ref()),
            format_optional_reference(concept.as_ref())
        ),
        TriggerSource::Frequency { frequency } => {
            format!("frequency({})", format_reference(frequency))
        }
        TriggerSource::Signal { signal } => {
            format!("signal({})", format_reference(signal))
        }
        TriggerSource::Decision { decision } => {
            format!("decision({})", format_reference(decision))
        }
        TriggerSource::Receipt { receipt } => {
            format!("receipt({})", format_reference(receipt))
        }
        TriggerSource::Sync => "sync".to_string(),
    }
}

fn format_input_source(source: &InputSource) -> String {
    match source {
        InputSource::Parameter { parameter } => format!("parameter({})", parameter.as_str()),
        InputSource::RecordQuantity { record } => {
            format!("record-quantity({})", format_reference(record))
        }
        InputSource::SavedProtein { view } => {
            format!("saved-protein({})", format_reference(view))
        }
        InputSource::Signal { signal } => {
            format!("signal({})", format_reference(signal))
        }
        InputSource::CapturedFact { fact } => {
            format!("captured-fact({})", format_reference(fact))
        }
    }
}

fn format_expression(expression: &ExpressionAst) -> String {
    match expression {
        ExpressionAst::Literal { value } => format!("literal({})", format_literal(value)),
        ExpressionAst::Input { input } => format!("input({})", input.as_str()),
        ExpressionAst::Unary { operator, value } => format!(
            "unary({}, {})",
            enum_atom(operator),
            format_expression(value)
        ),
        ExpressionAst::Binary {
            operator,
            left,
            right,
            precision,
        } => {
            let mut rendered = format!(
                "binary({}, {}, {}",
                enum_atom(operator),
                format_expression(left),
                format_expression(right)
            );
            if let Some(precision) = precision {
                rendered.push_str(", ");
                rendered.push_str(&format_precision(precision));
            }
            rendered.push(')');
            rendered
        }
        ExpressionAst::If {
            condition,
            then_value,
            else_value,
        } => format!(
            "if({}, {}, {})",
            format_expression(condition),
            format_expression(then_value),
            format_expression(else_value)
        ),
    }
}

fn format_type(value_type: &ValueType) -> String {
    match value_type {
        ValueType::Bool => "bool".to_string(),
        ValueType::I64 => "i64".to_string(),
        ValueType::Decimal { scale } => format!("decimal({scale})"),
        ValueType::Probability => "prob".to_string(),
        ValueType::Confidence => "conf".to_string(),
        ValueType::Text => "text".to_string(),
        ValueType::Duration => "duration".to_string(),
        ValueType::Timestamp => "timestamp".to_string(),
        ValueType::Quantity { scale, unit } => {
            format!("quantity({scale}, {})", format_uid(unit))
        }
        ValueType::Reference { target } => format!("reference({})", enum_atom(target)),
        ValueType::List { item } => format!("list({})", format_type(item)),
        ValueType::Set { item } => format!("set({})", format_type(item)),
        ValueType::Map { key, value } => {
            format!("map({}, {})", format_type(key), format_type(value))
        }
        ValueType::Datum { value } => format!("datum({})", format_type(value)),
        ValueType::Estimate { value } => format!("estimate({})", format_type(value)),
        ValueType::Candidate {
            route,
            template,
            fields,
        } => {
            let mut output = format!("candidate({}, {}, {{", enum_atom(route), template.as_str());
            write_joined(
                &mut output,
                fields.iter().map(|(field, value_type)| {
                    format!("{}: {}", field.as_str(), format_type(value_type))
                }),
            );
            output.push_str("})");
            output
        }
    }
}

fn format_literal(literal: &LiteralValue) -> String {
    match literal {
        LiteralValue::Bool { value } => format!("bool({value})"),
        LiteralValue::I64 { value } => format!("i64({value})"),
        LiteralValue::Decimal { value } => {
            format!(
                "decimal({}, {})",
                value.scale(),
                json_string(&value.to_string())
            )
        }
        LiteralValue::Probability { value } => format!("prob({})", json_string(&value.to_string())),
        LiteralValue::Confidence { value } => format!("conf({})", json_string(&value.to_string())),
        LiteralValue::Text { value } => format!("text({})", json_string(value)),
        LiteralValue::Duration { value } => format!("duration({})", value.get()),
        LiteralValue::Timestamp { value } => {
            format!("timestamp({})", json_string(&value.to_string()))
        }
        LiteralValue::Quantity { amount, unit } => format!(
            "quantity({}, {}, {})",
            amount.scale(),
            json_string(&amount.to_string()),
            format_uid(unit)
        ),
        LiteralValue::Reference { value } => format!("reference({})", format_reference(value)),
        LiteralValue::Datum {
            value_type,
            state,
            value,
        } => match value {
            Some(value) => format!(
                "datum({}, {}, {})",
                format_type(value_type),
                enum_atom(state),
                format_literal(value)
            ),
            None => format!("datum({}, {})", format_type(value_type), enum_atom(state)),
        },
        LiteralValue::Candidate {
            route,
            template,
            fields,
        } => {
            let mut output = format!("candidate({}, {}, {{", enum_atom(route), template.as_str());
            write_joined(
                &mut output,
                fields.iter().map(|(field, value)| {
                    format!("{} = {}", field.as_str(), format_literal(value))
                }),
            );
            output.push_str("})");
            output
        }
    }
}

fn format_uid(uid: &TypedUid) -> String {
    format!("uid({}, {})", enum_atom(&uid.kind()), uid.as_str())
}

fn format_precision(precision: &DecimalPrecision) -> String {
    let mut rendered = format!(
        "precision({}, {}",
        precision.scale,
        enum_atom(&precision.rounding)
    );
    match &precision.result_unit {
        Some(DeclaredUnit::Dimensionless) => rendered.push_str(", dimensionless"),
        Some(DeclaredUnit::Unit { unit }) => {
            rendered.push_str(", ");
            rendered.push_str(&format_uid(unit));
        }
        None => {}
    }
    rendered.push(')');
    rendered
}

fn format_reference(reference: &ResolvedReference) -> String {
    format!(
        "ref({}, {}, {})",
        enum_atom(&reference.target.kind()),
        reference.target.as_str(),
        reference.display_slug.as_ref().map_or("none", Slug::as_str)
    )
}

fn format_optional_reference(reference: Option<&ResolvedReference>) -> String {
    reference.map_or_else(|| "none".to_string(), format_reference)
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("serializing a Rust string cannot fail")
}

fn enum_atom<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .expect("serializing a closed enum cannot fail")
        .as_str()
        .expect("closed enum must serialize as an atom")
        .to_string()
}

fn write_joined<I, S>(output: &mut String, values: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    for (index, value) in values.into_iter().enumerate() {
        if index != 0 {
            output.push_str(", ");
        }
        output.push_str(value.as_ref());
    }
}

fn lex(source: &str) -> Result<Vec<Token>, KarmaDslError> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut offset = 0;
    let mut line = 1;
    let mut column = 1;
    while offset < bytes.len() {
        match bytes[offset] {
            b' ' | b'\t' | b'\r' => {
                advance_byte(bytes[offset], &mut offset, &mut line, &mut column)
            }
            b'\n' => advance_byte(bytes[offset], &mut offset, &mut line, &mut column),
            b'#' => {
                while offset < bytes.len() && bytes[offset] != b'\n' {
                    advance_byte(bytes[offset], &mut offset, &mut line, &mut column);
                }
            }
            b'"' => {
                let start = (offset, line, column);
                let mut end = offset + 1;
                let mut escaped = false;
                while end < bytes.len() {
                    let byte = bytes[end];
                    if !escaped && byte == b'"' {
                        break;
                    }
                    if !escaped && byte == b'\\' {
                        escaped = true;
                    } else {
                        escaped = false;
                    }
                    end += 1;
                }
                if end == bytes.len() {
                    return Err(error_at(
                        DslErrorKind::UnexpectedEnd,
                        start,
                        "unterminated string literal",
                    ));
                }
                let encoded = &source[offset..=end];
                let decoded = serde_json::from_str::<String>(encoded).map_err(|_| {
                    error_at(
                        DslErrorKind::InvalidAtom,
                        start,
                        "string must use valid JSON escaping",
                    )
                })?;
                if decoded.len() > MAX_DSL_STRING_BYTES {
                    return Err(error_at(
                        DslErrorKind::StringTooLarge,
                        start,
                        format!("decoded string exceeds {MAX_DSL_STRING_BYTES} bytes"),
                    ));
                }
                while offset <= end {
                    advance_byte(bytes[offset], &mut offset, &mut line, &mut column);
                }
                tokens.push(Token {
                    kind: TokenKind::String(decoded),
                    offset: start.0,
                    line: start.1,
                    column: start.2,
                });
            }
            byte if is_symbol(byte) => {
                tokens.push(Token {
                    kind: TokenKind::Symbol(char::from(byte)),
                    offset,
                    line,
                    column,
                });
                advance_byte(byte, &mut offset, &mut line, &mut column);
            }
            byte if byte.is_ascii() && !byte.is_ascii_control() => {
                let start = (offset, line, column);
                let word_start = offset;
                while offset < bytes.len()
                    && !bytes[offset].is_ascii_whitespace()
                    && bytes[offset] != b'#'
                    && !is_symbol(bytes[offset])
                {
                    if !bytes[offset].is_ascii() || bytes[offset].is_ascii_control() {
                        return Err(error_at(
                            DslErrorKind::UnexpectedCharacter,
                            (offset, line, column),
                            "semantic DSL tokens must be ASCII",
                        ));
                    }
                    advance_byte(bytes[offset], &mut offset, &mut line, &mut column);
                }
                tokens.push(Token {
                    kind: TokenKind::Word(source[word_start..offset].to_string()),
                    offset: start.0,
                    line: start.1,
                    column: start.2,
                });
            }
            _ => {
                return Err(error_at(
                    DslErrorKind::UnexpectedCharacter,
                    (offset, line, column),
                    "Unicode is allowed only inside string literals",
                ));
            }
        }
        if tokens.len() > MAX_DSL_TOKENS {
            return Err(error_at(
                DslErrorKind::TooManyTokens,
                (offset, line, column),
                format!("DSL exceeds {MAX_DSL_TOKENS} tokens"),
            ));
        }
    }
    Ok(tokens)
}

fn is_symbol(byte: u8) -> bool {
    matches!(
        byte,
        b'{' | b'}' | b'[' | b']' | b'(' | b')' | b',' | b';' | b':' | b'='
    )
}

fn advance_byte(byte: u8, offset: &mut usize, line: &mut usize, column: &mut usize) {
    *offset += 1;
    if byte == b'\n' {
        *line += 1;
        *column = 1;
    } else {
        *column += 1;
    }
}

fn error_at(
    kind: DslErrorKind,
    location: (usize, usize, usize),
    message: impl Into<String>,
) -> KarmaDslError {
    KarmaDslError {
        kind,
        offset: location.0,
        line: location.1,
        column: location.2,
        message: message.into(),
    }
}

struct Parser<'source> {
    tokens: Vec<Token>,
    index: usize,
    source: &'source str,
}

impl<'source> Parser<'source> {
    fn new(tokens: Vec<Token>, source: &'source str) -> Self {
        Self {
            tokens,
            index: 0,
            source,
        }
    }

    fn parse_program(mut self) -> Result<ProgramAst, KarmaDslError> {
        self.expect_word("karma")?;
        self.expect_word("1")?;
        self.expect_symbol(';')?;
        self.expect_word("program")?;
        let slug = Slug::new(self.take_word("program slug")?).map_err(|error| self.atom(error))?;
        self.expect_symbol('{')?;

        let mut schema = None;
        let mut purpose = None;
        let mut tags = None;
        let mut capabilities = None;
        let mut parameters = BTreeMap::new();
        let mut nodes = BTreeMap::new();
        let mut outputs = BTreeMap::new();

        while !self.consume_symbol('}') {
            let declaration = self.take_word("program declaration")?;
            match declaration.as_str() {
                "schema" => {
                    let value = self.parse_enum_atom::<ProgramSchema>("program schema")?;
                    self.set_once(&mut schema, value, "schema")?;
                    self.expect_symbol(';')?;
                }
                "purpose" => {
                    let value = self.take_string("program purpose")?;
                    self.set_once(&mut purpose, value, "purpose")?;
                    self.expect_symbol(';')?;
                }
                "tags" => {
                    let value = self.parse_slug_set()?;
                    self.set_once(&mut tags, value, "tags")?;
                    self.expect_symbol(';')?;
                }
                "capabilities" => {
                    let value = self.parse_capability_set()?;
                    self.set_once(&mut capabilities, value, "capabilities")?;
                    self.expect_symbol(';')?;
                }
                "param" => {
                    let (id, parameter) = self.parse_parameter()?;
                    insert_unique(&mut parameters, id, parameter, &self)?;
                }
                "node" => {
                    let (id, node) = self.parse_node()?;
                    insert_unique(&mut nodes, id, node, &self)?;
                }
                "output" => {
                    let id = self.parse_local_id("program output id")?;
                    self.expect_symbol('=')?;
                    let source = self.parse_output_ref()?;
                    self.expect_symbol(';')?;
                    insert_unique(&mut outputs, id, source, &self)?;
                }
                _ => return Err(self.unexpected(format!("unknown declaration {declaration:?}"))),
            }
        }
        if self.index != self.tokens.len() {
            return Err(
                self.error_here(DslErrorKind::TrailingInput, "trailing input after program")
            );
        }
        Ok(ProgramAst {
            schema: schema.ok_or_else(|| self.missing("schema"))?,
            slug,
            purpose: purpose.ok_or_else(|| self.missing("purpose"))?,
            tags: tags.ok_or_else(|| self.missing("tags"))?,
            parameters,
            nodes,
            outputs,
            required_capabilities: capabilities.ok_or_else(|| self.missing("capabilities"))?,
        })
    }

    fn parse_frequency(mut self) -> Result<FrequencyAst, KarmaDslError> {
        self.expect_word("karma-frequency")?;
        self.expect_word("1")?;
        self.expect_symbol(';')?;
        self.expect_word("frequency")?;
        let slug =
            Slug::new(self.take_word("frequency slug")?).map_err(|error| self.atom(error))?;
        self.expect_symbol('{')?;

        let mut schema = None;
        let mut purpose = None;
        let mut tags = None;
        let mut parameters = BTreeMap::new();
        let mut cadence = None;
        let mut anchor = None;
        let mut timezone = None;
        let mut tzdb = None;
        let mut gap = None;
        let mut fold = None;
        let mut timer = None;
        let mut missed = None;
        let mut inactive_gap = None;
        let mut rephase = None;
        let mut overload = None;

        while !self.consume_symbol('}') {
            let declaration = self.take_word("frequency declaration")?;
            match declaration.as_str() {
                "schema" => {
                    let value = self.parse_enum_atom::<FrequencySchema>("frequency schema")?;
                    self.set_once(&mut schema, value, "schema")?;
                    self.expect_symbol(';')?;
                }
                "purpose" => {
                    let value = self.take_string("frequency purpose")?;
                    self.set_once(&mut purpose, value, "purpose")?;
                    self.expect_symbol(';')?;
                }
                "tags" => {
                    let value = self.parse_slug_set()?;
                    self.set_once(&mut tags, value, "tags")?;
                    self.expect_symbol(';')?;
                }
                "param" => {
                    let (id, definition) = self.parse_frequency_parameter()?;
                    insert_unique(&mut parameters, id, definition, &self)?;
                }
                "every" => {
                    let value = self.parse_frequency_cadence()?;
                    self.set_once(&mut cadence, value, "cadence")?;
                    self.expect_symbol(';')?;
                }
                "anchor" => {
                    let value = self.parse_frequency_anchor()?;
                    self.set_once(&mut anchor, value, "anchor")?;
                    self.expect_symbol(';')?;
                }
                "timezone" => {
                    let value = TimeZoneId::new(self.take_word("timezone")?)
                        .map_err(|error| self.atom(error))?;
                    self.set_once(&mut timezone, value, "timezone")?;
                    self.expect_symbol(';')?;
                }
                "tzdb" => {
                    self.expect_word("version")?;
                    let version = TzdbVersion::new(self.take_string("tzdb version")?)
                        .map_err(|error| self.atom(error))?;
                    self.expect_word("digest")?;
                    let digest = CanonicalHash::parse(self.take_string("tzdb digest")?)
                        .map_err(|error| self.atom(error))?;
                    self.set_once(&mut tzdb, TzdbRevision { version, digest }, "tzdb")?;
                    self.expect_symbol(';')?;
                }
                "gap" => {
                    let value = self.parse_enum_atom::<GapPolicy>("gap policy")?;
                    self.set_once(&mut gap, value, "gap policy")?;
                    self.expect_symbol(';')?;
                }
                "fold" => {
                    let value = self.parse_enum_atom::<FoldPolicy>("fold policy")?;
                    self.set_once(&mut fold, value, "fold policy")?;
                    self.expect_symbol(';')?;
                }
                "timer" => {
                    let value = self.parse_frequency_timer()?;
                    self.set_once(&mut timer, value, "timer")?;
                }
                "missed" => {
                    let value = self.parse_missed_policy()?;
                    self.set_once(&mut missed, value, "missed policy")?;
                    self.expect_symbol(';')?;
                }
                "inactive-gap" => {
                    let value = self.parse_enum_atom::<InactiveGapPolicy>("inactive-gap policy")?;
                    self.set_once(&mut inactive_gap, value, "inactive-gap policy")?;
                    self.expect_symbol(';')?;
                }
                "rephase" => {
                    let value = self.parse_enum_atom::<RephasePolicy>("rephase policy")?;
                    self.set_once(&mut rephase, value, "rephase policy")?;
                    self.expect_symbol(';')?;
                }
                "overload" => {
                    let value = self.parse_enum_atom::<OverloadPolicy>("overload policy")?;
                    self.set_once(&mut overload, value, "overload policy")?;
                    self.expect_symbol(';')?;
                }
                _ => {
                    return Err(
                        self.unexpected(format!("unknown Frequency declaration {declaration:?}"))
                    );
                }
            }
        }
        if self.index != self.tokens.len() {
            return Err(self.error_here(
                DslErrorKind::TrailingInput,
                "trailing input after Frequency",
            ));
        }

        let cadence = match (
            cadence.ok_or_else(|| self.missing("cadence"))?,
            anchor.ok_or_else(|| self.missing("anchor"))?,
        ) {
            (
                ParsedFrequencyCadence::Elapsed(interval),
                ParsedFrequencyAnchor::Timestamp(anchor),
            ) => {
                if timezone.is_some() || tzdb.is_some() || gap.is_some() || fold.is_some() {
                    return Err(self.unexpected(
                        "elapsed Frequency cannot carry timezone/tzdb/gap/fold declarations",
                    ));
                }
                FrequencyCadenceAst::Elapsed { interval, anchor }
            }
            (ParsedFrequencyCadence::Calendar(cadence), ParsedFrequencyAnchor::Civil(anchor)) => {
                FrequencyCadenceAst::Calendar {
                    cadence,
                    anchor,
                    timezone: timezone.ok_or_else(|| self.missing("timezone"))?,
                    tzdb: tzdb.ok_or_else(|| self.missing("tzdb"))?,
                    gap: gap.ok_or_else(|| self.missing("gap policy"))?,
                    fold: fold.ok_or_else(|| self.missing("fold policy"))?,
                }
            }
            (ParsedFrequencyCadence::Elapsed(_), ParsedFrequencyAnchor::Civil(_)) => {
                return Err(self.unexpected("elapsed Frequency requires a UTC timestamp anchor"));
            }
            (ParsedFrequencyCadence::Calendar(_), ParsedFrequencyAnchor::Timestamp(_)) => {
                return Err(self.unexpected("calendar Frequency requires a civil anchor"));
            }
        };

        let frequency = FrequencyAst {
            schema: schema.ok_or_else(|| self.missing("schema"))?,
            slug,
            purpose: purpose.ok_or_else(|| self.missing("purpose"))?,
            tags: tags.ok_or_else(|| self.missing("tags"))?,
            parameters,
            cadence,
            timer: timer.ok_or_else(|| self.missing("timer"))?,
            missed: missed.ok_or_else(|| self.missing("missed policy"))?,
            inactive_gap: inactive_gap.ok_or_else(|| self.missing("inactive-gap policy"))?,
            rephase: rephase.ok_or_else(|| self.missing("rephase policy"))?,
            overload: overload.ok_or_else(|| self.missing("overload policy"))?,
        };
        frequency
            .compile(&BTreeMap::new())
            .map_err(|error| self.error_here(DslErrorKind::InvalidAtom, error.to_string()))?;
        Ok(frequency)
    }

    fn parse_frequency_parameter(
        &mut self,
    ) -> Result<(LocalId, FrequencyParameterDefinition), KarmaDslError> {
        let id = self.parse_local_id("Frequency parameter id")?;
        self.expect_symbol(':')?;
        let kind = self.take_word("Frequency parameter type")?;
        self.expect_word("default")?;
        let definition = match kind.as_str() {
            "duration" => {
                let default = self.parse_duration_literal()?;
                self.expect_word("range")?;
                self.expect_symbol('[')?;
                let minimum = self.parse_duration_literal()?;
                self.expect_symbol(',')?;
                let maximum = self.parse_duration_literal()?;
                self.expect_symbol(']')?;
                FrequencyParameterDefinition::Duration {
                    default,
                    minimum,
                    maximum,
                }
            }
            "positive-integer" => {
                let default = self.parse_positive_integer_literal()?;
                self.expect_word("range")?;
                self.expect_symbol('[')?;
                let minimum = self.parse_positive_integer_literal()?;
                self.expect_symbol(',')?;
                let maximum = self.parse_positive_integer_literal()?;
                self.expect_symbol(']')?;
                FrequencyParameterDefinition::PositiveInteger {
                    default,
                    minimum,
                    maximum,
                }
            }
            _ => {
                return Err(self.unexpected(format!("unknown Frequency parameter type {kind:?}")));
            }
        };
        self.expect_symbol(';')?;
        Ok((id, definition))
    }

    fn parse_frequency_cadence(&mut self) -> Result<ParsedFrequencyCadence, KarmaDslError> {
        match self.take_word("Frequency cadence kind")?.as_str() {
            "elapsed" => self
                .parse_duration_binding()
                .map(ParsedFrequencyCadence::Elapsed),
            "calendar" => self
                .parse_cadence_ast()
                .map(ParsedFrequencyCadence::Calendar),
            value => Err(self.unexpected(format!("unknown Frequency cadence {value:?}"))),
        }
    }

    fn parse_frequency_anchor(&mut self) -> Result<ParsedFrequencyAnchor, KarmaDslError> {
        match self.take_word("Frequency anchor kind")?.as_str() {
            "timestamp" => {
                self.expect_symbol('(')?;
                let value = TimestampMs::parse_canonical(&self.take_string("UTC anchor")?)
                    .map_err(|error| self.atom(error))?;
                self.expect_symbol(')')?;
                Ok(ParsedFrequencyAnchor::Timestamp(value))
            }
            "civil" => {
                self.expect_symbol('(')?;
                let value = CivilDateTime::parse_canonical(&self.take_string("civil anchor")?)
                    .map_err(|error| self.atom(error))?;
                self.expect_symbol(')')?;
                Ok(ParsedFrequencyAnchor::Civil(value))
            }
            value => Err(self.unexpected(format!("unknown Frequency anchor {value:?}"))),
        }
    }

    fn parse_frequency_timer(&mut self) -> Result<FrequencyTimerAst, KarmaDslError> {
        self.expect_symbol('{')?;
        let mut resolution = None;
        let mut max_lateness = None;
        let mut coalesce_window = None;
        while !self.consume_symbol('}') {
            let field = self.take_word("timer field")?;
            match field.as_str() {
                "resolution" => {
                    let value = self.parse_duration_binding()?;
                    self.set_once(&mut resolution, value, "timer resolution")?;
                }
                "max-lateness" => {
                    let value = self.parse_duration_binding()?;
                    self.set_once(&mut max_lateness, value, "timer maximum lateness")?;
                }
                "coalesce-window" => {
                    let value = self.parse_duration_binding()?;
                    self.set_once(&mut coalesce_window, value, "timer coalescing window")?;
                }
                _ => return Err(self.unexpected(format!("unknown timer field {field:?}"))),
            }
            self.expect_symbol(';')?;
        }
        Ok(FrequencyTimerAst {
            required_resolution: resolution.ok_or_else(|| self.missing("timer resolution"))?,
            max_lateness: max_lateness.ok_or_else(|| self.missing("timer maximum lateness"))?,
            coalesce_window: coalesce_window
                .ok_or_else(|| self.missing("timer coalescing window"))?,
        })
    }

    fn parse_duration_binding(&mut self) -> Result<DurationBinding, KarmaDslError> {
        match self.take_word("duration binding")?.as_str() {
            "duration" => {
                self.expect_symbol('(')?;
                let value = DurationMs::new(self.parse_i64("duration milliseconds")?);
                self.expect_symbol(')')?;
                Ok(DurationBinding::Literal { value })
            }
            "parameter" => {
                self.expect_symbol('(')?;
                let parameter = self.parse_local_id("duration parameter")?;
                self.expect_symbol(')')?;
                Ok(DurationBinding::Parameter { parameter })
            }
            value => Err(self.unexpected(format!("unknown duration binding {value:?}"))),
        }
    }

    fn parse_positive_integer_binding(&mut self) -> Result<PositiveIntegerBinding, KarmaDslError> {
        match self.take_word("positive-integer binding")?.as_str() {
            "integer" => self
                .parse_positive_integer_argument()
                .map(|value| PositiveIntegerBinding::Literal { value }),
            "parameter" => {
                self.expect_symbol('(')?;
                let parameter = self.parse_local_id("positive-integer parameter")?;
                self.expect_symbol(')')?;
                Ok(PositiveIntegerBinding::Parameter { parameter })
            }
            value => Err(self.unexpected(format!("unknown positive-integer binding {value:?}"))),
        }
    }

    fn parse_duration_literal(&mut self) -> Result<DurationMs, KarmaDslError> {
        self.expect_word("duration")?;
        self.expect_symbol('(')?;
        let value = DurationMs::new(self.parse_i64("duration milliseconds")?);
        self.expect_symbol(')')?;
        Ok(value)
    }

    fn parse_positive_integer_literal(&mut self) -> Result<NonZeroU32, KarmaDslError> {
        self.expect_word("integer")?;
        self.parse_positive_integer_argument()
    }

    fn parse_positive_integer_argument(&mut self) -> Result<NonZeroU32, KarmaDslError> {
        self.expect_symbol('(')?;
        let value = self.take_word("positive integer")?;
        let value = value.parse::<u32>().map_err(|_| {
            self.error_here(DslErrorKind::InvalidAtom, "positive integer must fit u32")
        })?;
        let value = NonZeroU32::new(value).ok_or_else(|| {
            self.error_here(
                DslErrorKind::InvalidAtom,
                "positive integer must be non-zero",
            )
        })?;
        self.expect_symbol(')')?;
        Ok(value)
    }

    fn parse_cadence_ast(&mut self) -> Result<CadenceAst, KarmaDslError> {
        self.expect_symbol('(')?;
        self.expect_word("step")?;
        self.expect_symbol('(')?;
        let mut every = CadenceStepAst::default();
        if !self.consume_symbol(')') {
            loop {
                let component = self.take_word("step component")?;
                self.expect_symbol('(')?;
                let binding = self.parse_positive_integer_binding()?;
                self.expect_symbol(')')?;
                let slot = match component.as_str() {
                    "years" => &mut every.years,
                    "months" => &mut every.months,
                    "weeks" => &mut every.weeks,
                    "days" => &mut every.days,
                    "hours" => &mut every.hours,
                    "minutes" => &mut every.minutes,
                    "seconds" => &mut every.seconds,
                    "milliseconds" => &mut every.milliseconds,
                    other => {
                        return Err(self.unexpected(format!("unknown step component {other:?}")));
                    }
                };
                if slot.is_some() {
                    return Err(self.duplicate("step component"));
                }
                *slot = Some(binding);
                if !self.consume_symbol(',') {
                    break;
                }
            }
            self.expect_symbol(')')?;
        }
        self.expect_symbol(',')?;
        let land_on = if self.peek_word_is("land") {
            self.expect_word("land")?;
            self.expect_symbol('(')?;
            let days = self.parse_weekday_set()?;
            self.expect_symbol(')')?;
            self.expect_symbol(',')?;
            Some(days)
        } else {
            None
        };
        self.expect_word("invalid-day")?;
        self.expect_symbol('(')?;
        let invalid_day = self.parse_enum_atom::<InvalidDay>("invalid month-day policy")?;
        self.expect_symbol(')')?;
        self.expect_symbol(',')?;
        self.expect_word("bound")?;
        self.expect_symbol('(')?;
        let bound = self.parse_cadence_bound()?;
        self.expect_symbol(')')?;
        self.expect_symbol(')')?;
        Ok(CadenceAst {
            every,
            land_on,
            invalid_day,
            bound,
        })
    }

    fn parse_cadence_bound(&mut self) -> Result<CadenceBound, KarmaDslError> {
        match self.take_word("cadence bound")?.as_str() {
            "unbounded" => Ok(CadenceBound::Unbounded),
            "count" => {
                self.expect_symbol('(')?;
                let occurrences = self.parse_u64("occurrence count")?;
                self.expect_symbol(')')?;
                Ok(CadenceBound::Count { occurrences })
            }
            "until" => {
                self.expect_symbol('(')?;
                let at = CivilDateTime::parse_canonical(&self.take_string("bound instant")?)
                    .map_err(|error| self.atom(error))?;
                self.expect_symbol(')')?;
                Ok(CadenceBound::Until { at })
            }
            other => Err(self.unexpected(format!("unknown cadence bound {other:?}"))),
        }
    }

    fn parse_weekday_set(&mut self) -> Result<WeekdaySet, KarmaDslError> {
        self.expect_symbol('[')?;
        let mut days = BTreeSet::new();
        if self.consume_symbol(']') {
            return WeekdaySet::new(days).map_err(|error| self.atom(error));
        }
        loop {
            let day = self.parse_enum_atom::<CivilWeekday>("civil weekday")?;
            if !days.insert(day) {
                return Err(self.duplicate("civil weekday"));
            }
            if self.consume_symbol(']') {
                return WeekdaySet::new(days).map_err(|error| self.atom(error));
            }
            self.expect_symbol(',')?;
        }
    }

    fn parse_missed_policy(&mut self) -> Result<MissedPolicy, KarmaDslError> {
        match self.take_word("missed policy")?.as_str() {
            "skip" => Ok(MissedPolicy::Skip),
            "coalesce" => Ok(MissedPolicy::Coalesce),
            "pause-on-lag" => Ok(MissedPolicy::PauseOnLag),
            "replay" => {
                self.expect_symbol('(')?;
                let max = self.take_word("replay maximum")?;
                let max = max.parse::<u32>().map_err(|_| {
                    self.error_here(DslErrorKind::InvalidAtom, "replay maximum must fit u32")
                })?;
                let max = NonZeroU32::new(max).ok_or_else(|| {
                    self.error_here(DslErrorKind::InvalidAtom, "replay maximum must be non-zero")
                })?;
                self.expect_symbol(')')?;
                Ok(MissedPolicy::Replay { max })
            }
            value => Err(self.unexpected(format!("unknown missed policy {value:?}"))),
        }
    }

    fn parse_parameter(&mut self) -> Result<(LocalId, ParameterDefinition), KarmaDslError> {
        let id = self.parse_local_id("parameter id")?;
        self.expect_symbol(':')?;
        let value_type = self.parse_type(0)?;
        let mutable = match self.take_word("parameter mutability")?.as_str() {
            "mutable" => true,
            "fixed" => false,
            value => return Err(self.unexpected(format!("unknown parameter mutability {value:?}"))),
        };
        self.expect_symbol('=')?;
        let default = self.parse_literal(0)?;
        self.expect_symbol(';')?;
        Ok((
            id,
            ParameterDefinition {
                value_type,
                default,
                mutable,
            },
        ))
    }

    fn parse_node(&mut self) -> Result<(LocalId, NodeAst), KarmaDslError> {
        let id = self.parse_local_id("node id")?;
        self.expect_symbol('{')?;
        let mut inputs = BTreeMap::new();
        let mut outputs = BTreeMap::new();
        let mut operation = None;
        while !self.consume_symbol('}') {
            let declaration = self.take_word("node declaration")?;
            match declaration.as_str() {
                "bind" => {
                    let input_id = self.parse_local_id("input binding id")?;
                    self.expect_symbol(':')?;
                    let expected_type = self.parse_type(0)?;
                    self.expect_symbol('=')?;
                    let source = self.parse_output_ref()?;
                    self.expect_symbol(';')?;
                    insert_unique(
                        &mut inputs,
                        input_id,
                        InputBinding {
                            source,
                            expected_type,
                        },
                        self,
                    )?;
                }
                "port" => {
                    let port_id = self.parse_local_id("output port id")?;
                    self.expect_symbol(':')?;
                    let value_type = self.parse_type(0)?;
                    self.expect_word("sensitivity")?;
                    let sensitivity = self.parse_enum_atom::<Sensitivity>("sensitivity")?;
                    self.expect_word("freshness")?;
                    let freshness = if self.peek_word("none") {
                        self.index += 1;
                        None
                    } else {
                        self.expect_word("duration")?;
                        self.expect_symbol('(')?;
                        let value = self.parse_i64("freshness milliseconds")?;
                        self.expect_symbol(')')?;
                        Some(DurationMs::new(value))
                    };
                    self.expect_symbol(';')?;
                    insert_unique(
                        &mut outputs,
                        port_id,
                        PortContract {
                            value_type,
                            sensitivity,
                            freshness,
                        },
                        self,
                    )?;
                }
                "op" => {
                    let value = self.parse_operation()?;
                    self.set_once(&mut operation, value, "node operation")?;
                }
                _ => {
                    return Err(
                        self.unexpected(format!("unknown node declaration {declaration:?}"))
                    );
                }
            }
        }
        Ok((
            id,
            NodeAst {
                inputs,
                outputs,
                operation: operation.ok_or_else(|| self.missing("node operation"))?,
            },
        ))
    }

    fn parse_operation(&mut self) -> Result<NodeOperation, KarmaDslError> {
        let kind = self.take_word("node operation kind")?;
        match kind.as_str() {
            "trigger" => {
                self.expect_symbol('(')?;
                let source = self.parse_trigger_source()?;
                self.expect_symbol(',')?;
                let output = self.parse_local_id("trigger output")?;
                self.expect_symbol(')')?;
                self.expect_symbol(';')?;
                Ok(NodeOperation::Trigger { source, output })
            }
            "input" => {
                self.expect_symbol('(')?;
                let source = self.parse_input_source()?;
                self.expect_symbol(',')?;
                let output = self.parse_local_id("input output")?;
                self.expect_symbol(')')?;
                self.expect_symbol(';')?;
                Ok(NodeOperation::Input { source, output })
            }
            "derive" => {
                self.expect_symbol('{')?;
                let mut expressions = BTreeMap::new();
                while !self.consume_symbol('}') {
                    self.expect_word("expr")?;
                    let output = self.parse_local_id("derive output")?;
                    self.expect_symbol('=')?;
                    let expression = self.parse_expression(0)?;
                    self.expect_symbol(';')?;
                    insert_unique(&mut expressions, output, expression, self)?;
                }
                Ok(NodeOperation::Derive { expressions })
            }
            "delay" => {
                self.expect_symbol('(')?;
                let input = self.parse_local_id("delay input")?;
                self.expect_symbol(',')?;
                let output = self.parse_local_id("delay output")?;
                self.expect_symbol(',')?;
                let initial = self.parse_literal(0)?;
                self.expect_symbol(',')?;
                let state = self.parse_state_contract()?;
                self.expect_symbol(')')?;
                self.expect_symbol(';')?;
                Ok(NodeOperation::Delay {
                    input,
                    output,
                    initial,
                    state,
                })
            }
            "threshold" => {
                self.expect_symbol('(')?;
                let input = self.parse_local_id("threshold input")?;
                self.expect_symbol(',')?;
                let active = self.parse_local_id("threshold active output")?;
                self.expect_symbol(',')?;
                let entered = self.parse_local_id("threshold entered output")?;
                self.expect_symbol(',')?;
                let left = self.parse_local_id("threshold left output")?;
                self.expect_symbol(',')?;
                let direction =
                    self.parse_enum_atom::<ThresholdDirection>("threshold direction")?;
                self.expect_symbol(',')?;
                let enter = self.parse_literal(0)?;
                self.expect_symbol(',')?;
                let exit = self.parse_literal(0)?;
                self.expect_symbol(',')?;
                let initial_active = self.parse_bool()?;
                self.expect_symbol(',')?;
                let state = self.parse_state_contract()?;
                self.expect_symbol(')')?;
                self.expect_symbol(';')?;
                Ok(NodeOperation::Threshold {
                    input,
                    active,
                    entered,
                    left,
                    direction,
                    enter,
                    exit,
                    initial_active,
                    state,
                })
            }
            "debounce" => {
                self.expect_symbol('(')?;
                let input = self.parse_local_id("debounce input")?;
                self.expect_symbol(',')?;
                let stable = self.parse_local_id("debounce stable output")?;
                self.expect_symbol(',')?;
                let entered = self.parse_local_id("debounce entered output")?;
                self.expect_symbol(',')?;
                let left = self.parse_local_id("debounce left output")?;
                self.expect_symbol(',')?;
                let for_at_least = self.parse_duration_constructor("debounce duration")?;
                self.expect_symbol(',')?;
                let initial = self.parse_bool()?;
                self.expect_symbol(',')?;
                let state = self.parse_state_contract()?;
                self.expect_symbol(')')?;
                self.expect_symbol(';')?;
                Ok(NodeOperation::Debounce {
                    input,
                    stable,
                    entered,
                    left,
                    for_at_least,
                    initial,
                    state,
                })
            }
            "cooldown" => {
                self.expect_symbol('(')?;
                let input = self.parse_local_id("cooldown input")?;
                self.expect_symbol(',')?;
                let allowed = self.parse_local_id("cooldown allowed output")?;
                self.expect_symbol(',')?;
                let cooldown = self.parse_duration_constructor("cooldown duration")?;
                self.expect_symbol(',')?;
                let state = self.parse_state_contract()?;
                self.expect_symbol(')')?;
                self.expect_symbol(';')?;
                Ok(NodeOperation::Cooldown {
                    input,
                    allowed,
                    cooldown,
                    state,
                })
            }
            "rate-limit" => {
                self.expect_symbol('(')?;
                let input = self.parse_local_id("rate-limit input")?;
                self.expect_symbol(',')?;
                let allowed = self.parse_local_id("rate-limit allowed output")?;
                self.expect_symbol(',')?;
                let max = self.parse_u32("rate-limit maximum")?;
                self.expect_symbol(',')?;
                let window = self.parse_duration_constructor("rate-limit window")?;
                self.expect_symbol(',')?;
                let state = self.parse_state_contract()?;
                self.expect_symbol(')')?;
                self.expect_symbol(';')?;
                Ok(NodeOperation::RateLimit {
                    input,
                    allowed,
                    max,
                    window,
                    state,
                })
            }
            "route-candidate" => {
                self.expect_symbol('(')?;
                let condition = self.parse_local_id("candidate condition")?;
                self.expect_symbol(',')?;
                let output = self.parse_local_id("candidate output")?;
                self.expect_symbol(',')?;
                let route = self.parse_enum_atom::<CandidateRoute>("candidate route")?;
                self.expect_symbol(',')?;
                let template = Slug::new(self.take_word("candidate template")?)
                    .map_err(|error| self.atom(error))?;
                self.expect_symbol(',')?;
                let fields = self.parse_candidate_input_fields()?;
                self.expect_symbol(')')?;
                self.expect_symbol(';')?;
                Ok(NodeOperation::RouteCandidate {
                    condition,
                    output,
                    route,
                    template,
                    fields,
                })
            }
            _ => Err(self.unexpected(format!("unknown node operation {kind:?}"))),
        }
    }

    fn parse_duration_constructor(&mut self, label: &str) -> Result<DurationMs, KarmaDslError> {
        self.expect_word("duration")?;
        self.expect_symbol('(')?;
        let duration = DurationMs::new(self.parse_i64(label)?);
        self.expect_symbol(')')?;
        Ok(duration)
    }

    fn parse_candidate_input_fields(
        &mut self,
    ) -> Result<BTreeMap<LocalId, LocalId>, KarmaDslError> {
        self.expect_symbol('{')?;
        let mut fields = BTreeMap::new();
        if self.consume_symbol('}') {
            return Ok(fields);
        }
        loop {
            let field = self.parse_local_id("candidate field")?;
            self.expect_symbol('=')?;
            let input = self.parse_local_id("candidate field input")?;
            insert_unique(&mut fields, field, input, self)?;
            if self.consume_symbol('}') {
                return Ok(fields);
            }
            self.expect_symbol(',')?;
        }
    }

    fn parse_state_contract(&mut self) -> Result<StateContract, KarmaDslError> {
        self.expect_word("state")?;
        self.expect_symbol('(')?;
        let persistence = self.parse_enum_atom::<StatePersistence>("state persistence")?;
        self.expect_symbol(',')?;
        let reset = self.parse_enum_atom::<StateResetPolicy>("state reset policy")?;
        self.expect_symbol(',')?;
        let late_event = self.parse_enum_atom::<LateEventPolicy>("late event policy")?;
        self.expect_symbol(',')?;
        let migration = self.parse_enum_atom::<StateMigrationPolicy>("state migration policy")?;
        self.expect_symbol(',')?;
        let simulation =
            self.parse_enum_atom::<SimulationStatePolicy>("simulation state policy")?;
        self.expect_symbol(')')?;
        Ok(StateContract {
            persistence,
            reset,
            late_event,
            migration,
            simulation,
        })
    }

    fn parse_trigger_source(&mut self) -> Result<TriggerSource, KarmaDslError> {
        let kind = self.take_word("trigger source")?;
        match kind.as_str() {
            "manual" => Ok(TriggerSource::Manual),
            "sync" => Ok(TriggerSource::Sync),
            "fact" => {
                self.expect_symbol('(')?;
                let record = self.parse_optional_reference()?;
                self.expect_symbol(',')?;
                let concept = self.parse_optional_reference()?;
                self.expect_symbol(')')?;
                Ok(TriggerSource::Fact { record, concept })
            }
            "frequency" => Ok(TriggerSource::Frequency {
                frequency: self.parse_single_reference_argument()?,
            }),
            "signal" => Ok(TriggerSource::Signal {
                signal: self.parse_single_reference_argument()?,
            }),
            "decision" => Ok(TriggerSource::Decision {
                decision: self.parse_single_reference_argument()?,
            }),
            "receipt" => Ok(TriggerSource::Receipt {
                receipt: self.parse_single_reference_argument()?,
            }),
            _ => Err(self.unexpected(format!("unknown trigger source {kind:?}"))),
        }
    }

    fn parse_input_source(&mut self) -> Result<InputSource, KarmaDslError> {
        let kind = self.take_word("input source")?;
        match kind.as_str() {
            "parameter" => Ok(InputSource::Parameter {
                parameter: self.parse_single_local_id_argument("parameter")?,
            }),
            "record-quantity" => Ok(InputSource::RecordQuantity {
                record: self.parse_single_reference_argument()?,
            }),
            "saved-protein" => Ok(InputSource::SavedProtein {
                view: self.parse_single_reference_argument()?,
            }),
            "signal" => Ok(InputSource::Signal {
                signal: self.parse_single_reference_argument()?,
            }),
            "captured-fact" => Ok(InputSource::CapturedFact {
                fact: self.parse_single_reference_argument()?,
            }),
            _ => Err(self.unexpected(format!("unknown input source {kind:?}"))),
        }
    }

    fn parse_expression(&mut self, depth: usize) -> Result<ExpressionAst, KarmaDslError> {
        self.check_depth(depth)?;
        let kind = self.take_word("expression constructor")?;
        self.expect_symbol('(')?;
        let expression = match kind.as_str() {
            "literal" => ExpressionAst::Literal {
                value: self.parse_literal(depth + 1)?,
            },
            "input" => ExpressionAst::Input {
                input: self.parse_local_id("expression input")?,
            },
            "unary" => {
                let operator = self.parse_enum_atom::<UnaryOperator>("unary operator")?;
                self.expect_symbol(',')?;
                let value = Box::new(self.parse_expression(depth + 1)?);
                ExpressionAst::Unary { operator, value }
            }
            "binary" => {
                let operator = self.parse_enum_atom::<BinaryOperator>("binary operator")?;
                self.expect_symbol(',')?;
                let left = Box::new(self.parse_expression(depth + 1)?);
                self.expect_symbol(',')?;
                let right = Box::new(self.parse_expression(depth + 1)?);
                let precision = if self.consume_symbol(',') {
                    Some(self.parse_precision()?)
                } else {
                    None
                };
                ExpressionAst::Binary {
                    operator,
                    left,
                    right,
                    precision,
                }
            }
            "if" => {
                let condition = Box::new(self.parse_expression(depth + 1)?);
                self.expect_symbol(',')?;
                let then_value = Box::new(self.parse_expression(depth + 1)?);
                self.expect_symbol(',')?;
                let else_value = Box::new(self.parse_expression(depth + 1)?);
                ExpressionAst::If {
                    condition,
                    then_value,
                    else_value,
                }
            }
            _ => return Err(self.unexpected(format!("unknown expression constructor {kind:?}"))),
        };
        self.expect_symbol(')')?;
        Ok(expression)
    }

    fn parse_type(&mut self, depth: usize) -> Result<ValueType, KarmaDslError> {
        self.check_depth(depth)?;
        let kind = self.take_word("type")?;
        let value_type = match kind.as_str() {
            "bool" => Ok(ValueType::Bool),
            "i64" => Ok(ValueType::I64),
            "prob" => Ok(ValueType::Probability),
            "conf" => Ok(ValueType::Confidence),
            "text" => Ok(ValueType::Text),
            "duration" => Ok(ValueType::Duration),
            "timestamp" => Ok(ValueType::Timestamp),
            "decimal" => {
                self.expect_symbol('(')?;
                let scale = self.parse_u8("decimal scale")?;
                self.expect_symbol(')')?;
                Ok(ValueType::Decimal { scale })
            }
            "quantity" => {
                self.expect_symbol('(')?;
                let scale = self.parse_u8("quantity scale")?;
                self.expect_symbol(',')?;
                let unit = self.parse_uid()?;
                self.expect_symbol(')')?;
                Ok(ValueType::Quantity { scale, unit })
            }
            "reference" => {
                self.expect_symbol('(')?;
                let target = self.parse_enum_atom::<ReferenceKind>("reference target")?;
                self.expect_symbol(')')?;
                Ok(ValueType::Reference { target })
            }
            "list" | "set" | "datum" | "estimate" => {
                self.expect_symbol('(')?;
                let value = Box::new(self.parse_type(depth + 1)?);
                self.expect_symbol(')')?;
                match kind.as_str() {
                    "list" => Ok(ValueType::List { item: value }),
                    "set" => Ok(ValueType::Set { item: value }),
                    "datum" => Ok(ValueType::Datum { value }),
                    "estimate" => Ok(ValueType::Estimate { value }),
                    _ => unreachable!(),
                }
            }
            "map" => {
                self.expect_symbol('(')?;
                let key = Box::new(self.parse_type(depth + 1)?);
                self.expect_symbol(',')?;
                let value = Box::new(self.parse_type(depth + 1)?);
                self.expect_symbol(')')?;
                Ok(ValueType::Map { key, value })
            }
            "candidate" => {
                self.expect_symbol('(')?;
                let route = self.parse_enum_atom::<CandidateRoute>("candidate route")?;
                self.expect_symbol(',')?;
                let template = Slug::new(self.take_word("candidate template")?)
                    .map_err(|error| self.atom(error))?;
                self.expect_symbol(',')?;
                let fields = self.parse_candidate_type_fields(depth + 1)?;
                self.expect_symbol(')')?;
                Ok(ValueType::Candidate {
                    route,
                    template,
                    fields,
                })
            }
            _ => Err(self.unexpected(format!("unknown type {kind:?}"))),
        }?;
        value_type.validate().map_err(|error| self.atom(error))?;
        Ok(value_type)
    }

    fn parse_literal(&mut self, depth: usize) -> Result<LiteralValue, KarmaDslError> {
        self.check_depth(depth)?;
        let kind = self.take_word("literal constructor")?;
        self.expect_symbol('(')?;
        let literal = match kind.as_str() {
            "bool" => LiteralValue::Bool {
                value: self.parse_bool()?,
            },
            "i64" => LiteralValue::I64 {
                value: self.parse_i64("i64 literal")?,
            },
            "decimal" => {
                let scale = self.parse_u8("decimal scale")?;
                self.expect_symbol(',')?;
                let value =
                    DecimalValue::parse_canonical(scale, &self.take_string("decimal value")?)
                        .map_err(|error| self.atom(error))?;
                LiteralValue::Decimal { value }
            }
            "prob" => LiteralValue::Probability {
                value: Probability::from_str(&self.take_string("probability")?)
                    .map_err(|error| self.atom(error))?,
            },
            "conf" => LiteralValue::Confidence {
                value: Confidence::from_str(&self.take_string("confidence")?)
                    .map_err(|error| self.atom(error))?,
            },
            "text" => LiteralValue::Text {
                value: self.take_string("text literal")?,
            },
            "duration" => LiteralValue::Duration {
                value: DurationMs::new(self.parse_i64("duration milliseconds")?),
            },
            "timestamp" => LiteralValue::Timestamp {
                value: TimestampMs::parse_canonical(&self.take_string("timestamp")?)
                    .map_err(|error| self.atom(error))?,
            },
            "quantity" => {
                let scale = self.parse_u8("quantity scale")?;
                self.expect_symbol(',')?;
                let amount =
                    DecimalValue::parse_canonical(scale, &self.take_string("quantity amount")?)
                        .map_err(|error| self.atom(error))?;
                self.expect_symbol(',')?;
                let unit = self.parse_uid()?;
                LiteralValue::Quantity { amount, unit }
            }
            "reference" => LiteralValue::Reference {
                value: self.parse_reference()?,
            },
            "datum" => {
                let value_type = Box::new(self.parse_type(depth + 1)?);
                self.expect_symbol(',')?;
                let state = self.parse_enum_atom::<DatumState>("datum state")?;
                let value = if self.consume_symbol(',') {
                    Some(Box::new(self.parse_literal(depth + 1)?))
                } else {
                    None
                };
                LiteralValue::Datum {
                    value_type,
                    state,
                    value,
                }
            }
            "candidate" => {
                let route = self.parse_enum_atom::<CandidateRoute>("candidate route")?;
                self.expect_symbol(',')?;
                let template = Slug::new(self.take_word("candidate template")?)
                    .map_err(|error| self.atom(error))?;
                self.expect_symbol(',')?;
                let fields = self.parse_candidate_literal_fields(depth + 1)?;
                LiteralValue::Candidate {
                    route,
                    template,
                    fields,
                }
            }
            _ => return Err(self.unexpected(format!("unknown literal constructor {kind:?}"))),
        };
        self.expect_symbol(')')?;
        literal.value_type().map_err(|error| self.atom(error))?;
        Ok(literal)
    }

    fn parse_candidate_type_fields(
        &mut self,
        depth: usize,
    ) -> Result<BTreeMap<LocalId, ValueType>, KarmaDslError> {
        self.check_depth(depth)?;
        self.expect_symbol('{')?;
        let mut fields = BTreeMap::new();
        if self.consume_symbol('}') {
            return Ok(fields);
        }
        loop {
            let field = self.parse_local_id("candidate type field")?;
            self.expect_symbol(':')?;
            let value_type = self.parse_type(depth)?;
            insert_unique(&mut fields, field, value_type, self)?;
            if self.consume_symbol('}') {
                return Ok(fields);
            }
            self.expect_symbol(',')?;
        }
    }

    fn parse_candidate_literal_fields(
        &mut self,
        depth: usize,
    ) -> Result<BTreeMap<LocalId, LiteralValue>, KarmaDslError> {
        self.check_depth(depth)?;
        self.expect_symbol('{')?;
        let mut fields = BTreeMap::new();
        if self.consume_symbol('}') {
            return Ok(fields);
        }
        loop {
            let field = self.parse_local_id("candidate literal field")?;
            self.expect_symbol('=')?;
            let value = self.parse_literal(depth)?;
            insert_unique(&mut fields, field, value, self)?;
            if self.consume_symbol('}') {
                return Ok(fields);
            }
            self.expect_symbol(',')?;
        }
    }

    fn parse_precision(&mut self) -> Result<DecimalPrecision, KarmaDslError> {
        self.expect_word("precision")?;
        self.expect_symbol('(')?;
        let scale = self.parse_u8("precision scale")?;
        self.expect_symbol(',')?;
        let rounding = self.parse_enum_atom::<Rounding>("rounding mode")?;
        let result_unit = if self.consume_symbol(',') {
            if self.peek_word("dimensionless") {
                self.index += 1;
                Some(DeclaredUnit::Dimensionless)
            } else {
                Some(DeclaredUnit::Unit {
                    unit: self.parse_uid()?,
                })
            }
        } else {
            None
        };
        self.expect_symbol(')')?;
        Ok(DecimalPrecision {
            scale,
            rounding,
            result_unit,
        })
    }

    fn parse_uid(&mut self) -> Result<TypedUid, KarmaDslError> {
        self.expect_word("uid")?;
        self.expect_symbol('(')?;
        let kind = self.parse_enum_atom::<ReferenceKind>("uid kind")?;
        self.expect_symbol(',')?;
        let value = self.take_word("uid value")?;
        self.expect_symbol(')')?;
        TypedUid::new(kind, value).map_err(|error| self.atom(error))
    }

    fn parse_reference(&mut self) -> Result<ResolvedReference, KarmaDslError> {
        self.expect_word("ref")?;
        self.expect_symbol('(')?;
        let kind = self.parse_enum_atom::<ReferenceKind>("reference kind")?;
        self.expect_symbol(',')?;
        let uid = TypedUid::new(kind, self.take_word("reference uid")?)
            .map_err(|error| self.atom(error))?;
        self.expect_symbol(',')?;
        let display_slug = if self.peek_word("none") {
            self.index += 1;
            None
        } else {
            Some(
                Slug::new(self.take_word("reference display slug")?)
                    .map_err(|error| self.atom(error))?,
            )
        };
        self.expect_symbol(')')?;
        Ok(ResolvedReference {
            target: uid,
            display_slug,
        })
    }

    fn parse_optional_reference(&mut self) -> Result<Option<ResolvedReference>, KarmaDslError> {
        if self.peek_word("none") {
            self.index += 1;
            Ok(None)
        } else {
            self.parse_reference().map(Some)
        }
    }

    fn parse_single_reference_argument(&mut self) -> Result<ResolvedReference, KarmaDslError> {
        self.expect_symbol('(')?;
        let reference = self.parse_reference()?;
        self.expect_symbol(')')?;
        Ok(reference)
    }

    fn parse_single_local_id_argument(&mut self, label: &str) -> Result<LocalId, KarmaDslError> {
        self.expect_symbol('(')?;
        let id = self.parse_local_id(label)?;
        self.expect_symbol(')')?;
        Ok(id)
    }

    fn parse_output_ref(&mut self) -> Result<OutputRef, KarmaDslError> {
        self.expect_word("source")?;
        self.expect_symbol('(')?;
        let node = self.parse_local_id("source node")?;
        self.expect_symbol(',')?;
        let port = self.parse_local_id("source port")?;
        self.expect_symbol(')')?;
        Ok(OutputRef { node, port })
    }

    fn parse_slug_set(&mut self) -> Result<BTreeSet<Slug>, KarmaDslError> {
        self.expect_symbol('[')?;
        let mut values = BTreeSet::new();
        if self.consume_symbol(']') {
            return Ok(values);
        }
        loop {
            let slug = Slug::new(self.take_word("tag slug")?).map_err(|error| self.atom(error))?;
            if !values.insert(slug) {
                return Err(self.duplicate("tag"));
            }
            if self.consume_symbol(']') {
                return Ok(values);
            }
            self.expect_symbol(',')?;
        }
    }

    fn parse_capability_set(&mut self) -> Result<CapabilitySet, KarmaDslError> {
        self.expect_symbol('[')?;
        let mut values = BTreeSet::new();
        if self.consume_symbol(']') {
            return Ok(CapabilitySet::default());
        }
        loop {
            let capability = self.parse_enum_atom::<Capability>("capability")?;
            if !values.insert(capability) {
                return Err(self.duplicate("capability"));
            }
            if self.consume_symbol(']') {
                return Ok(CapabilitySet::new(values));
            }
            self.expect_symbol(',')?;
        }
    }

    fn parse_local_id(&mut self, label: &str) -> Result<LocalId, KarmaDslError> {
        LocalId::new(self.take_word(label)?).map_err(|error| self.atom(error))
    }

    fn parse_enum_atom<T: DeserializeOwned>(&mut self, label: &str) -> Result<T, KarmaDslError> {
        let value = self.take_word(label)?;
        serde_json::from_value(serde_json::Value::String(value.clone())).map_err(|_| {
            self.error_here(
                DslErrorKind::InvalidAtom,
                format!("invalid {label} {value:?}"),
            )
        })
    }

    fn parse_bool(&mut self) -> Result<bool, KarmaDslError> {
        match self.take_word("Boolean literal")?.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            value => Err(self.error_here(
                DslErrorKind::InvalidAtom,
                format!("invalid Boolean literal {value:?}"),
            )),
        }
    }

    fn parse_u8(&mut self, label: &str) -> Result<u8, KarmaDslError> {
        let value = self.take_word(label)?;
        value.parse::<u8>().map_err(|_| {
            self.error_here(
                DslErrorKind::InvalidAtom,
                format!("invalid {label} {value:?}"),
            )
        })
    }

    fn parse_u64(&mut self, label: &str) -> Result<u64, KarmaDslError> {
        let value = self.take_word(label)?;
        value.parse::<u64>().map_err(|_| {
            self.error_here(
                DslErrorKind::InvalidAtom,
                format!("invalid {label} {value:?}"),
            )
        })
    }

    fn parse_u32(&mut self, label: &str) -> Result<u32, KarmaDslError> {
        let value = self.take_word(label)?;
        value.parse::<u32>().map_err(|_| {
            self.error_here(
                DslErrorKind::InvalidAtom,
                format!("invalid {label} {value:?}"),
            )
        })
    }

    fn parse_i64(&mut self, label: &str) -> Result<i64, KarmaDslError> {
        let value = self.take_word(label)?;
        value.parse::<i64>().map_err(|_| {
            self.error_here(
                DslErrorKind::InvalidAtom,
                format!("invalid {label} {value:?}"),
            )
        })
    }

    fn check_depth(&self, depth: usize) -> Result<(), KarmaDslError> {
        if depth > MAX_DSL_NESTING {
            Err(self.error_here(
                DslErrorKind::NestingTooDeep,
                format!("DSL nesting exceeds {MAX_DSL_NESTING}"),
            ))
        } else {
            Ok(())
        }
    }

    fn set_once<T>(
        &self,
        target: &mut Option<T>,
        value: T,
        label: &str,
    ) -> Result<(), KarmaDslError> {
        if target.replace(value).is_some() {
            Err(self.duplicate(label))
        } else {
            Ok(())
        }
    }

    fn expect_word(&mut self, expected: &str) -> Result<(), KarmaDslError> {
        let actual = self.take_word(expected)?;
        if actual == expected {
            Ok(())
        } else {
            Err(self.unexpected(format!("expected {expected:?}, found {actual:?}")))
        }
    }

    fn take_word(&mut self, label: &str) -> Result<String, KarmaDslError> {
        let token = self.next_token()?;
        if let TokenKind::Word(value) = token.kind {
            Ok(value)
        } else {
            Err(error_at(
                DslErrorKind::UnexpectedToken,
                (token.offset, token.line, token.column),
                format!("expected {label}"),
            ))
        }
    }

    fn take_string(&mut self, label: &str) -> Result<String, KarmaDslError> {
        let token = self.next_token()?;
        if let TokenKind::String(value) = token.kind {
            Ok(value)
        } else {
            Err(error_at(
                DslErrorKind::UnexpectedToken,
                (token.offset, token.line, token.column),
                format!("expected {label} string"),
            ))
        }
    }

    fn expect_symbol(&mut self, expected: char) -> Result<(), KarmaDslError> {
        let token = self.next_token()?;
        if token.kind == TokenKind::Symbol(expected) {
            Ok(())
        } else {
            Err(error_at(
                DslErrorKind::UnexpectedToken,
                (token.offset, token.line, token.column),
                format!("expected symbol {expected:?}"),
            ))
        }
    }

    fn peek_word_is(&self, expected: &str) -> bool {
        self.tokens
            .get(self.index)
            .is_some_and(|token| token.kind == TokenKind::Word(expected.to_string()))
    }

    fn consume_symbol(&mut self, expected: char) -> bool {
        if self
            .tokens
            .get(self.index)
            .is_some_and(|token| token.kind == TokenKind::Symbol(expected))
        {
            self.index += 1;
            true
        } else {
            false
        }
    }

    fn peek_word(&self, expected: &str) -> bool {
        self.tokens
            .get(self.index)
            .is_some_and(|token| matches!(&token.kind, TokenKind::Word(value) if value == expected))
    }

    fn next_token(&mut self) -> Result<Token, KarmaDslError> {
        let token =
            self.tokens.get(self.index).cloned().ok_or_else(|| {
                self.error_here(DslErrorKind::UnexpectedEnd, "unexpected end of DSL")
            })?;
        self.index += 1;
        Ok(token)
    }

    fn atom(&self, error: KarmaBoundaryError) -> KarmaDslError {
        self.error_here(DslErrorKind::InvalidAtom, error.to_string())
    }

    fn duplicate(&self, label: &str) -> KarmaDslError {
        self.error_here(
            DslErrorKind::DuplicateDeclaration,
            format!("duplicate {label}"),
        )
    }

    fn missing(&self, label: &str) -> KarmaDslError {
        self.error_here(
            DslErrorKind::MissingDeclaration,
            format!("missing required {label}"),
        )
    }

    fn unexpected(&self, message: impl Into<String>) -> KarmaDslError {
        self.error_here(DslErrorKind::UnexpectedToken, message)
    }

    fn error_here(&self, kind: DslErrorKind, message: impl Into<String>) -> KarmaDslError {
        if let Some(token) = self.tokens.get(self.index.saturating_sub(1)) {
            error_at(kind, (token.offset, token.line, token.column), message)
        } else {
            let (line, column) = end_location(self.source);
            error_at(kind, (self.source.len(), line, column), message)
        }
    }
}

fn insert_unique<K: Ord, V>(
    values: &mut BTreeMap<K, V>,
    key: K,
    value: V,
    parser: &Parser<'_>,
) -> Result<(), KarmaDslError> {
    if values.insert(key, value).is_some() {
        Err(parser.duplicate("map id"))
    } else {
        Ok(())
    }
}

fn end_location(source: &str) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;
    for byte in source.bytes() {
        if byte == b'\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }
    (line, column)
}

enum ParsedFrequencyCadence {
    Elapsed(DurationBinding),
    Calendar(CadenceAst),
}

enum ParsedFrequencyAnchor {
    Timestamp(TimestampMs),
    Civil(CivilDateTime),
}
