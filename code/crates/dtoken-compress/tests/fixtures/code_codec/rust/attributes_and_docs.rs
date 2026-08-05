#![allow(dead_code)]

/// A doc comment must remain visible to the parser.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Envelope {
    value: String,
}

impl Envelope {
    pub fn new(value: impl Into<String>) -> Self {
        Self { value: value.into() }
    }
}
