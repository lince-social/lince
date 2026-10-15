#[derive(Clone, Debug)]
pub struct Formula(Vec<Instruction>);

#[derive(Clone, Debug)]
enum Instruction {
    Constant(f64),
    Variable(u8),
    Negate,
    Binary(u8),
    Function(String, usize),
}

struct Parser<'a> {
    source: &'a [u8],
    cursor: usize,
    instructions: Vec<Instruction>,
    depth: usize,
}

impl Formula {
    pub fn compile(source: &str) -> Result<Self, String> {
        if source.is_empty() || source.len() > 4096 || !source.is_ascii() {
            return Err("A formula must contain 1 to 4096 ASCII bytes.".into());
        }
        let mut parser = Parser {
            source: source.as_bytes(),
            cursor: 0,
            instructions: Vec::new(),
            depth: 0,
        };
        parser.expression(0)?;
        parser.space();
        if parser.cursor != parser.source.len() {
            return Err(format!("Unexpected input at byte {}.", parser.cursor));
        }
        Ok(Self(parser.instructions))
    }

    pub fn evaluate(&self, x: f64, y: f64) -> Result<f64, String> {
        let mut stack = [0.0_f64; 256];
        let mut count = 0;
        for instruction in &self.0 {
            let value = match instruction {
                Instruction::Constant(value) => *value,
                Instruction::Variable(variable) => match variable {
                    0 => x,
                    1 => y,
                    2 => x.hypot(y),
                    _ => y.atan2(x),
                },
                Instruction::Negate => {
                    count -= 1;
                    -stack[count]
                }
                Instruction::Binary(operator) => {
                    count -= 2;
                    let (a, b) = (stack[count], stack[count + 1]);
                    match operator {
                        b'+' => a + b,
                        b'-' => a - b,
                        b'*' => a * b,
                        b'/' => a / b,
                        _ => a.powf(b),
                    }
                }
                Instruction::Function(name, arity) => {
                    count -= arity;
                    let a = stack[count];
                    let b = stack[count + 1];
                    match name.as_str() {
                        "sin" => a.sin(),
                        "cos" => a.cos(),
                        "sqrt" => a.sqrt(),
                        "exp" => a.exp(),
                        "abs" => a.abs(),
                        "min" => a.min(b),
                        "max" => a.max(b),
                        "pow" => a.powf(b),
                        "atan2" => a.atan2(b),
                        _ => unreachable!(),
                    }
                }
            };
            if !value.is_finite() {
                return Err("Formula produced a non-finite value.".into());
            }
            stack[count] = value;
            count += 1;
        }
        Ok(stack[0])
    }
}

impl Parser<'_> {
    fn space(&mut self) {
        while self
            .source
            .get(self.cursor)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.cursor += 1;
        }
    }

    fn push(&mut self, instruction: Instruction) -> Result<(), String> {
        if self.instructions.len() >= 255 {
            return Err("Formula exceeds 255 operations.".into());
        }
        self.instructions.push(instruction);
        Ok(())
    }

    fn expression(&mut self, minimum: u8) -> Result<(), String> {
        self.depth += 1;
        if self.depth > 32 {
            return Err("Formula exceeds 32 nested expressions.".into());
        }
        self.atom()?;
        loop {
            self.space();
            let Some(operator) = self.source.get(self.cursor).copied() else {
                break;
            };
            let precedence = match operator {
                b'+' | b'-' => 1,
                b'*' | b'/' => 2,
                b'^' => 4,
                _ => break,
            };
            if precedence < minimum {
                break;
            }
            self.cursor += 1;
            self.expression(if operator == b'^' {
                precedence
            } else {
                precedence + 1
            })?;
            self.push(Instruction::Binary(operator))?;
        }
        self.depth -= 1;
        Ok(())
    }

    fn atom(&mut self) -> Result<(), String> {
        self.space();
        let Some(token) = self.source.get(self.cursor).copied() else {
            return Err("Expected a number, variable, or function.".into());
        };
        if token == b'-' || token == b'+' {
            self.cursor += 1;
            self.expression(3)?;
            if token == b'-' {
                self.push(Instruction::Negate)?;
            }
            return Ok(());
        }
        if token == b'(' {
            self.cursor += 1;
            self.expression(0)?;
            return self.expect(b')');
        }
        if token.is_ascii_digit() || token == b'.' {
            let start = self.cursor;
            while self
                .source
                .get(self.cursor)
                .is_some_and(|c| c.is_ascii_digit() || *c == b'.')
            {
                self.cursor += 1;
            }
            if self
                .source
                .get(self.cursor)
                .is_some_and(|c| *c == b'e' || *c == b'E')
            {
                self.cursor += 1;
                if self
                    .source
                    .get(self.cursor)
                    .is_some_and(|c| *c == b'+' || *c == b'-')
                {
                    self.cursor += 1;
                }
                while self.source.get(self.cursor).is_some_and(u8::is_ascii_digit) {
                    self.cursor += 1;
                }
            }
            let number = std::str::from_utf8(&self.source[start..self.cursor])
                .unwrap()
                .parse::<f64>()
                .map_err(|_| "Invalid number.")?;
            if !number.is_finite() {
                return Err("Use finite numbers.".into());
            }
            return self.push(Instruction::Constant(number));
        }
        let start = self.cursor;
        while self
            .source
            .get(self.cursor)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'_')
        {
            self.cursor += 1;
        }
        if start == self.cursor {
            return Err(format!("Unexpected input at byte {}.", self.cursor));
        }
        let name = std::str::from_utf8(&self.source[start..self.cursor]).unwrap();
        match name {
            "x" => return self.push(Instruction::Variable(0)),
            "y" => return self.push(Instruction::Variable(1)),
            "r" => return self.push(Instruction::Variable(2)),
            "theta" => return self.push(Instruction::Variable(3)),
            "pi" => return self.push(Instruction::Constant(std::f64::consts::PI)),
            _ => {}
        }
        let arity = match name {
            "sin" | "cos" | "sqrt" | "exp" | "abs" => 1,
            "min" | "max" | "pow" | "atan2" => 2,
            _ => return Err(format!("Unknown variable or function: {name}.")),
        };
        self.expect(b'(')?;
        for index in 0..arity {
            if index > 0 {
                self.expect(b',')?;
            }
            self.expression(0)?;
        }
        self.expect(b')')?;
        self.push(Instruction::Function(name.into(), arity))
    }

    fn expect(&mut self, token: u8) -> Result<(), String> {
        self.space();
        if self.source.get(self.cursor) != Some(&token) {
            return Err(format!(
                "Expected '{}' at byte {}.",
                char::from(token),
                self.cursor
            ));
        }
        self.cursor += 1;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_variables_and_functions() {
        assert_eq!(
            Formula::compile("-2^2+pow(3,2)")
                .unwrap()
                .evaluate(0.0, 0.0)
                .unwrap(),
            5.0
        );
        assert_eq!(
            Formula::compile("r+x+y")
                .unwrap()
                .evaluate(3.0, 4.0)
                .unwrap(),
            12.0
        );
        assert!(
            Formula::compile("sin(pi/2)")
                .unwrap()
                .evaluate(0.0, 0.0)
                .unwrap()
                > 0.999
        );
    }

    #[test]
    fn malformed_and_unbounded_formulas_fail_without_execution() {
        for source in [
            "",
            "x()",
            "system(1)",
            "1+",
            "pow(2)",
            "1e999",
            "@record",
            "x;1",
        ] {
            assert!(Formula::compile(source).is_err(), "{source}");
        }
        assert!(Formula::compile(&format!("{}1{}", "(".repeat(40), ")".repeat(40))).is_err());
        assert!(Formula::compile(&"1+".repeat(300)).is_err());
        assert!(Formula::compile("1/0").unwrap().evaluate(0.0, 0.0).is_err());
        assert!(
            Formula::compile("sqrt(-1)")
                .unwrap()
                .evaluate(0.0, 0.0)
                .is_err()
        );
    }
}
