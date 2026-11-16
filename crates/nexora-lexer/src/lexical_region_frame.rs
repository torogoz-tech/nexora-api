pub(crate) struct LexicalRegionFrame {
    pub previous: Option<crate::context_token::ContextToken>,
    pub returned_kind: Option<String>,
    pub returned_trivia: bool,
    pub mode: String,
    pub index: usize,
    pub header: bool,
    pub close_value: Option<String>,
    pub terminal_value: Option<String>,
    pub counted_children: Vec<crate::resolved_counted_transition::ResolvedCountedTransition>,
}
