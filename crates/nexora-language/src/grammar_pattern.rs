use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum GrammarPattern {
    Literal {
        text: String,
        #[serde(default)]
        kind: Option<String>,
    },
    Token {
        kind: String,
    },
    Sequence {
        items: Vec<Self>,
    },
    Choice {
        alternatives: Vec<Self>,
    },
    Repeat {
        item: Box<Self>,
        min: usize,
        max: usize,
    },
    Optional {
        item: Box<Self>,
    },
    Capture {
        name: String,
        item: Box<Self>,
        #[serde(default)]
        join: String,
    },
    Rule {
        name: String,
    },
    Balanced {
        open: String,
    },
    Until {
        close: String,
    },
    ScanUntil {
        stop: Vec<crate::TokenSelector>,
        #[serde(default, skip_serializing_if = "is_false")]
        allow_eof: bool,
    },
    GapWithout {
        characters: String,
    },
    EndOfContext,
    LineEnd,
}

fn is_false(value: &bool) -> bool {
    !value
}
