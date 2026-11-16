pub(crate) struct CompiledLexicalBoundary {
    pub regex: regex::Regex,
    pub after_kinds: Vec<String>,
}
