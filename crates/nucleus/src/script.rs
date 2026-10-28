use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use anicca::grammar::grammar as ast;

use crate::DecimalValue;
use crate::expr::BinOp;

pub const MAX_STEPS: usize = 100_000;
pub const MAX_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_DEPTH: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Number(DecimalValue),
    Bool(bool),
    Text(String),
    Reference(String),
    Concept(String),
    Duration(i64),
    Null,
    List(Vec<Value>),
    Object(BTreeMap<String, Value>),
    Declaration(String),
}

impl Value {
    pub fn render(&self) -> String {
        match self {
            Self::Number(value) => value.to_string(),
            Self::Bool(value) => value.to_string(),
            Self::Text(value) | Self::Declaration(value) => value.clone(),
            Self::Reference(value) => format!("@{value}"),
            Self::Concept(value) => format!("#{value}"),
            Self::Duration(value) => format!("{value}s"),
            Self::Null => "null".into(),
            Self::List(values)
                if !values.is_empty()
                    && values
                        .iter()
                        .all(|value| matches!(value, Self::Declaration(_))) =>
            {
                values
                    .iter()
                    .map(Value::render)
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            Self::List(values) => format!(
                "[{}]",
                values
                    .iter()
                    .map(Self::literal)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Object(values) => format!(
                "{{{}}}",
                values
                    .iter()
                    .map(|(key, value)| { format!("{}: {}", quoted(key), value.literal()) })
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        }
    }

    fn literal(&self) -> String {
        match self {
            Self::Text(value) | Self::Declaration(value) => quoted(value),
            other => other.render(),
        }
    }

    pub fn to_json(&self) -> Result<serde_json::Value, String> {
        use serde_json::Value as J;
        Ok(match self {
            Self::Number(value) if value.scale() == 0 => i64::try_from(value.mantissa())
                .map(|number| J::Number(number.into()))
                .unwrap_or_else(|_| J::String(value.to_string())),
            Self::Number(value) => J::String(value.to_string()),
            Self::Bool(value) => J::Bool(*value),
            Self::Text(value) | Self::Reference(value) | Self::Concept(value) => {
                J::String(value.clone())
            }
            Self::Duration(value) => J::Number((*value).into()),
            Self::Null => J::Null,
            Self::List(values) => {
                J::Array(values.iter().map(Self::to_json).collect::<Result<_, _>>()?)
            }
            Self::Object(values) => J::Object(
                values
                    .iter()
                    .map(|(key, value)| Ok((key.clone(), value.to_json()?)))
                    .collect::<Result<_, String>>()?,
            ),
            Self::Declaration(_) => {
                return Err("A declaration cannot be used as a query descriptor".into());
            }
        })
    }

    pub fn from_json(value: serde_json::Value) -> Result<Self, String> {
        use serde_json::Value as J;
        Ok(match value {
            J::Null => Self::Null,
            J::Bool(value) => Self::Bool(value),
            J::Number(value) => Self::Number(
                DecimalValue::parse_inferred(&value.to_string())
                    .map_err(|error| error.to_string())?,
            ),
            J::String(value) => Self::Text(value),
            J::Array(values) => Self::List(
                values
                    .into_iter()
                    .map(Self::from_json)
                    .collect::<Result<_, _>>()?,
            ),
            J::Object(values) => Self::Object(
                values
                    .into_iter()
                    .map(|(key, value)| Ok((key, Self::from_json(value)?)))
                    .collect::<Result<_, String>>()?,
            ),
        })
    }
}

fn quoted(value: &str) -> String {
    serde_json::to_string(value).expect("text serialization")
}

pub trait Host {
    fn call(&mut self, name: &str, arguments: &[Value]) -> Result<Value, String>;
    fn print(&mut self, text: &str) -> Result<(), String>;
    fn cancelled(&self) -> bool {
        false
    }
}

#[derive(Debug, Clone)]
pub struct Error {
    pub message: String,
    pub offset: usize,
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

pub fn validate(document: &ast::Document) -> Result<(), Error> {
    let mut names = vec![BTreeSet::new()];
    let mut steps = 0usize;
    let mut directory = false;
    for declaration in &document.declarations {
        if let ast::Declaration::Statement(statement) = declaration {
            validate_statement(statement, &mut names, &mut steps, 0, &mut directory)?;
        }
    }
    Ok(())
}

fn reserved_binding(name: &str) -> bool {
    matches!(
        name,
        "dir" | "let" | "true" | "false" | "null" | "and" | "or" | "if" | "else" | "for" | "in"
    )
}

fn statement_offset(statement: &ast::Statement) -> usize {
    match statement {
        ast::Statement::Binding(value) => value.value.node.span.0,
        ast::Statement::Assignment(value) => value.value.node.span.0,
        ast::Statement::Conditional(value) => value.condition.node.span.0,
        ast::Statement::ForLoop(value) => value.iterable.node.span.0,
        ast::Statement::Call(value) => value
            .call
            .arguments
            .first()
            .map_or(0, |value| value.node.span.0),
    }
}

fn validate_statement(
    statement: &ast::Statement,
    names: &mut Vec<BTreeSet<String>>,
    steps: &mut usize,
    depth: usize,
    directory: &mut bool,
) -> Result<(), Error> {
    let offset = statement_offset(statement);
    let error = |message: String| Error { message, offset };
    *steps += 1;
    if depth > MAX_DEPTH || *steps > MAX_STEPS {
        return Err(error("Statement limit exceeded".into()));
    }
    match statement {
        ast::Statement::Binding(binding) => {
            validate_expression(&binding.value, names, steps, depth + 1)?;
            if reserved_binding(&binding.name.0)
                || !names
                    .last_mut()
                    .expect("local scope")
                    .insert(binding.name.0.clone())
            {
                return Err(error(format!(
                    "Duplicate or reserved binding `{}`",
                    binding.name.0
                )));
            }
        }
        ast::Statement::Assignment(assignment) if assignment.name.0 == "dir" => {
            if depth != 0 {
                return Err(error("dir must be set at the top level".into()));
            }
            if !matches!(assignment.value.node.value, ast::ExpressionKind::Text(_)) {
                return Err(error("dir needs a literal path in quotes".into()));
            }
            if *directory {
                return Err(error("dir is assigned more than once".into()));
            }
            *directory = true;
        }
        ast::Statement::Assignment(assignment) => {
            if reserved_binding(&assignment.name.0)
                || !names
                    .iter()
                    .rev()
                    .any(|scope| scope.contains(&assignment.name.0))
            {
                return Err(error(format!(
                    "Assignment needs an existing local binding `{}`",
                    assignment.name.0
                )));
            }
            validate_expression(&assignment.value, names, steps, depth + 1)?;
        }
        ast::Statement::Call(statement) => {
            for argument in &statement.call.arguments {
                validate_expression(argument, names, steps, depth + 1)?;
            }
        }
        ast::Statement::Conditional(conditional) => {
            validate_conditional(conditional, names, steps, depth, directory)?
        }
        ast::Statement::ForLoop(value) => {
            validate_expression(&value.iterable, names, steps, depth + 1)?;
            if reserved_binding(&value.name.0) {
                return Err(error(format!("Reserved loop binding `{}`", value.name.0)));
            }
            validate_block(
                &value.body,
                names,
                steps,
                depth + 1,
                directory,
                Some(&value.name.0),
            )?;
        }
    }
    Ok(())
}

fn validate_conditional(
    conditional: &ast::Conditional,
    names: &mut Vec<BTreeSet<String>>,
    steps: &mut usize,
    depth: usize,
    directory: &mut bool,
) -> Result<(), Error> {
    validate_expression(&conditional.condition, names, steps, depth + 1)?;
    validate_block(
        &conditional.then_block,
        names,
        steps,
        depth + 1,
        directory,
        None,
    )?;
    if let Some(otherwise) = &conditional.otherwise {
        match &otherwise.branch {
            ast::ElseBranch::Block(block) => {
                validate_block(block, names, steps, depth + 1, directory, None)?
            }
            ast::ElseBranch::Conditional(value) => {
                validate_conditional(value, names, steps, depth + 1, directory)?
            }
        }
    }
    Ok(())
}

fn validate_block(
    block: &ast::Block,
    names: &mut Vec<BTreeSet<String>>,
    steps: &mut usize,
    depth: usize,
    directory: &mut bool,
    binding: Option<&str>,
) -> Result<(), Error> {
    names.push(binding.into_iter().map(str::to_string).collect());
    let result = block
        .statements
        .iter()
        .try_for_each(|statement| validate_statement(statement, names, steps, depth, directory));
    names.pop();
    result
}

fn validate_expression(
    expression: &ast::Expression,
    names: &[BTreeSet<String>],
    steps: &mut usize,
    depth: usize,
) -> Result<(), Error> {
    use ast::ExpressionKind as E;
    *steps += 1;
    let error = |message: String| Error {
        message,
        offset: expression.node.span.0,
    };
    if depth > MAX_DEPTH || *steps > MAX_STEPS {
        return Err(error("Expression limit exceeded".into()));
    }
    let mut visit = |value: &ast::Expression| validate_expression(value, names, steps, depth + 1);
    match &expression.node.value {
        E::Name(name) if !names.iter().rev().any(|scope| scope.contains(&name.0)) => {
            return Err(error(format!("Unknown binding `{}`", name.0)));
        }
        E::Parenthesized(_, value, _) | E::Unary(_, value) => visit(value)?,
        E::Sum(left, _, right)
        | E::Product(left, _, right)
        | E::Compare(left, _, right)
        | E::And(left, _, right)
        | E::Or(left, _, right) => {
            visit(left)?;
            visit(right)?;
        }
        E::Call(call) => {
            for argument in &call.arguments {
                visit(argument)?;
            }
        }
        E::List(list) => {
            for value in &list.values {
                visit(value)?;
            }
        }
        E::Object(object) => {
            let mut keys = BTreeSet::new();
            for field in &object.fields {
                let key = match &field.key {
                    ast::ExpressionKey::Name(value) => &value.0,
                    ast::ExpressionKey::Text(value) => &value.0,
                };
                if !keys.insert(key) {
                    return Err(error(format!("Duplicate object key `{key}`")));
                }
                visit(&field.value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

pub struct Evaluator<'a, H> {
    host: &'a mut H,
    variables: Vec<BTreeMap<String, Value>>,
    declarations: BTreeMap<String, ast::Declaration>,
    steps: usize,
    bytes: usize,
    output: usize,
}

impl<'a, H: Host> Evaluator<'a, H> {
    pub fn new(host: &'a mut H) -> Self {
        Self {
            host,
            variables: vec![BTreeMap::new()],
            declarations: BTreeMap::new(),
            steps: 0,
            bytes: 0,
            output: 0,
        }
    }

    pub fn run(&mut self, document: &ast::Document) -> Result<(), Error> {
        validate(document)?;
        for declaration in &document.declarations {
            let uid = match declaration {
                ast::Declaration::Record(record) => record.opening.uid(),
                ast::Declaration::Frequency(frequency) => {
                    frequency.uid.as_ref().map(|uid| uid.0.as_str())
                }
                _ => None,
            };
            let name = match declaration {
                ast::Declaration::Record(value) => value.header.subject.slug(),
                ast::Declaration::Frequency(value) => Some(value.opening.0.as_str()),
                ast::Declaration::Karma(value) => Some(value.opening.0.as_str()),
                _ => None,
            };
            if let Some(name) = name
                && self
                    .declarations
                    .insert(name.into(), declaration.clone())
                    .is_some()
            {
                return Err(self.error(0, format!("Duplicate declaration `{name}`")));
            }
            if let Some(uid) = uid
                && name != Some(uid)
                && self
                    .declarations
                    .insert(uid.into(), declaration.clone())
                    .is_some()
            {
                return Err(self.error(0, format!("Duplicate declaration identity `{uid}`")));
            }
            match declaration {
                ast::Declaration::Extension(extension) => {
                    let namespace: String = serde_json::from_str(&extension.namespace.0)
                        .map_err(|error| self.error(0, error.to_string()))?;
                    self.declarations.insert(
                        format!("extension:{}:{namespace}", extension.target.value()),
                        declaration.clone(),
                    );
                }
                ast::Declaration::Karma(karma) => {
                    for rule in &karma.rules.rules {
                        let mut selected = karma.clone();
                        selected.rules.rules = vec![rule.clone()];
                        self.declarations.insert(
                            format!("rule:{}", rule.name.0),
                            ast::Declaration::Karma(selected.clone()),
                        );
                        if let Some(uid) = &rule.uid {
                            self.declarations.insert(
                                format!("rule:{}", uid.0),
                                ast::Declaration::Karma(selected),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
        for declaration in &document.declarations {
            if let ast::Declaration::Statement(statement) = declaration {
                self.execute_statement(statement, 0)?;
            }
        }
        Ok(())
    }

    fn tick(&mut self, offset: usize, depth: usize) -> Result<(), Error> {
        self.steps += 1;
        if depth > MAX_DEPTH || self.steps > MAX_STEPS || self.host.cancelled() {
            return Err(self.error(offset, "Execution cancelled or expression limit exceeded"));
        }
        Ok(())
    }

    fn execute_statement(&mut self, statement: &ast::Statement, depth: usize) -> Result<(), Error> {
        let offset = statement_offset(statement);
        self.tick(offset, depth)?;
        match statement {
            ast::Statement::Binding(binding) => {
                if reserved_binding(&binding.name.0)
                    || self
                        .variables
                        .last()
                        .expect("local scope")
                        .contains_key(&binding.name.0)
                {
                    return Err(self.error(
                        offset,
                        format!("Duplicate or reserved binding `{}`", binding.name.0),
                    ));
                }
                let value = self.evaluate(&binding.value, depth + 1)?;
                self.variables
                    .last_mut()
                    .expect("local scope")
                    .insert(binding.name.0.clone(), value);
            }
            ast::Statement::Assignment(assignment) if assignment.name.0 == "dir" => {}
            ast::Statement::Assignment(assignment) => {
                let scope = self
                    .variables
                    .iter()
                    .rposition(|scope| scope.contains_key(&assignment.name.0))
                    .ok_or_else(|| {
                        self.error(
                            offset,
                            format!(
                                "Assignment needs an existing local binding `{}`",
                                assignment.name.0
                            ),
                        )
                    })?;
                let value = self.evaluate(&assignment.value, depth + 1)?;
                self.variables[scope].insert(assignment.name.0.clone(), value);
            }
            ast::Statement::Call(statement) => {
                self.call(&statement.call, offset, depth + 1)?;
            }
            ast::Statement::Conditional(conditional) => {
                self.execute_conditional(conditional, depth)?
            }
            ast::Statement::ForLoop(value) => {
                let Value::List(values) = self.evaluate(&value.iterable, depth + 1)? else {
                    return Err(self.error(offset, "for needs an array"));
                };
                for item in values {
                    self.tick(offset, depth + 1)?;
                    self.execute_block(&value.body, depth + 1, Some((&value.name.0, item)))?;
                }
            }
        }
        Ok(())
    }

    fn execute_conditional(
        &mut self,
        conditional: &ast::Conditional,
        depth: usize,
    ) -> Result<(), Error> {
        let condition = self.evaluate(&conditional.condition, depth + 1)?;
        if self.truth(&condition, conditional.condition.node.span.0)? {
            self.execute_block(&conditional.then_block, depth + 1, None)?;
        } else if let Some(otherwise) = &conditional.otherwise {
            match &otherwise.branch {
                ast::ElseBranch::Block(block) => self.execute_block(block, depth + 1, None)?,
                ast::ElseBranch::Conditional(value) => {
                    self.execute_conditional(value, depth + 1)?
                }
            }
        }
        Ok(())
    }

    fn execute_block(
        &mut self,
        block: &ast::Block,
        depth: usize,
        binding: Option<(&str, Value)>,
    ) -> Result<(), Error> {
        self.variables.push(
            binding
                .into_iter()
                .map(|(name, value)| (name.to_string(), value))
                .collect(),
        );
        let result = block
            .statements
            .iter()
            .try_for_each(|statement| self.execute_statement(statement, depth));
        self.variables.pop();
        result
    }

    pub fn evaluate(&mut self, expression: &ast::Expression, depth: usize) -> Result<Value, Error> {
        use ast::ExpressionKind as E;
        let offset = expression.node.span.0;
        self.tick(offset, depth)?;
        let result = match &expression.node.value {
            E::Number(value) => Value::Number(
                DecimalValue::parse_inferred(&value.0)
                    .map_err(|error| self.error(offset, error.to_string()))?,
            ),
            E::Duration(value) => Value::Duration(
                crate::expr::parse_duration(&value.0)
                    .ok_or_else(|| self.error(offset, "Invalid duration"))?,
            ),
            E::Text(value) => Value::Text(value.0.clone()),
            E::Body(value) => Value::Text(value.0.clone()),
            E::Reference(value) => Value::Reference(value.0.clone()),
            E::Concept(value) => Value::Concept(value.0.clone()),
            E::True(_) => Value::Bool(true),
            E::False(_) => Value::Bool(false),
            E::Null(_) => Value::Null,
            E::Name(value) => self
                .variables
                .iter()
                .rev()
                .find_map(|scope| scope.get(&value.0))
                .cloned()
                .ok_or_else(|| self.error(offset, format!("Unknown binding `{}`", value.0)))?,
            E::Call(value) => self.call(value, offset, depth + 1)?,
            E::Parenthesized(_, value, _) => self.evaluate(value, depth + 1)?,
            E::List(value) => Value::List(
                value
                    .values
                    .iter()
                    .map(|value| self.evaluate(value, depth + 1))
                    .collect::<Result<_, _>>()?,
            ),
            E::Object(value) => {
                let mut values = BTreeMap::new();
                for field in &value.fields {
                    let key = match &field.key {
                        ast::ExpressionKey::Name(value) => &value.0,
                        ast::ExpressionKey::Text(value) => &value.0,
                    };
                    if values.contains_key(key) {
                        return Err(self.error(offset, format!("Duplicate object key `{key}`")));
                    }
                    values.insert(key.clone(), self.evaluate(&field.value, depth + 1)?);
                }
                Value::Object(values)
            }
            E::And(left, _, right) => {
                let left = self.evaluate(left, depth + 1)?;
                Value::Bool(
                    self.truth(&left, offset)? && {
                        let right = self.evaluate(right, depth + 1)?;
                        self.truth(&right, offset)?
                    },
                )
            }
            E::Or(left, _, right) => {
                let left = self.evaluate(left, depth + 1)?;
                Value::Bool(
                    self.truth(&left, offset)? || {
                        let right = self.evaluate(right, depth + 1)?;
                        self.truth(&right, offset)?
                    },
                )
            }
            E::Unary(operator, value) => {
                let value = self.evaluate(value, depth + 1)?;
                match (operator, value) {
                    (ast::UnaryOperator::Not(_), value) => {
                        Value::Bool(!self.truth(&value, offset)?)
                    }
                    (ast::UnaryOperator::Positive(_), Value::Number(value)) => Value::Number(value),
                    (ast::UnaryOperator::Negative(_), Value::Number(value)) => Value::Number(
                        value
                            .checked_neg()
                            .ok_or_else(|| self.error(offset, "Numeric overflow"))?,
                    ),
                    _ => return Err(self.error(offset, "Unary arithmetic requires a number")),
                }
            }
            E::Sum(left, operator, right) => self.binary(
                left,
                match operator {
                    ast::SumOperator::Add(_) => BinOp::Add,
                    ast::SumOperator::Subtract(_) => BinOp::Sub,
                },
                right,
                offset,
                depth,
            )?,
            E::Product(left, operator, right) => self.binary(
                left,
                match operator {
                    ast::ProductOperator::Multiply(_) => BinOp::Mul,
                    ast::ProductOperator::Divide(_) => BinOp::Div,
                    ast::ProductOperator::Remainder(_) => BinOp::Rem,
                },
                right,
                offset,
                depth,
            )?,
            E::Compare(left, operator, right) => self.binary(
                left,
                match operator {
                    ast::CompareOperator::Less(_) => BinOp::Lt,
                    ast::CompareOperator::LessEqual(_) => BinOp::Le,
                    ast::CompareOperator::Greater(_) => BinOp::Gt,
                    ast::CompareOperator::GreaterEqual(_) => BinOp::Ge,
                    ast::CompareOperator::Equal(_) => BinOp::Eq,
                    ast::CompareOperator::NotEqual(_) => BinOp::Ne,
                },
                right,
                offset,
                depth,
            )?,
        };
        self.bytes = self.bytes.saturating_add(result.render().len());
        if self.bytes > MAX_BYTES {
            return Err(self.error(offset, "Value exceeds memory limit"));
        }
        Ok(result)
    }

    fn binary(
        &mut self,
        left: &ast::Expression,
        operator: BinOp,
        right: &ast::Expression,
        offset: usize,
        depth: usize,
    ) -> Result<Value, Error> {
        let left = self.evaluate(left, depth + 1)?;
        let right = self.evaluate(right, depth + 1)?;
        if operator == BinOp::Add
            && let (Value::Text(left), Value::Text(right)) = (&left, &right)
        {
            if left.len().saturating_add(right.len()) > MAX_BYTES {
                return Err(self.error(offset, "Text exceeds memory limit"));
            }
            return Ok(Value::Text(format!("{left}{right}")));
        }
        if matches!(operator, BinOp::Eq | BinOp::Ne) {
            let equal = match (&left, &right) {
                (Value::Number(a), Value::Number(b)) => a.exact_numeric_cmp(*b).is_eq(),
                _ => left == right,
            };
            return Ok(Value::Bool(if operator == BinOp::Eq {
                equal
            } else {
                !equal
            }));
        }
        let (Value::Number(left), Value::Number(right)) = (left, right) else {
            return Err(self.error(
                offset,
                "Arithmetic and ordering require numbers; use str(...) to convert text",
            ));
        };
        let ordering = left.exact_numeric_cmp(right);
        Ok(match operator {
            BinOp::Lt => Value::Bool(ordering.is_lt()),
            BinOp::Le => Value::Bool(ordering.is_le()),
            BinOp::Gt => Value::Bool(ordering.is_gt()),
            BinOp::Ge => Value::Bool(ordering.is_ge()),
            _ => Value::Number(
                crate::karma::condition::arithmetic(operator, left, right)
                    .map_err(|error| self.error(offset, error.to_string()))?,
            ),
        })
    }

    fn call(&mut self, call: &ast::Call, offset: usize, depth: usize) -> Result<Value, Error> {
        self.tick(offset, depth)?;
        let arguments = call
            .arguments
            .iter()
            .map(|argument| self.evaluate(argument, depth + 1))
            .collect::<Result<Vec<_>, _>>()?;
        let result: Result<Value, String> = match (call.name.0.as_str(), arguments.as_slice()) {
            ("str", [value]) => Ok(Value::Text(value.render())),
            ("push", [Value::List(values), value]) => {
                let mut values = values.clone();
                values.push(value.clone());
                Ok(Value::List(values))
            }
            ("print", values) => {
                let mut text = values
                    .iter()
                    .map(Value::render)
                    .collect::<Vec<_>>()
                    .join(" ");
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                self.output = self.output.saturating_add(text.len());
                if self.output > MAX_BYTES {
                    return Err(self.error(offset, "Output limit exceeded"));
                }
                self.host.print(&text).map(|_| Value::Null)
            }
            ("len", [Value::List(values)]) => DecimalValue::from_mantissa(0, values.len() as i128)
                .map(Value::Number)
                .map_err(|error| error.to_string()),
            ("len", [Value::Text(value)]) => {
                DecimalValue::from_mantissa(0, value.chars().count() as i128)
                    .map(Value::Number)
                    .map_err(|error| error.to_string())
            }
            ("get", [Value::Object(values), Value::Text(key)]) => values
                .get(key)
                .cloned()
                .ok_or_else(|| format!("No property `{key}`")),
            ("get", [Value::List(values), Value::Number(index)]) if index.scale() == 0 => {
                usize::try_from(index.mantissa())
                    .ok()
                    .and_then(|index| values.get(index))
                    .cloned()
                    .ok_or_else(|| "Index outside the list".into())
            }
            (
                "record" | "frequency" | "karma" | "declaration",
                [Value::Reference(name) | Value::Text(name)],
            ) if self.declarations.contains_key(name) => {
                Ok(Value::Declaration(anicca::format(&ast::Document {
                    declarations: vec![self.declarations[name].clone()],
                })))
            }
            (
                "extension",
                [
                    Value::Reference(target) | Value::Text(target),
                    Value::Text(namespace),
                ],
            ) if self
                .declarations
                .contains_key(&format!("extension:{target}:{namespace}")) =>
            {
                Ok(Value::Declaration(anicca::format(&ast::Document {
                    declarations: vec![
                        self.declarations[&format!("extension:{target}:{namespace}")].clone(),
                    ],
                })))
            }
            ("rule", [Value::Reference(name) | Value::Text(name)])
                if self.declarations.contains_key(&format!("rule:{name}")) =>
            {
                Ok(Value::Declaration(anicca::format(&ast::Document {
                    declarations: vec![self.declarations[&format!("rule:{name}")].clone()],
                })))
            }
            ("quantity", [Value::Reference(name) | Value::Text(name)])
                if self.declarations.contains_key(name) =>
            {
                match &self.declarations[name] {
                    ast::Declaration::Record(record) => {
                        DecimalValue::parse_inferred(&record.header.subject.quantity().value.0)
                            .map(Value::Number)
                            .map_err(|error| error.to_string())
                    }
                    _ => Err("quantity() needs a Record".into()),
                }
            }
            ("head" | "body" | "slug", [Value::Reference(name) | Value::Text(name)])
                if self.declarations.contains_key(name) =>
            {
                self.local_property(name, &call.name.0)
            }
            (
                "property",
                [
                    Value::Reference(name) | Value::Text(name),
                    Value::Text(property),
                ],
            ) if self.declarations.contains_key(name) => self.local_property(name, property),
            ("str" | "len" | "get" | "push", _) => {
                Err(format!("Invalid arguments to {}()", call.name.0))
            }
            (name, values) => self.host.call(name, values),
        };
        result.map_err(|message| self.error(offset, message))
    }

    fn local_property(&self, name: &str, property: &str) -> Result<Value, String> {
        let ast::Declaration::Record(record) = &self.declarations[name] else {
            return Err("A Record property requires a Record".into());
        };
        Ok(match property {
            "head" => Value::Text(record.title.value()),
            "body" => Value::Text(
                record
                    .description
                    .as_ref()
                    .map(|value| value.0.clone())
                    .unwrap_or_default(),
            ),
            "slug" => record
                .header
                .subject
                .slug()
                .map(|slug| Value::Text(slug.into()))
                .unwrap_or(Value::Null),
            "uid" => record
                .opening
                .uid()
                .map(|uid| Value::Text(uid.into()))
                .unwrap_or(Value::Null),
            "quantity" => Value::Number(
                DecimalValue::parse_inferred(&record.header.subject.quantity().value.0)
                    .map_err(|error| error.to_string())?,
            ),
            "unit" => record
                .header
                .subject
                .quantity()
                .unit
                .as_ref()
                .map(|value| Value::Reference(value.name.0.clone()))
                .unwrap_or(Value::Null),
            _ => return Err(format!("Record property `{property}` is unavailable")),
        })
    }

    fn truth(&self, value: &Value, offset: usize) -> Result<bool, Error> {
        match value {
            Value::Bool(value) => Ok(*value),
            Value::Number(value) => Ok(value.mantissa() != 0),
            _ => Err(self.error(offset, "A condition needs a Boolean or number")),
        }
    }

    fn error(&self, offset: usize, message: impl Into<String>) -> Error {
        Error {
            message: message.into(),
            offset,
        }
    }
}
