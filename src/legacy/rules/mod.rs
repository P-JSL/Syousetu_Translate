//! Read-only reconstruction of legacy rule *syntax*, not a translation VM.
//! Literal bytes remain borrowed because source and target encodings differ.
mod audit;
mod codec;
mod syntax;
pub use audit::{inspect, Inspection, TableReport};
pub use codec::complement;
pub use syntax::{
    parse, Element, Field, Join, Operator, ParseError, Predicate, RelativeOffset, Rule, Value,
};

#[cfg(test)]
mod tests;
