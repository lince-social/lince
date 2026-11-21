use super::{Expr, TokenKey};
use crate::error::NucleusError;

/// Reading name used for a concept reference.
pub const ASSERTION: &str = "assertion";
/// Reading name used for an assertion lookup.
pub const HAS_ASSERTION: &str = "has_assertion";

/// Builds a lookup token for a subject, predicate, and optional object.
///
/// # Errors
///
/// Returns a parse error for a wrong argument count or an invalid reference or predicate.
pub fn assertion_token(args: &[Expr]) -> Result<TokenKey, NucleusError> {
    let references = assertion_references(args)?;
    Ok(TokenKey {
        func: HAS_ASSERTION.into(),
        slug: joined_references(references).unwrap_or_default(),
        dur_secs: None,
    })
}

pub(super) fn assertion_references(
    args: &[Expr],
) -> Result<impl Iterator<Item = &str>, NucleusError> {
    if !(2..=3).contains(&args.len()) {
        return Err(NucleusError::Parse(
            "Use has_assertion(@record, #predicate, @object) or omit @object".into(),
        ));
    }
    let mut references = [None; 3];
    for ((position, argument), slot) in args.iter().enumerate().zip(&mut references) {
        let reference = match argument {
            Expr::Ref(reference) => reference,
            Expr::Fn(name, inner) if position == 1 && name == ASSERTION => match inner.as_slice() {
                [Expr::Ref(reference)] => reference,
                _ => return Err(NucleusError::Parse("Choose an assertion predicate".into())),
            },
            _ => {
                return Err(NucleusError::Parse(
                    "Use Record references and an assertion predicate".into(),
                ));
            }
        };
        *slot = Some(reference.as_str());
    }
    Ok(references.into_iter().flatten())
}

/// Returns the binding name for a positional assertion argument.
pub fn assertion_binding(position: usize) -> &'static str {
    match position {
        0 => "has_assertion.subject",
        1 => "has_assertion.predicate",
        _ => "has_assertion.object",
    }
}

/// Encodes an extension namespace and property as a reading name.
#[expect(
    clippy::expect_used,
    clippy::missing_panics_doc,
    reason = "serializing two string slices to an in-memory JSON string cannot fail"
)]
pub fn extension_reading(namespace: &str, property: &str) -> String {
    format!(
        "extension:{}",
        serde_json::to_string(&(namespace, property)).expect("the token key contains only strings")
    )
}

/// Decodes the namespace and property of an encoded extension reading.
pub fn extension_parts(reading: &str) -> Option<(String, String)> {
    serde_json::from_str(reading.strip_prefix("extension:")?).ok()
}

/// Returns the bare binding name for an encoded extension reading or a plain reading.
pub fn binding_reading(reading: &str) -> &str {
    if reading.starts_with("extension:") {
        "extension"
    } else {
        reading
    }
}

/// Builds a lookup token for a Record's named extension property.
///
/// # Errors
///
/// Returns a parse error for an invalid argument list, namespace, or property.
pub fn extension_token(args: &[Expr]) -> Result<TokenKey, NucleusError> {
    let [
        Expr::Ref(record),
        Expr::Text(namespace),
        Expr::Text(property),
    ] = args
    else {
        return Err(NucleusError::Parse(
            "Use extension(@record, \"namespace\", \"property\")".into(),
        ));
    };
    if [namespace, property].iter().any(|name| {
        name.is_empty()
            || name.trim() != name.as_str()
            || name.len() > 200
            || name.chars().any(char::is_control)
    }) || property.contains('.')
    {
        return Err(NucleusError::Parse(
            "Choose a valid extension namespace and property".into(),
        ));
    }
    Ok(TokenKey {
        func: extension_reading(namespace, property),
        slug: record.clone(),
        dur_secs: None,
    })
}

pub(super) fn joined_references<'a>(
    mut references: impl Iterator<Item = &'a str>,
) -> Option<String> {
    let mut joined = references.next()?.to_owned();
    for reference in references {
        joined.push('|');
        joined.push_str(reference);
    }
    Some(joined)
}

impl Expr {
    /// Collects reading tokens in expression order, including duplicates.
    pub fn tokens(&self) -> Vec<TokenKey> {
        let mut out = Vec::new();
        self.collect_tokens(&mut out);
        out
    }

    fn collect_tokens(&self, out: &mut Vec<TokenKey>) {
        match self {
            Self::Ref(slug) => out.push(TokenKey {
                func: "quantity".into(),
                slug: slug.clone(),
                dur_secs: None,
            }),
            Self::Fn(name, args) => {
                let special = match name.as_str() {
                    HAS_ASSERTION => Some(assertion_token(args)),
                    "extension" => Some(extension_token(args)),
                    _ => None,
                };
                if let Some(token) = special {
                    out.extend(token.ok());
                    return;
                }

                let references = args.iter().filter_map(|arg| match arg {
                    Self::Ref(slug) => Some(slug.as_str()),
                    _ => None,
                });
                if let Some(slug) = joined_references(references) {
                    out.push(TokenKey {
                        func: name.clone(),
                        slug,
                        dur_secs: args.iter().find_map(|arg| match arg {
                            Self::Dur(seconds) => Some(*seconds),
                            _ => None,
                        }),
                    });
                }
                for a in args {
                    if !matches!(a, Self::Ref(_)) {
                        a.collect_tokens(out);
                    }
                }
            }
            Self::Unary(_, e) => e.collect_tokens(out),
            Self::Bin(_, a, b) => {
                a.collect_tokens(out);
                b.collect_tokens(out);
            }
            Self::Num(_) | Self::Dur(_) | Self::Text(_) => {}
        }
    }
}
