use regex::Regex;
#[derive(Clone)]
pub(crate) struct CompiledTokenRule {
    pub kind: String,
    pub root: bool,
    pub regex: Regex,
    pub region: Option<String>,
    pub follow_exclude: Option<Regex>,
    pub after: Vec<nexora_language::TokenSelector>,
    pub after_kinds: Vec<String>,
    pub at_start: bool,
    pub reject_on_context_mismatch: bool,
    pub trivia: bool,
    pub reject: bool,
}
