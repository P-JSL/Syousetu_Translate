//! A lossless byte parser for observed field/operator/literal syntax.
//! Operators and separators are recorded without assigning execution semantics.
use std::fmt;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RelativeOffset(i16);
impl RelativeOffset {
    pub fn get(self) -> i16 {
        self.0
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Field {
    pub tag: u8,
    pub offset: RelativeOffset,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operator {
    Equal,
    NotEqual,
    Assign,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Join {
    Bar,
    Ampersand,
}
#[derive(Debug, Eq, PartialEq)]
pub enum Value<'a> {
    Literal(&'a [u8]),
    Field(Field),
    Set {
        first: &'a [u8],
        rest: Vec<(Join, &'a [u8])>,
    },
}
#[derive(Debug, Eq, PartialEq)]
pub struct Predicate<'a> {
    pub field: Field,
    pub operator: Operator,
    pub value: Value<'a>,
}
#[derive(Debug, Eq, PartialEq)]
pub enum Element<'a> {
    Predicate(Predicate<'a>),
    Separator(usize),
}
#[derive(Debug, Eq, PartialEq)]
pub struct Rule<'a> {
    pub raw: &'a [u8],
    pub elements: Vec<Element<'a>>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseError {
    pub offset: usize,
    pub reason: &'static str,
}
impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "rule syntax at byte {}: {}", self.offset, self.reason)
    }
}
impl std::error::Error for ParseError {}
type Result<T> = std::result::Result<T, ParseError>;

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Cursor<'a> {
    fn error(&self, reason: &'static str) -> ParseError {
        ParseError {
            offset: self.at,
            reason,
        }
    }
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }
    fn spaces(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_whitespace()) {
            self.at += 1;
        }
    }
    fn expect(&mut self, byte: u8, reason: &'static str) -> Result<()> {
        if self.peek() != Some(byte) {
            return Err(self.error(reason));
        }
        self.at += 1;
        Ok(())
    }
    fn literal(&mut self) -> Result<&'a [u8]> {
        self.expect(b'"', "expected opening quote")?;
        let start = self.at;
        while let Some(byte) = self.peek() {
            if byte == b'"' {
                let value = &self.bytes[start..self.at];
                self.at += 1;
                return Ok(value);
            }
            // A Shift-JIS trail byte can be 0x5c. It is not an ASCII escape.
            // Preserve all non-quote bytes, including such backslashes.
            self.at += 1;
        }
        Err(self.error("unterminated literal"))
    }
    fn value(&mut self) -> Result<Value<'a>> {
        if self.peek().is_some_and(|byte| byte.is_ascii_uppercase()) {
            return self.field().map(Value::Field);
        }
        if self.peek() != Some(b'{') {
            return self.literal().map(Value::Literal);
        }
        self.at += 1;
        self.spaces();
        let first = self.literal()?;
        let mut rest = Vec::new();
        loop {
            self.spaces();
            let join = match self.peek() {
                Some(b'|') => Join::Bar,
                Some(b'&') => Join::Ampersand,
                Some(b'}') => {
                    self.at += 1;
                    break;
                }
                _ => return Err(self.error("expected set separator or closing brace")),
            };
            self.at += 1;
            self.spaces();
            rest.push((join, self.literal()?));
        }
        Ok(Value::Set { first, rest })
    }
    fn field(&mut self) -> Result<Field> {
        let tag = self
            .peek()
            .filter(u8::is_ascii_uppercase)
            .ok_or_else(|| self.error("expected field tag"))?;
        self.at += 1;
        let start = self.at;
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        let digits = self.at;
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.at += 1;
        }
        if self.at == digits {
            return Err(self.error("expected relative offset"));
        }
        let number = std::str::from_utf8(&self.bytes[start..self.at])
            .map_err(|_| self.error("non-ASCII offset"))?
            .parse()
            .map_err(|_| self.error("relative offset exceeds i16"))?;
        Ok(Field {
            tag,
            offset: RelativeOffset(number),
        })
    }
    fn predicate(&mut self) -> Result<Predicate<'a>> {
        let field = self.field()?;
        self.spaces();
        let operator = match self.peek() {
            Some(b'!') => {
                self.at += 1;
                self.expect(b'=', "expected !=")?;
                Operator::NotEqual
            }
            Some(b'=') => {
                self.at += 1;
                if self.peek() == Some(b'=') {
                    self.at += 1;
                    Operator::Equal
                } else {
                    Operator::Assign
                }
            }
            _ => return Err(self.error("expected =, == or !=")),
        };
        self.spaces();
        Ok(Predicate {
            field,
            operator,
            value: self.value()?,
        })
    }
}

pub fn parse(bytes: &[u8]) -> Result<Rule<'_>> {
    let mut cursor = Cursor { bytes, at: 0 };
    let mut elements = Vec::new();
    let mut predicates = 0;
    cursor.spaces();
    while cursor.peek().is_some() {
        if cursor.peek() == Some(b';') {
            let start = cursor.at;
            while cursor.peek() == Some(b';') {
                cursor.at += 1;
            }
            elements.push(Element::Separator(cursor.at - start));
        } else {
            elements.push(Element::Predicate(cursor.predicate()?));
            predicates += 1;
            cursor.spaces();
            if cursor.peek().is_some_and(|byte| byte != b';') {
                return Err(cursor.error("expected clause boundary"));
            }
        }
        cursor.spaces();
    }
    if predicates == 0 {
        return Err(cursor.error("rule has no predicates"));
    }
    Ok(Rule {
        raw: bytes,
        elements,
    })
}
