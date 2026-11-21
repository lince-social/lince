use std::fmt::{self, Display, Write as _, from_fn};

use super::{enum_atom, json_string, render, write_joined};
use crate::{
    CadenceAst, CadenceBound, DurationBinding, FrequencyAst, FrequencyCadenceAst,
    FrequencyParameterDefinition, MissedPolicy, PositiveIntegerBinding, Slug,
};

/// Writes a Frequency in canonical DSL order.
pub fn format_frequency(frequency: &FrequencyAst) -> String {
    render(|output| write_frequency(output, frequency))
}

fn write_frequency(output: &mut String, frequency: &FrequencyAst) -> fmt::Result {
    output.push_str("karma-frequency 1;\n");
    writeln!(output, "frequency {} {{", frequency.slug)?;
    writeln!(output, "  schema {};", enum_atom(&frequency.schema))?;
    writeln!(output, "  purpose {};", json_string(&frequency.purpose))?;
    output.push_str("  tags [");
    write_joined(output, frequency.tags.iter().map(Slug::as_str))?;
    output.push_str("];\n");
    for (id, parameter) in &frequency.parameters {
        match parameter {
            FrequencyParameterDefinition::Duration {
                default,
                minimum,
                maximum,
            } => writeln!(
                output,
                "  param {}: duration default duration({}) range [duration({}), duration({})];",
                id.as_str(),
                default.get(),
                minimum.get(),
                maximum.get()
            )?,
            FrequencyParameterDefinition::PositiveInteger {
                default,
                minimum,
                maximum,
            } => writeln!(
                output,
                "  param {}: positive-integer default integer({}) range [integer({}), integer({})];",
                id.as_str(),
                default.get(),
                minimum.get(),
                maximum.get()
            )?,
        }
    }
    match &frequency.cadence {
        FrequencyCadenceAst::Elapsed { interval, anchor } => {
            writeln!(
                output,
                "  every elapsed {};",
                format_duration_binding(interval)
            )?;
            writeln!(
                output,
                "  anchor timestamp({});",
                json_string(&anchor.to_string())
            )?;
        }
        FrequencyCadenceAst::Calendar {
            cadence,
            anchor,
            timezone,
            tzdb,
            gap,
            fold,
        } => {
            writeln!(output, "  every {};", format_cadence_ast(cadence))?;
            writeln!(
                output,
                "  anchor civil({});",
                json_string(&anchor.to_string())
            )?;
            writeln!(output, "  timezone {};", timezone.as_str())?;
            writeln!(
                output,
                "  tzdb version {} digest {};",
                json_string(tzdb.version.as_str()),
                json_string(tzdb.digest.as_str())
            )?;
            writeln!(output, "  gap {};", enum_atom(gap))?;
            writeln!(output, "  fold {};", enum_atom(fold))?;
        }
    }
    output.push_str("  timer {\n");
    writeln!(
        output,
        "    resolution {};",
        format_duration_binding(&frequency.timer.required_resolution)
    )?;
    writeln!(
        output,
        "    max-lateness {};",
        format_duration_binding(&frequency.timer.max_lateness)
    )?;
    writeln!(
        output,
        "    coalesce-window {};",
        format_duration_binding(&frequency.timer.coalesce_window)
    )?;
    output.push_str("  }\n");
    writeln!(
        output,
        "  missed {};",
        format_missed_policy(frequency.missed)
    )?;
    writeln!(
        output,
        "  inactive-gap {};",
        enum_atom(&frequency.inactive_gap)
    )?;
    writeln!(output, "  rephase {};", enum_atom(&frequency.rephase))?;
    writeln!(output, "  overload {};", enum_atom(&frequency.overload))?;
    output.push_str("}\n");
    Ok(())
}

fn format_duration_binding(binding: &DurationBinding) -> impl Display + '_ {
    from_fn(move |out| match binding {
        DurationBinding::Literal { value } => write!(out, "duration({})", value.get()),
        DurationBinding::Parameter { parameter } => {
            write!(out, "parameter({})", parameter.as_str())
        }
    })
}

fn format_positive_integer_binding(binding: &PositiveIntegerBinding) -> impl Display + '_ {
    from_fn(move |out| match binding {
        PositiveIntegerBinding::Literal { value } => write!(out, "integer({value})"),
        PositiveIntegerBinding::Parameter { parameter } => {
            write!(out, "parameter({})", parameter.as_str())
        }
    })
}

fn format_cadence_ast(cadence: &CadenceAst) -> impl Display + '_ {
    from_fn(move |out| {
        let step = &cadence.every;
        let components = [
            ("years", &step.years),
            ("months", &step.months),
            ("weeks", &step.weeks),
            ("days", &step.days),
            ("hours", &step.hours),
            ("minutes", &step.minutes),
            ("seconds", &step.seconds),
            ("milliseconds", &step.milliseconds),
        ];
        out.write_str("calendar(step(")?;
        write_joined(
            out,
            components.into_iter().filter_map(|(name, binding)| {
                binding.as_ref().map(|value| {
                    from_fn(move |out| {
                        write!(out, "{name}({})", format_positive_integer_binding(value))
                    })
                })
            }),
        )?;
        out.write_str(")")?;
        if let Some(weekdays) = &cadence.land_on {
            out.write_str(", land([")?;
            write_joined(out, weekdays.iter().map(|day| enum_atom(&day)))?;
            out.write_str("])")?;
        }
        write!(
            out,
            ", invalid-day({}), bound({}))",
            enum_atom(&cadence.invalid_day),
            format_cadence_bound(cadence.bound)
        )
    })
}

fn format_cadence_bound(bound: CadenceBound) -> impl Display {
    from_fn(move |out| match bound {
        CadenceBound::Unbounded => out.write_str("unbounded"),
        CadenceBound::Count { occurrences } => write!(out, "count({occurrences})"),
        CadenceBound::Until { at } => write!(out, "until({})", json_string(&at.to_string())),
    })
}

fn format_missed_policy(policy: MissedPolicy) -> impl Display {
    from_fn(move |out| match policy {
        MissedPolicy::Skip => out.write_str("skip"),
        MissedPolicy::Coalesce => out.write_str("coalesce"),
        MissedPolicy::Replay { max } => write!(out, "replay({max})"),
        MissedPolicy::PauseOnLag => out.write_str("pause-on-lag"),
    })
}
