pub(crate) struct CompiledLexicalTransition {
    pub regex: regex::Regex,
    pub region: String,
    pub reject: bool,
}
