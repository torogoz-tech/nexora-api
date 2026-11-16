use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    #[serde(alias = "syntax_unit")]
    SyntaxUnit,
    #[serde(alias = "lexical_region")]
    LexicalRegion,
    #[serde(alias = "type_alias")]
    TypeAlias,
    #[serde(alias = "enum")]
    Enum,
    #[serde(alias = "expression")]
    Expression,
    #[serde(alias = "statement")]
    Statement,
    #[serde(alias = "block")]
    Block,
    #[serde(alias = "symbol")]
    Symbol,
    #[serde(alias = "repository")]
    Repository,
    #[serde(alias = "file")]
    File,
    #[serde(alias = "module")]
    Module,
    #[serde(alias = "namespace")]
    Namespace,
    #[serde(alias = "class")]
    Class,
    #[serde(alias = "interface")]
    Interface,
    #[serde(alias = "function")]
    Function,
    #[serde(alias = "method")]
    Method,
    #[serde(alias = "variable")]
    Variable,
    #[serde(alias = "parameter")]
    Parameter,
}
impl NodeKind {
    pub fn parse(kind: &str) -> Option<Self> {
        match kind {
            "SyntaxUnit" => Some(Self::SyntaxUnit),
            "LexicalRegion" => Some(Self::LexicalRegion),
            "TypeAlias" => Some(Self::TypeAlias),
            "Enum" => Some(Self::Enum),
            "Expression" => Some(Self::Expression),
            "Statement" => Some(Self::Statement),
            "Block" => Some(Self::Block),
            "Symbol" => Some(Self::Symbol),
            "Repository" => Some(Self::Repository),
            "File" => Some(Self::File),
            "Module" => Some(Self::Module),
            "Namespace" => Some(Self::Namespace),
            "Class" => Some(Self::Class),
            "Interface" => Some(Self::Interface),
            "Function" => Some(Self::Function),
            "Method" => Some(Self::Method),
            "Variable" => Some(Self::Variable),
            "Parameter" => Some(Self::Parameter),
            _ => None,
        }
    }
}
