use std::fmt::{self, Display, Write as _, from_fn};

use super::values::{
    format_expression, format_literal, format_optional_reference, format_reference, format_type,
};
use super::{enum_atom, json_string, render, write_joined};
use crate::{InputSource, NodeOperation, ProgramAst, Slug, StateContract, TriggerSource};

/// Writes a Karma program in canonical DSL order.
pub fn format_program(program: &ProgramAst) -> String {
    render(|output| write_program(output, program))
}

fn write_program(output: &mut String, program: &ProgramAst) -> fmt::Result {
    output.push_str("karma 1;\n");
    writeln!(output, "program {} {{", program.slug)?;
    writeln!(output, "  schema {};", enum_atom(&program.schema))?;
    writeln!(output, "  purpose {};", json_string(&program.purpose))?;
    output.push_str("  tags [");
    write_joined(output, program.tags.iter().map(Slug::as_str))?;
    output.push_str("];\n");
    output.push_str("  capabilities [");
    let mut capability_names = program
        .required_capabilities
        .iter()
        .map(|capability| enum_atom(&capability))
        .collect::<Vec<_>>();
    capability_names.sort_unstable();
    write_joined(output, capability_names)?;
    output.push_str("];\n");

    for (id, parameter) in &program.parameters {
        writeln!(
            output,
            "  param {}: {} {} = {};",
            id.as_str(),
            format_type(&parameter.value_type),
            if parameter.mutable {
                "mutable"
            } else {
                "fixed"
            },
            format_literal(&parameter.default)
        )?;
    }

    for (id, node) in &program.nodes {
        writeln!(output, "  node {} {{", id.as_str())?;
        for (input_id, binding) in &node.inputs {
            writeln!(
                output,
                "    bind {}: {} = source({}, {});",
                input_id.as_str(),
                format_type(&binding.expected_type),
                binding.source.node.as_str(),
                binding.source.port.as_str()
            )?;
        }
        for (port_id, contract) in &node.outputs {
            let freshness = from_fn(|out| match contract.freshness {
                Some(value) => write!(out, "duration({})", value.get()),
                None => out.write_str("none"),
            });
            writeln!(
                output,
                "    port {}: {} sensitivity {} freshness {};",
                port_id.as_str(),
                format_type(&contract.value_type),
                enum_atom(&contract.sensitivity),
                freshness
            )?;
        }
        format_operation(output, &node.operation)?;
        output.push_str("  }\n");
    }

    for (id, source) in &program.outputs {
        writeln!(
            output,
            "  output {} = source({}, {});",
            id.as_str(),
            source.node.as_str(),
            source.port.as_str()
        )?;
    }
    output.push_str("}\n");
    Ok(())
}

fn format_operation(output: &mut String, operation: &NodeOperation) -> fmt::Result {
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
            )?;
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
            )?;
        }
        NodeOperation::Derive { expressions } => {
            output.push_str("    op derive {\n");
            for (port, expression) in expressions {
                writeln!(
                    output,
                    "      expr {} = {};",
                    port.as_str(),
                    format_expression(expression)
                )?;
            }
            output.push_str("    }\n");
        }
        NodeOperation::Delay {
            input,
            output: port,
            initial,
            state,
        } => {
            writeln!(
                output,
                "    op delay({}, {}, {}, {});",
                input.as_str(),
                port.as_str(),
                format_literal(initial),
                format_state_contract(state)
            )?;
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
        )?,
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
        )?,
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
        )?,
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
        )?,
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
            )?;
            write_joined(
                output,
                fields.iter().map(|(field, input)| {
                    from_fn(move |out| write!(out, "{} = {}", field.as_str(), input.as_str()))
                }),
            )?;
            output.push_str("});\n");
        }
    }
    Ok(())
}

fn format_state_contract(state: &StateContract) -> impl Display + '_ {
    from_fn(move |out| {
        write!(
            out,
            "state({}, {}, {}, {}, {})",
            enum_atom(&state.persistence),
            enum_atom(&state.reset),
            enum_atom(&state.late_event),
            enum_atom(&state.migration),
            enum_atom(&state.simulation)
        )
    })
}

fn format_trigger_source(source: &TriggerSource) -> impl Display + '_ {
    from_fn(move |out| match source {
        TriggerSource::Manual => out.write_str("manual"),
        TriggerSource::Fact { record, concept } => write!(
            out,
            "fact({}, {})",
            format_optional_reference(record.as_ref()),
            format_optional_reference(concept.as_ref())
        ),
        TriggerSource::Frequency { frequency } => {
            write!(out, "frequency({})", format_reference(frequency))
        }
        TriggerSource::Signal { signal } => {
            write!(out, "signal({})", format_reference(signal))
        }
        TriggerSource::Decision { decision } => {
            write!(out, "decision({})", format_reference(decision))
        }
        TriggerSource::Receipt { receipt } => {
            write!(out, "receipt({})", format_reference(receipt))
        }
        TriggerSource::Sync => out.write_str("sync"),
    })
}

fn format_input_source(source: &InputSource) -> impl Display + '_ {
    from_fn(move |out| match source {
        InputSource::Parameter { parameter } => write!(out, "parameter({})", parameter.as_str()),
        InputSource::RecordQuantity { record } => {
            write!(out, "record-quantity({})", format_reference(record))
        }
        InputSource::SavedProtein { view } => {
            write!(out, "saved-protein({})", format_reference(view))
        }
        InputSource::Signal { signal } => {
            write!(out, "signal({})", format_reference(signal))
        }
        InputSource::CapturedFact { fact } => {
            write!(out, "captured-fact({})", format_reference(fact))
        }
    })
}
