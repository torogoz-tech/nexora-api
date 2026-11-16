use crate::{CaptureRange, ConcreteSyntaxNode, MatchFailure, parse_task::ParseTask};
use nexora_core::{NodeId, RegionStatus, SourceSpan, SyntaxCapture, SyntaxRegion};
use nexora_language::{GrammarDefinition, ProductionRule};
use nexora_lexer::Token;
use std::collections::BTreeMap;
pub(crate) struct NodeProjector<'a> {
    grammar: &'a GrammarDefinition,
    significant: &'a [(usize, &'a Token)],
}
impl<'a> NodeProjector<'a> {
    pub fn new(grammar: &'a GrammarDefinition, significant: &'a [(usize, &'a Token)]) -> Self {
        Self {
            grammar,
            significant,
        }
    }
    pub fn project(
        &self,
        production: &ProductionRule,
        range: std::ops::Range<usize>,
        captures: &BTreeMap<String, CaptureRange>,
        task: &ParseTask,
    ) -> Result<(ConcreteSyntaxNode, Vec<ParseTask>), MatchFailure> {
        let start = range.start;
        let end = range.end;
        let fail = |message: &str| MatchFailure {
            at: start,
            message: message.into(),
            fatal: true,
        };
        let capture = captures
            .get(&production.projection.name_capture)
            .ok_or_else(|| fail("missing projected capture"))?;
        let name = self.significant[capture.start..capture.end]
            .iter()
            .map(|(_, t)| t.text.as_str())
            .collect::<Vec<_>>()
            .join(&capture.join);
        let hierarchical = self.grammar.schema_version >= 3;
        let id = NodeId(if hierarchical {
            format!(
                "cst:{}:{}:{}",
                self.significant[start].1.span.start.offset,
                self.significant[end - 1].1.span.end.offset,
                production.name
            )
        } else {
            format!("cst:{}", self.significant[start].1.span.start.offset)
        });
        let mut node = ConcreteSyntaxNode {
            id: id.clone(),
            production: production.name.clone(),
            ast_kind: production.projection.ast_kind.clone(),
            name,
            expression: None,
            lexical_region: None,
            role: production.projection.role,
            span: SourceSpan {
                start: self.significant[start].1.span.start,
                end: self.significant[end - 1].1.span.end,
            },
            token_start: self.significant[start].0,
            token_end: self.significant[end - 1].0 + 1,
            parent_id: task.parent.clone(),
            scope: production.projection.scope.clone(),
            captures: BTreeMap::new(),
            regions: Vec::new(),
        };
        if hierarchical {
            for (name, capture) in captures {
                node.captures.insert(
                    name.clone(),
                    SyntaxCapture {
                        span: SourceSpan {
                            start: self.significant[capture.start].1.span.start,
                            end: self.significant[capture.end - 1].1.span.end,
                        },
                        token_start: self.significant[capture.start].0,
                        token_end: self.significant[capture.end - 1].0 + 1,
                    },
                );
            }
        }
        let mut tasks = Vec::new();
        let mut intervals = Vec::new();
        for definition in &production.projection.regions {
            let c = captures
                .get(&definition.capture)
                .ok_or_else(|| fail("missing region capture"))?;
            let left = c
                .start
                .checked_add(definition.trim_start)
                .ok_or_else(|| fail("region trim overflow"))?;
            let right = c
                .end
                .checked_sub(definition.trim_end)
                .ok_or_else(|| fail("region trim underflow"))?;
            if left > right
                || left < c.start
                || right > c.end
                || left < start
                || right > end
                || (definition.context.is_some() && left == start && right == end)
            {
                return Err(fail(
                    "region must be bounded and strictly smaller than its owner",
                ));
            }
            if task.depth >= self.grammar.max_nesting {
                return Err(fail("region nesting budget exceeded"));
            }
            let begin = if definition.trim_start == 0 {
                self.significant[c.start].1.span.start
            } else {
                self.significant[left - 1].1.span.end
            };
            let finish = if definition.trim_end == 0 {
                self.significant[c.end - 1].1.span.end
            } else {
                self.significant[right].1.span.start
            };
            if finish.offset < begin.offset {
                return Err(fail("inverted region span"));
            }
            let raw_start = if definition.trim_start == 0 {
                self.significant[c.start].0
            } else {
                self.significant[left - 1].0 + 1
            };
            let raw_end = if definition.trim_end == 0 {
                self.significant[c.end - 1].0 + 1
            } else {
                self.significant[right].0
            };
            node.regions.push(SyntaxRegion {
                coverage: None,
                capture: definition.capture.clone(),
                span: SourceSpan {
                    start: begin,
                    end: finish,
                },
                token_start: raw_start,
                token_end: raw_end,
                context: definition.context.clone(),
                status: if definition.context.is_some() {
                    RegionStatus::Partial
                } else {
                    RegionStatus::Opaque
                },
            });
            intervals.push((left, right));
            if definition.context.is_some() {
                tasks.push(ParseTask {
                    start: left,
                    end: right,
                    parent: Some(id.clone()),
                    context: definition.context.clone(),
                    region: Some(definition.capture.clone()),
                    depth: task.depth + 1,
                });
            }
        }
        intervals.sort();
        if intervals.windows(2).any(|p| p[0].1 > p[1].0) {
            return Err(fail("structured regions overlap"));
        }
        tasks.sort_by_key(|t| t.start);
        Ok((node, tasks))
    }
}
