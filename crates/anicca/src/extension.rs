use std::collections::HashSet;
use std::fmt::Write;

use serde_json::{Map, Value};

use crate::{Diagnostic, diagnostic, grammar::grammar as ast};

pub(crate) fn validate(document: &ast::Document) -> Result<(), Diagnostic> {
    let mut seen = HashSet::new();
    for declaration in &document.declarations {
        let ast::Declaration::Extension(extension) = declaration else {
            continue;
        };
        let namespace = text(&extension.namespace)?;
        if namespace.is_empty()
            || namespace.len() > 200
            || namespace.trim() != namespace
            || namespace.chars().any(char::is_control)
        {
            return Err(diagnostic("Extension namespace is invalid"));
        }
        if !seen.insert((&extension.target.slug.0, namespace.clone())) {
            return Err(diagnostic(format!(
                "duplicate extension `{namespace}` for `@{}`",
                extension.target.slug.0
            )));
        }
        let value = object(&extension.fields, 0).map_err(|error| {
            diagnostic(format!(
                "@{} {namespace}: {}",
                extension.target.slug.0, error.message
            ))
        })?;
        if value.to_string().len() > 1024 * 1024 {
            return Err(diagnostic("Extension exceeds 1 MiB"));
        }
    }
    Ok(())
}

pub(crate) fn text(value: &ast::ExtensionText) -> Result<String, Diagnostic> {
    serde_json::from_str(&value.0)
        .map_err(|error| diagnostic(format!("Invalid extension string: {error}")))
}

pub(crate) fn object(object: &ast::ExtensionObject, depth: usize) -> Result<Value, Diagnostic> {
    if depth > 32 || object.fields.len() > 4096 {
        return Err(diagnostic("Extension exceeds its depth or field limit"));
    }
    let mut output = Map::new();
    for field in &object.fields {
        if field.key.0.len() > 200 {
            return Err(diagnostic("Extension key exceeds 200 bytes"));
        }
        if output
            .insert(field.key.0.clone(), value(&field.value, depth + 1)?)
            .is_some()
        {
            return Err(diagnostic(format!(
                "duplicate extension key `{}`",
                field.key.0
            )));
        }
    }
    Ok(Value::Object(output))
}

fn value(value: &ast::ExtensionValue, depth: usize) -> Result<Value, Diagnostic> {
    if depth > 32 {
        return Err(diagnostic("Extension exceeds its depth limit"));
    }
    match value {
        ast::ExtensionValue::Object(value) => object(value, depth),
        ast::ExtensionValue::List(value) => {
            if value.values.len() > 4096 {
                return Err(diagnostic("Extension list exceeds 4096 items"));
            }
            value
                .values
                .iter()
                .map(|item| self::value(item, depth + 1))
                .collect::<Result<Vec<_>, _>>()
                .map(Value::Array)
        }
        ast::ExtensionValue::Text(value) => text(value).map(Value::String),
        ast::ExtensionValue::Number(value) => serde_json::from_str(&value.0)
            .map_err(|_| diagnostic(format!("invalid extension number `{}`", value.0))),
        ast::ExtensionValue::True(_) => Ok(Value::Bool(true)),
        ast::ExtensionValue::False(_) => Ok(Value::Bool(false)),
        ast::ExtensionValue::Null(_) => Ok(Value::Null),
    }
}

pub(crate) fn format_ast(output: &mut String, extension: &ast::Extension) {
    writeln!(
        output,
        "Extension @{} {} {{",
        extension.target.slug.0, extension.namespace.0
    )
    .unwrap();
    format_fields(output, &extension.fields, 1);
    output.push_str("}\n");
}

fn format_fields(output: &mut String, object: &ast::ExtensionObject, depth: usize) {
    for field in &object.fields {
        write!(output, "{}{} ", "    ".repeat(depth), field.key.0).unwrap();
        format_value(output, &field.value, depth);
        output.push('\n');
    }
}

fn format_value(output: &mut String, value: &ast::ExtensionValue, depth: usize) {
    match value {
        ast::ExtensionValue::Object(value) => {
            output.push_str("{\n");
            format_fields(output, value, depth + 1);
            write!(output, "{}}}", "    ".repeat(depth)).unwrap();
        }
        ast::ExtensionValue::List(value) => {
            output.push('[');
            for (index, value) in value.values.iter().enumerate() {
                if index != 0 {
                    output.push(' ');
                }
                format_value(output, value, depth);
            }
            output.push(']');
        }
        ast::ExtensionValue::Text(value) => output.push_str(&value.0),
        ast::ExtensionValue::Number(value) => output.push_str(&value.0),
        ast::ExtensionValue::True(_) => output.push_str("true"),
        ast::ExtensionValue::False(_) => output.push_str("false"),
        ast::ExtensionValue::Null(_) => output.push_str("null"),
    }
}

pub fn render(target: &str, namespace: &str, fields: &Value) -> Result<String, Diagnostic> {
    let fields = fields
        .as_object()
        .ok_or_else(|| diagnostic("Extension requires an object"))?;
    let mut output = format!("Extension @{target} {} {{\n", crate::quoted(namespace));
    render_fields(&mut output, fields, 1)?;
    output.push_str("}\n");
    crate::parse(&output)?;
    Ok(output)
}

fn render_fields(
    output: &mut String,
    fields: &Map<String, Value>,
    depth: usize,
) -> Result<(), Diagnostic> {
    if depth > 32 {
        return Err(diagnostic("Extension exceeds its depth limit"));
    }
    for (key, value) in fields {
        let mut bytes = key.bytes();
        if !bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
            || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
        {
            return Err(diagnostic(format!(
                "Extension key `{key}` is not a Lingua key"
            )));
        }
        write!(output, "{}{key} ", "    ".repeat(depth)).unwrap();
        render_value(output, value, depth)?;
        output.push('\n');
    }
    Ok(())
}

fn render_value(output: &mut String, value: &Value, depth: usize) -> Result<(), Diagnostic> {
    if depth > 32 {
        return Err(diagnostic("Extension exceeds its depth limit"));
    }
    match value {
        Value::Object(fields) => {
            output.push_str("{\n");
            render_fields(output, fields, depth + 1)?;
            write!(output, "{}}}", "    ".repeat(depth)).unwrap();
        }
        Value::Array(values) => {
            output.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    output.push(' ');
                }
                render_value(output, value, depth + 1)?;
            }
            output.push(']');
        }
        value => output.push_str(&value.to_string()),
    }
    Ok(())
}
