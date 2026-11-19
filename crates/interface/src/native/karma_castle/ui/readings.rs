use super::*;
use std::{borrow::Cow, fmt};

pub(super) struct Part {
    pub(super) text: String,
    pub(super) reading: Option<Reading>,
}

pub(super) fn prepare(owner: Entity, source: &str, rule: Option<&Rule>, index: usize) -> Vec<Part> {
    let _prepare = nucleus::performance::Span::new("karma-reading-preparation");
    let mut offset = 0;
    let parts: Vec<_> = model::fragments(source)
        .into_iter()
        .map(|(text, slug)| {
            let reading = slug.map(|slug| {
                let reading = model::reading_at(source, offset, offset + text.len());
                let reading = match rule {
                    Some(rule) if index == 2 && !rule.record.is_empty() => {
                        format!("@{}", rule.record)
                    }
                    Some(rule) => model::bound_reading(rule, &reading),
                    None => reading,
                };
                let slug = rule
                    .and_then(|rule| {
                        if index == 2 {
                            (!rule.record.is_empty()).then_some(rule.record.as_str())
                        } else {
                            rule.bindings.iter().find_map(|binding| {
                                (binding.authored == slug
                                    && reading.contains(binding.target.as_str()))
                                .then_some(binding.target.as_str())
                            })
                        }
                    })
                    .map(str::to_owned)
                    .unwrap_or(slug);
                Reading {
                    owner,
                    slug,
                    source: reading,
                    hovered: false,
                    requested_at: None,
                    pending: false,
                    result: None,
                    hint: None,
                }
            });
            offset += text.len();
            Part { text, reading }
        })
        .collect();
    nucleus::performance::add("karma-reading-fragments-prepared", parts.len() as u64);
    parts
}

#[derive(Debug, PartialEq, Eq)]
enum Base<'a> {
    Frequency(model::FrequencyHint<'a>),
    Record {
        head: Cow<'a, str>,
        quantity: Cow<'a, str>,
    },
    Unavailable,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Hint<'a> {
    base: Base<'a>,
    expression: Option<(Cow<'a, str>, Cow<'a, str>)>,
    pub(super) error: Option<Cow<'a, str>>,
}

impl<'a> Hint<'a> {
    pub(super) fn new(view: &'a View, reading: &'a Reading, complex: bool, now: i64) -> Self {
        let base = if let Some(frequency) = view
            .frequency_lookup
            .get(&reading.slug)
            .map(|index| &view.frequencies[*index])
        {
            Base::Frequency(model::FrequencyHint::new(frequency, now))
        } else if let Some(record) = view
            .record_lookup
            .get(&reading.slug)
            .map(|index| &view.records[*index])
        {
            Base::Record {
                head: record["head"].as_str().unwrap_or(&reading.slug).into(),
                quantity: record["quantity"].as_str().unwrap_or("unavailable").into(),
            }
        } else {
            Base::Unavailable
        };
        Self {
            base,
            expression: if complex {
                reading
                    .result
                    .as_deref()
                    .map(|result| (reading.source.as_str().into(), result.into()))
            } else {
                None
            },
            error: None,
        }
    }

    pub(super) fn countdown(&self) -> bool {
        matches!(&self.base, Base::Frequency(frequency) if frequency.scheduled())
    }

    pub(super) fn into_owned(self) -> Hint<'static> {
        Hint {
            base: match self.base {
                Base::Frequency(frequency) => Base::Frequency(frequency.into_owned()),
                Base::Record { head, quantity } => Base::Record {
                    head: head.into_owned().into(),
                    quantity: quantity.into_owned().into(),
                },
                Base::Unavailable => Base::Unavailable,
            },
            expression: self
                .expression
                .map(|(source, result)| (source.into_owned().into(), result.into_owned().into())),
            error: self.error.map(|error| error.into_owned().into()),
        }
    }
}

impl fmt::Display for Hint<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.base {
            Base::Frequency(frequency) => write!(f, "{frequency}")?,
            Base::Record { head, quantity } => write!(f, "{head}\nCurrent quantity: {quantity}")?,
            Base::Unavailable => f.write_str("Record unavailable or not readable")?,
        }
        if let Some((source, result)) = &self.expression {
            write!(f, "\n{source}: {result}")?;
        }
        if let Some(error) = &self.error {
            write!(f, "\n{error}")?;
        }
        Ok(())
    }
}
