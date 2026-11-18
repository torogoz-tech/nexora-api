pub(crate) struct CompiledLexicalRegion {
    pub token_rules: Vec<crate::compiled_token_rule::CompiledTokenRule>,
    pub token_rule_set: regex::RegexSet,
    pub definition: nexora_language::LexicalRegionDefinition,
    pub opening_follow: Option<regex::Regex>,
    pub close_tail: Option<regex::Regex>,
    pub close_before: Vec<crate::compiled_lexical_boundary::CompiledLexicalBoundary>,
    pub header_children: Vec<crate::compiled_lexical_transition::CompiledLexicalTransition>,
    pub children: Vec<crate::compiled_lexical_transition::CompiledLexicalTransition>,
}
