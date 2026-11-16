use serde::Deserialize;

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CodeQueryOperation {
    Overview,
    FindSymbol,
    References,
    Dependencies,
    Traverse,
    Search,
    Context,
}

impl CodeQueryOperation {
    pub fn tool_name(self) -> &'static str {
        match self {
            Self::Overview => "nexora_repository_overview",
            Self::FindSymbol => "nexora_find_symbol",
            Self::References => "nexora_find_references",
            Self::Dependencies => "nexora_get_dependencies",
            Self::Traverse => "nexora_traverse_graph",
            Self::Search => "nexora_search_code",
            Self::Context => "nexora_get_source_context",
        }
    }
}
