//! A bounded structural TeX grammar. Equality is token fidelity, not algebraic inference.
use std::collections::BTreeSet;

const MAX_TOKENS: usize = 4096;
const MAX_DEPTH: usize = 64;
#[derive(Debug, PartialEq)]
pub(crate) struct Parsed {
    pub(crate) tokens: Vec<String>,
    pub(crate) symbols: BTreeSet<String>,
    pub(crate) uses_dfrac: bool,
}
fn symbol(token: &str) -> bool {
    (token.len() == 1 && token.as_bytes()[0].is_ascii_alphabetic())
        || matches!(token, "\\alpha" | "\\beta" | "\\gamma")
}
pub(crate) fn parse(text: &str) -> Result<Parsed, &'static str> {
    if text.len() > 65536 || text.is_empty() {
        return Err("formula size or empty expression");
    }
    let mut tokens = vec![];
    let mut uses_dfrac = false;
    let bytes = text.as_bytes();
    let mut offset = 0;
    while offset < bytes.len() {
        let start = offset;
        let b = bytes[offset];
        if matches!(b, b' ' | b'\t' | b'\r' | b'\n') {
            offset += 1;
            continue;
        }
        let token = if b == b'\\' {
            offset += 1;
            while offset < bytes.len() && bytes[offset].is_ascii_alphabetic() {
                offset += 1;
            }
            let command = &text[start..offset];
            match command {
                "\\frac" | "\\exp" | "\\alpha" | "\\beta" | "\\gamma" | "\\le" | "\\ge" => command,
                "\\dfrac" => {
                    uses_dfrac = true;
                    "\\frac"
                }
                _ => return Err("unknown or unsafe TeX command"),
            }
        } else if b.is_ascii_digit() {
            offset += 1;
            while offset < bytes.len() && bytes[offset].is_ascii_digit() {
                offset += 1;
            }
            if offset < bytes.len() && bytes[offset] == b'.' {
                offset += 1;
                let decimal_start = offset;
                while offset < bytes.len() && bytes[offset].is_ascii_digit() {
                    offset += 1;
                }
                if decimal_start == offset {
                    return Err("decimal must end with a digit");
                }
            }
            &text[start..offset]
        } else if b.is_ascii_alphabetic() || b"=+-*/(){}_^[ ]<>".contains(&b) {
            offset += 1;
            &text[start..offset]
        } else {
            return Err("unsupported character in formula");
        };
        tokens.push(token.to_owned());
        if tokens.len() > MAX_TOKENS {
            return Err("formula token limit");
        }
    }
    let mut parser = Parser {
        tokens: &tokens,
        at: 0,
    };
    parser.expression(0)?;
    if parser.at != tokens.len() {
        return Err("unconsumed formula tokens");
    }
    let symbols = tokens.iter().filter(|t| symbol(t)).cloned().collect();
    Ok(Parsed {
        tokens,
        symbols,
        uses_dfrac,
    })
}
struct Parser<'a> {
    tokens: &'a [String],
    at: usize,
}
impl Parser<'_> {
    fn peek(&self) -> &str {
        self.tokens.get(self.at).map(String::as_str).unwrap_or("")
    }
    fn expression(&mut self, depth: usize) -> Result<(), &'static str> {
        self.sum(depth)?;
        while matches!(self.peek(), "=" | "<" | ">" | "\\le" | "\\ge") {
            self.at += 1;
            self.sum(depth)?;
        }
        Ok(())
    }
    fn sum(&mut self, depth: usize) -> Result<(), &'static str> {
        self.product(depth)?;
        while matches!(self.peek(), "+" | "-") {
            self.at += 1;
            if matches!(self.peek(), "+" | "-") {
                return Err("adjacent additive signs require explicit grouping");
            }
            self.product(depth)?;
        }
        Ok(())
    }
    fn factor_start(&self) -> bool {
        symbol(self.peek())
            || self
                .peek()
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_digit)
            || matches!(self.peek(), "(" | "{" | "[" | "\\frac" | "\\exp")
    }
    fn product(&mut self, depth: usize) -> Result<(), &'static str> {
        if matches!(self.peek(), "+" | "-") {
            self.at += 1;
        }
        self.factor(depth)?;
        loop {
            if matches!(self.peek(), "*" | "/") {
                self.at += 1;
                if matches!(self.peek(), "+" | "-") {
                    self.at += 1;
                }
                self.factor(depth)?;
            } else if self.factor_start() {
                self.factor(depth)?;
            } else {
                break;
            }
        }
        Ok(())
    }
    fn group(&mut self, depth: usize, required: Option<&str>) -> Result<(), &'static str> {
        if depth >= MAX_DEPTH {
            return Err("formula nesting limit");
        }
        let opener = self.peek();
        if required.is_some_and(|r| r != opener) {
            return Err("required TeX group missing");
        }
        let close = match opener {
            "(" => ")",
            "{" => "}",
            "[" => "]",
            _ => return Err("expected TeX group"),
        };
        self.at += 1;
        self.expression(depth + 1)?;
        if self.peek() != close {
            return Err("unbalanced or empty TeX group");
        }
        self.at += 1;
        Ok(())
    }
    fn factor(&mut self, depth: usize) -> Result<(), &'static str> {
        match self.peek() {
            "\\frac" => {
                self.at += 1;
                self.group(depth, Some("{"))?;
                self.group(depth, Some("{"))?;
            }
            "\\exp" => {
                self.at += 1;
                if !matches!(self.peek(), "(" | "{") {
                    return Err("exp requires a delimited argument");
                }
                self.group(depth, None)?;
            }
            "(" | "{" | "[" => self.group(depth, None)?,
            token if symbol(token) || token.as_bytes().first().is_some_and(u8::is_ascii_digit) => {
                self.at += 1;
            }
            _ => return Err("formula operand missing"),
        }
        let mut scripts = BTreeSet::new();
        while matches!(self.peek(), "_" | "^") {
            if !scripts.insert(self.peek().to_owned()) {
                return Err("duplicate TeX script");
            }
            self.at += 1;
            if self.peek() == "{" {
                self.group(depth, Some("{"))?;
            } else if symbol(self.peek())
                || (self.peek().len() == 1 && self.peek().as_bytes()[0].is_ascii_digit())
            {
                self.at += 1;
            } else {
                return Err("script argument missing or ambiguous");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fidelity_preserves_scripts_signs_relations_and_fraction_arguments() {
        let p = parse("C_i(t) = -\\dfrac{a}{b} t^{2}\r\n").unwrap();
        assert!(p.uses_dfrac);
        assert_eq!(
            p,
            Parsed {
                uses_dfrac: true,
                ..parse("C_i(t)=-\\frac{a}{b}t^{2}").unwrap()
            }
        );
        assert_eq!(
            p.symbols,
            ["C", "i", "t", "a", "b"].map(str::to_owned).into()
        );
        for changed in [
            "C_j(t)=-\\frac{a}{b}t^{2}",
            "C_i(t)=\\frac{a}{b}t^{2}",
            "C_i(t)=-\\frac{b}{a}t^{2}",
            "C_i(t)=-\\frac{a}{b}t_{2}",
        ] {
            assert_ne!(p.tokens, parse(changed).unwrap().tokens);
        }
        assert!(parse("C_i(t)=C_0\\exp(-k t)").is_ok());
        assert!(parse("0\\le\\alpha<1").is_ok());
    }
    #[test]
    fn rendering_syntax_and_unbounded_nesting_never_pass_as_semantics() {
        for bad in [
            "",
            "x_",
            "x^",
            "x_12",
            "x^{}",
            "x_{i",
            "x_{i]",
            "x_^2",
            "x^{2}^{3}",
            "=x",
            "x=",
            "x++y",
            "x**y",
            "\\frac{x}",
            "\\frac{}{y}",
            "\\frac12",
            "\\exp",
            "\\exp x",
            "x$",
            "x%\ny",
            "\\href{evil}{x}",
            "x.`",
            "\\mystery{x}",
            "1.",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
        assert!(parse(&format!("{}x{}", "{".repeat(65), "}".repeat(65))).is_err());
    }
}
