// S-expressions as KiCad writes them: lists, bare atoms and quoted strings
// with \" and \\ escapes. Atoms and strings both become Atom.
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum Sexpr {
    Atom(String),
    List(Vec<Sexpr>)
}

#[derive(Debug)]
pub struct ParseError {
    line: usize,
    msg: &'static str
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

impl std::error::Error for ParseError {}

impl Sexpr {
    pub fn atom(&self) -> Option<&str> {
        match self {
            Sexpr::Atom(atom) => Some(atom),
            Sexpr::List(_) => None
        }
    }

    // Name of a list: its first atom, as in (name ...).
    pub fn name(&self) -> Option<&str> {
        match self {
            Sexpr::List(items) => items.first().and_then(Sexpr::atom),
            Sexpr::Atom(_) => None
        }
    }

    // Items of a list after its name.
    pub fn args(&self) -> &[Sexpr] {
        match self {
            Sexpr::List(items) if !items.is_empty() => &items[1..],
            _ => &[]
        }
    }

    // Child lists called name.
    pub fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Sexpr> + 'a {
        self.args().iter().filter(move |item| item.name() == Some(name))
    }

    pub fn child(&self, name: &str) -> Option<&Sexpr> {
        self.args().iter().find(|item| item.name() == Some(name))
    }

    // First argument of the child list called name, as in (name "value").
    pub fn value(&self, name: &str) -> Option<&str> {
        self.child(name)?.args().first()?.atom()
    }
}

pub fn parse(text: &str) -> Result<Sexpr, ParseError> {
    let mut chars = text.chars().peekable();
    let mut line = 1;
    // Lists being read; the bottom one collects the top-level expressions.
    let mut stack: Vec<Vec<Sexpr>> = vec![vec![]];
    let err = |line, msg| Err(ParseError { line, msg });

    while let Some(c) = chars.next() {
        match c {
            '\n' => line += 1,
            c if c.is_whitespace() => (),
            '(' => stack.push(vec![]),
            ')' => {
                if stack.len() == 1 {
                    return err(line, "unexpected )");
                }
                let list = stack.pop().unwrap();
                stack.last_mut().unwrap().push(Sexpr::List(list));
            },
            '"' => {
                let mut atom = String::new();
                loop {
                    match chars.next() {
                        None => return err(line, "unterminated string"),
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some('n') => atom.push('\n'),
                            Some(c) => atom.push(c),
                            None => return err(line, "unterminated string")
                        },
                        Some(c) => {
                            if c == '\n' {
                                line += 1;
                            }
                            atom.push(c);
                        }
                    }
                }
                stack.last_mut().unwrap().push(Sexpr::Atom(atom));
            },
            c => {
                let mut atom = c.to_string();
                while let Some(&c) = chars.peek() {
                    if c.is_whitespace() || c == '(' || c == ')' || c == '"' {
                        break;
                    }
                    atom.push(c);
                    chars.next();
                }
                stack.last_mut().unwrap().push(Sexpr::Atom(atom));
            }
        }
    }

    if stack.len() > 1 {
        return err(line, "missing )");
    }
    let mut top = stack.pop().unwrap();
    if top.len() != 1 {
        return err(line, "expected one expression");
    }
    Ok(top.pop().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn atom(s: &str) -> Sexpr {
        Sexpr::Atom(s.to_string())
    }

    #[test]
    fn parses_nested_lists_and_strings() {
        let expr = parse("(net (code \"1\") (name \"/A \\\"b\\\" \\\\\"))").unwrap();
        assert_eq!(expr, Sexpr::List(vec![
            atom("net"),
            Sexpr::List(vec![atom("code"), atom("1")]),
            Sexpr::List(vec![atom("name"), atom("/A \"b\" \\")])
        ]));
        assert_eq!(expr.value("name"), Some("/A \"b\" \\"));
        assert_eq!(expr.value("code"), Some("1"));
        assert_eq!(expr.value("pin"), None);
    }

    #[test]
    fn reports_errors_with_line() {
        assert_eq!(parse("(a\n(b)").unwrap_err().to_string(), "line 2: missing )");
        assert_eq!(parse("(a))").unwrap_err().to_string(), "line 1: unexpected )");
        assert_eq!(parse("(a \"b)").unwrap_err().to_string(), "line 1: unterminated string");
        assert_eq!(parse("(a) (b)").unwrap_err().to_string(), "line 1: expected one expression");
    }
}
