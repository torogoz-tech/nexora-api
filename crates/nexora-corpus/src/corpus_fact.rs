use nexora_graph::{EdgeKind, NodeKind, ResolutionState};
use serde::{Deserialize, Serialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum CorpusFact {
    Declaration {
        file: String,
        kind: NodeKind,
        name: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        owner: Vec<String>,
        source: String,
    },
    Container {
        file: String,
        kind: NodeKind,
        name: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        owner: Vec<String>,
        source: String,
    },
    Expression {
        file: String,
        kind: nexora_core::ExpressionKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        operator: Option<String>,
        operands: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        owner: Vec<String>,
        source: String,
    },
    LexicalRegion {
        file: String,
        mode: String,
        closed: bool,
        opener: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        header: Option<String>,
        #[serde(default, skip_serializing_if = "is_false")]
        self_closed: bool,
        #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
        captures: std::collections::BTreeMap<String, String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        closer: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lookahead: Option<String>,
        content: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        owner: Vec<String>,
        source: String,
    },
    Relation {
        file: String,
        kind: EdgeKind,
        target: String,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        owner: Vec<String>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        target_owner: Vec<String>,
        resolution: ResolutionState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resolution_reason: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
        source: String,
    },
}
impl CorpusFact {
    pub fn file(&self) -> &str {
        match self {
            Self::Declaration { file, .. }
            | Self::Container { file, .. }
            | Self::Expression { file, .. }
            | Self::LexicalRegion { file, .. }
            | Self::Relation { file, .. } => file,
        }
    }
    pub fn source(&self) -> &str {
        match self {
            Self::Declaration { source, .. }
            | Self::Container { source, .. }
            | Self::Expression { source, .. }
            | Self::LexicalRegion { source, .. }
            | Self::Relation { source, .. } => source,
        }
    }
    pub fn prefixed(&self, prefix: &str) -> Self {
        let path = |p: &str| format!("{prefix}/{p}");
        match self {
            Self::Declaration {
                file,
                kind,
                name,
                owner,
                source,
            } => Self::Declaration {
                file: path(file),
                kind: kind.clone(),
                name: name.clone(),
                owner: owner.clone(),
                source: source.clone(),
            },
            Self::Container {
                file,
                kind,
                name,
                owner,
                source,
            } => Self::Container {
                file: path(file),
                kind: kind.clone(),
                name: name.clone(),
                owner: owner.clone(),
                source: source.clone(),
            },
            Self::Expression {
                file,
                kind,
                operator,
                operands,
                owner,
                source,
            } => Self::Expression {
                file: path(file),
                kind: *kind,
                operator: operator.clone(),
                operands: operands.clone(),
                owner: owner.clone(),
                source: source.clone(),
            },
            Self::LexicalRegion {
                file,
                mode,
                closed,
                opener,
                header,
                self_closed,
                captures,
                closer,
                lookahead,
                content,
                owner,
                source,
            } => Self::LexicalRegion {
                file: path(file),
                mode: mode.clone(),
                closed: *closed,
                opener: opener.clone(),
                header: header.clone(),
                self_closed: *self_closed,
                captures: captures.clone(),
                closer: closer.clone(),
                lookahead: lookahead.clone(),
                content: content.clone(),
                owner: owner.clone(),
                source: source.clone(),
            },
            Self::Relation {
                file,
                kind,
                target,
                owner,
                target_owner,
                resolution,
                resolution_reason,
                label,
                source,
            } => Self::Relation {
                file: path(file),
                kind: kind.clone(),
                target: if *resolution == ResolutionState::Resolved
                    && matches!(kind, EdgeKind::Imports | EdgeKind::DependsOn)
                {
                    path(target)
                } else {
                    target.clone()
                },
                owner: owner.clone(),
                target_owner: target_owner.clone(),
                resolution: resolution.clone(),
                resolution_reason: resolution_reason.clone(),
                label: label.clone(),
                source: source.clone(),
            },
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}
