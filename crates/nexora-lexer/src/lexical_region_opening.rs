use crate::lexical_region_budget::LexicalRegionBudget;
use nexora_core::{LexicalRegion, NexoraError, NodeId, SourcePosition, SourceSpan};
pub(crate) struct LexicalRegionOpening {
    pub region: LexicalRegion,
    pub end: SourcePosition,
    pub close_value: Option<String>,
}
impl LexicalRegionOpening {
    pub fn literal(
        start: SourcePosition,
        text: &str,
        mode: &str,
        spec: &nexora_language::LexicalRegionDefinition,
        parent: NodeId,
        close_value: String,
    ) -> Self {
        let mut end = start;
        end.advance(text);
        Self {
            region: LexicalRegion {
                id: NodeId(format!("lex:{}:{mode}", start.offset)),
                mode: mode.into(),
                ast_kind: spec.ast_kind.clone(),
                parent: Some(parent),
                span: SourceSpan { start, end },
                content_span: SourceSpan { start: end, end },
                opener: SourceSpan { start, end },
                closer: None,
                lookahead_span: None,
                closed: false,
                header: None,
                header_content_span: spec.header.as_ref().map(|_| SourceSpan { start: end, end }),
                header_closed: false,
                self_closed: false,
                captures: Default::default(),
            },
            end,
            close_value: Some(close_value),
        }
    }
    pub fn new(
        source: &str,
        start: SourcePosition,
        pattern: &regex::Regex,
        mode: &str,
        spec: &nexora_language::LexicalRegionDefinition,
        parent: Option<NodeId>,
        budget: &mut LexicalRegionBudget,
    ) -> Result<Self, NexoraError> {
        let rest = &source[start.offset..];
        let captures = pattern
            .captures(rest)
            .ok_or_else(|| NexoraError::Configuration("missing region opening match".into()))?;
        let whole = captures.get(0).expect("whole opening capture");
        if whole.end() == 0 {
            return Err(NexoraError::Configuration(
                "region opening must advance".into(),
            ));
        }
        let close_value = if let Some(name) = &spec.close_capture {
            let value = captures
                .name(name)
                .ok_or_else(|| {
                    NexoraError::Configuration("missing required opening capture".into())
                })?
                .as_str();
            if value.is_empty() || value.len() > 256 {
                return Err(NexoraError::Limit("dynamic closing capture length".into()));
            }
            Some(value.to_owned())
        } else {
            None
        };
        let mut spans = std::collections::BTreeMap::new();
        for name in pattern.capture_names().flatten() {
            budget.spend()?;
            if let Some(value) = captures.name(name) {
                let mut left = start;
                left.advance(&rest[..value.start()]);
                let mut right = left;
                right.advance(value.as_str());
                spans.insert(
                    name.into(),
                    SourceSpan {
                        start: left,
                        end: right,
                    },
                );
            }
        }
        let mut end = start;
        end.advance(&rest[..whole.end()]);
        Ok(Self {
            region: LexicalRegion {
                id: NodeId(format!("lex:{}:{mode}", start.offset)),
                mode: mode.into(),
                ast_kind: spec.ast_kind.clone(),
                parent,
                span: SourceSpan { start, end },
                content_span: SourceSpan { start: end, end },
                opener: SourceSpan { start, end },
                closer: None,
                lookahead_span: None,
                closed: false,
                header: None,
                header_content_span: spec.header.as_ref().map(|_| SourceSpan { start: end, end }),
                header_closed: false,
                self_closed: false,
                captures: spans,
            },
            end,
            close_value,
        })
    }
}
