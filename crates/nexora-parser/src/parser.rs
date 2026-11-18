use crate::{CaptureRange, ConcreteSyntaxTree, ParseDiagnostic, PatternMatcher};
use nexora_core::SyntaxRole;
use nexora_language::GrammarDefinition;
use nexora_lexer::Token;
use std::collections::BTreeMap;
pub struct Parser {
    grammar: GrammarDefinition,
}
impl Parser {
    pub fn new(grammar: GrammarDefinition) -> Self {
        Self { grammar }
    }
    pub fn parse(&self, tokens: &[Token]) -> ConcreteSyntaxTree {
        self.parse_measured(tokens, self.grammar.max_steps).tree
    }
    /// Uses only the selected context, never the root grammar or its file scope.
    /// Token indices belong to this input stream, while source positions may be absolute.
    pub fn parse_in_context(
        &self,
        tokens: &[Token],
        context: &str,
        origin: nexora_core::SourcePosition,
    ) -> Result<ConcreteSyntaxTree, nexora_core::NexoraError> {
        Ok(self
            .parse_in_context_measured(tokens, context, origin, self.grammar.max_steps)?
            .tree)
    }
    pub fn parse_measured(&self, tokens: &[Token], max_steps: usize) -> crate::MeasuredParse {
        self.parse_entry(tokens, None, Default::default(), max_steps)
    }
    pub fn parse_in_context_measured(
        &self,
        tokens: &[Token],
        context: &str,
        origin: nexora_core::SourcePosition,
        max_steps: usize,
    ) -> Result<crate::MeasuredParse, nexora_core::NexoraError> {
        if !self.grammar.contexts.contains_key(context) {
            return Err(nexora_core::NexoraError::Configuration(format!(
                "unknown parser entry context {context:?}"
            )));
        }
        Ok(self.parse_entry(tokens, Some(context), origin, max_steps))
    }
    fn parse_entry(
        &self,
        tokens: &[Token],
        entry: Option<&str>,
        origin: nexora_core::SourcePosition,
        max_steps: usize,
    ) -> crate::MeasuredParse {
        let empty_position = tokens.last().map_or(origin, |token| token.span.end);
        let empty_span = nexora_core::SourceSpan {
            start: empty_position,
            end: empty_position,
        };
        let significant: Vec<_> = tokens
            .iter()
            .enumerate()
            .filter(|(_, t)| !t.trivia)
            .collect();
        let mut matcher =
            PatternMatcher::with_source_tokens(&self.grammar, &significant, tokens, max_steps);
        let mut tree = ConcreteSyntaxTree {
            hierarchical: self.grammar.schema_version >= 3,
            context: entry.map(str::to_owned),
            file_scope: entry
                .is_none()
                .then(|| self.grammar.file_scope.clone())
                .flatten(),
            ..Default::default()
        };
        let mut tasks = vec![crate::parse_task::ParseTask {
            start: 0,
            end: significant.len(),
            parent: None,
            context: entry.map(str::to_owned),
            region: None,
            depth: 0,
        }];
        let projector = crate::node_projector::NodeProjector::new(&self.grammar, &significant);
        let mut node_indexes = BTreeMap::new();
        let mut halted = false;
        while let Some(task) = tasks.pop() {
            if halted {
                break;
            }
            if tree.diagnostics.len() >= 1024 {
                tree.diagnostics.push(ParseDiagnostic {
                    message: "parser diagnostic budget exhausted".into(),
                    span: significant
                        .get(task.start)
                        .map_or(empty_span, |(_, token)| token.span),
                });
                break;
            }
            matcher.tokens = &significant[..task.end];
            matcher.context_start = task.start;
            if tree.hierarchical
                && let Err(error) = matcher.spend(task.start)
            {
                tree.diagnostics.push(ParseDiagnostic {
                    message: error.message,
                    span: significant
                        .get(task.start.min(significant.len().saturating_sub(1)))
                        .map_or(empty_span, |(_, t)| t.span),
                });
                break;
            }
            let context = task
                .context
                .as_ref()
                .and_then(|n| self.grammar.contexts.get(n));
            if task.context.is_some() && context.is_none() {
                tree.diagnostics.push(ParseDiagnostic {
                    message: "unknown region context".into(),
                    span: significant
                        .get(task.start.min(significant.len().saturating_sub(1)))
                        .map_or(empty_span, |(_, token)| token.span),
                });
                break;
            }
            if let Some(definition) = context.and_then(|c| c.expression.as_ref()) {
                match crate::expression_parser::ExpressionParser::parse(
                    &mut matcher,
                    definition,
                    task.start..task.end,
                    task.depth,
                    task.parent.clone(),
                ) {
                    Ok(nodes) => {
                        if tree.nodes.len() + nodes.len() > 100_000 {
                            tree.diagnostics.push(ParseDiagnostic {
                                message: "syntax node budget exhausted".into(),
                                span: significant[task.start].1.span,
                            });
                            halted = true;
                        } else {
                            for node in nodes {
                                node_indexes.insert(node.id.clone(), tree.nodes.len());
                                tree.nodes.push(node);
                            }
                            if let (Some(parent), Some(capture)) = (&task.parent, &task.region)
                                && let Some(node) = node_indexes
                                    .get(parent)
                                    .and_then(|i| tree.nodes.get_mut(*i))
                                && let Some(region) =
                                    node.regions.iter_mut().find(|r| &r.capture == capture)
                            {
                                region.status = nexora_core::RegionStatus::Parsed;
                            }
                        }
                    }
                    Err(error) => {
                        halted = error.fatal;
                        tree.diagnostics.push(ParseDiagnostic {
                            message: error.message,
                            span: significant
                                .get(error.at.min(task.end.saturating_sub(1)))
                                .map_or(empty_span, |(_, t)| t.span),
                        });
                    }
                }
                continue;
            }
            let ignored_patterns =
                context.map_or(self.grammar.ignored.as_slice(), |c| c.ignored.as_slice());
            let selected = context.map_or(self.grammar.root_productions.as_slice(), |c| {
                c.productions.as_slice()
            });
            let productions: Vec<_> = if selected.is_empty() {
                self.grammar.productions.iter().collect()
            } else {
                selected
                    .iter()
                    .filter_map(|name| self.grammar.productions.iter().find(|p| &p.name == name))
                    .collect()
            };
            let diagnostics_before = tree.diagnostics.len();
            let mut cursor = task.start;
            while cursor < task.end {
                if tree.diagnostics.len() >= 1024 {
                    tree.diagnostics.push(ParseDiagnostic {
                        message: "parser diagnostic budget exhausted".into(),
                        span: significant[cursor].1.span,
                    });
                    halted = true;
                    break;
                }
                if tree.nodes.len() >= 100_000 {
                    tree.diagnostics.push(ParseDiagnostic {
                        message: "syntax node budget exhausted".into(),
                        span: significant[cursor].1.span,
                    });
                    halted = true;
                    break;
                }
                let start = cursor;
                let mut ignored = false;
                let mut ignored_fatal = None;
                for pattern in ignored_patterns {
                    match matcher.pattern(pattern, start, &mut BTreeMap::new(), 0) {
                        Ok(end) if end > start => {
                            cursor = end;
                            ignored = true;
                            break;
                        }
                        Ok(_) => {
                            ignored_fatal = Some(crate::MatchFailure {
                                at: start,
                                message: "ignored pattern did not advance".into(),
                                fatal: true,
                            });
                            break;
                        }
                        Err(error) if error.fatal => {
                            ignored_fatal = Some(error);
                            break;
                        }
                        Err(_) => {}
                    }
                }
                if let Some(error) = ignored_fatal {
                    tree.diagnostics.push(ParseDiagnostic {
                        message: error.message,
                        span: significant
                            .get(error.at.min(task.end.saturating_sub(1)))
                            .map_or(significant[start].1.span, |(_, t)| t.span),
                    });
                    halted = true;
                    break;
                }
                if ignored {
                    continue;
                }
                let mut farthest = None;
                let mut fatal = false;
                let mut accepted = false;
                for production in &productions {
                    let mut captures: BTreeMap<String, CaptureRange> = BTreeMap::new();
                    match matcher.pattern(&production.pattern, start, &mut captures, 0) {
                        Ok(end) if end > start => {
                            match projector.project(production, start..end, &captures, &task) {
                                Ok((node, children)) => {
                                    if tasks.len() + children.len() > 100_000 {
                                        fatal = true;
                                        farthest = Some(crate::MatchFailure {
                                            at: start,
                                            message: "pending region budget exhausted".into(),
                                            fatal: true,
                                        });
                                        break;
                                    }
                                    if tree.hierarchical {
                                        node_indexes.insert(node.id.clone(), tree.nodes.len());
                                    }
                                    tree.nodes.push(node);
                                    tasks.extend(children.into_iter().rev());
                                    cursor = end;
                                    accepted = true;
                                    break;
                                }
                                Err(error) => {
                                    fatal = true;
                                    farthest = Some(error);
                                    break;
                                }
                            }
                        }
                        Ok(_) => {
                            fatal = true;
                            farthest = Some(crate::MatchFailure {
                                at: start,
                                message: "production did not advance".into(),
                                fatal: true,
                            });
                            break;
                        }
                        Err(error) => {
                            if error.fatal {
                                fatal = true;
                                farthest = Some(error);
                                break;
                            }
                            if farthest
                                .as_ref()
                                .is_none_or(|e: &crate::MatchFailure| error.at >= e.at)
                            {
                                farthest = Some(error);
                            }
                        }
                    }
                }
                if !accepted {
                    let error = farthest.unwrap_or(crate::MatchFailure {
                        at: start,
                        message: "no matching production".into(),
                        fatal: false,
                    });
                    let span = significant
                        .get(error.at.min(task.end.saturating_sub(1)))
                        .map_or(significant[start].1.span, |(_, t)| t.span);
                    tree.diagnostics.push(ParseDiagnostic {
                        message: error.message,
                        span,
                    });
                    if fatal {
                        halted = true;
                        break;
                    }
                    if context.is_some_and(|c| c.stop_on_error) {
                        break;
                    }
                    cursor = self
                        .recover(&mut matcher, start)
                        .max(start + 1)
                        .min(task.end);
                }
            }
            if let (Some(parent), Some(capture)) = (&task.parent, &task.region)
                && let Some(node) = node_indexes
                    .get(parent)
                    .and_then(|i| tree.nodes.get_mut(*i))
                && let Some(region) = node.regions.iter_mut().find(|r| &r.capture == capture)
            {
                region.status = if !halted && tree.diagnostics.len() == diagnostics_before {
                    nexora_core::RegionStatus::Parsed
                } else {
                    nexora_core::RegionStatus::Partial
                };
            }
        }
        crate::lexical_region_projector::LexicalRegionProjector::append(&mut tree, tokens);
        if tree.hierarchical {
            tree.nodes.sort_by_key(|n| {
                (
                    n.span.start.offset,
                    std::cmp::Reverse(n.span.end.offset),
                    n.id.clone(),
                )
            });
        }
        let mut recognized = vec![false; tokens.len()];
        for node in tree
            .nodes
            .iter()
            .filter(|n| n.role == SyntaxRole::Reference)
        {
            recognized[node.token_start..node.token_end].fill(true);
        }
        for (index, token) in &significant {
            if !recognized[*index]
                && self
                    .grammar
                    .opaque_references
                    .iter()
                    .any(|s| PatternMatcher::selected(token, s))
            {
                if tree.diagnostics.len() >= 1024 {
                    break;
                }
                tree.diagnostics.push(ParseDiagnostic {
                    message: "unsupported or opaque reference; coverage is incomplete".into(),
                    span: token.span,
                });
            }
        }
        let mut incomplete: Vec<usize> = tree
            .diagnostics
            .iter()
            .map(|d| d.span.start.offset)
            .chain(
                tree.nodes
                    .iter()
                    .flat_map(|n| n.regions.iter())
                    .filter(|r| r.status == nexora_core::RegionStatus::Partial)
                    .map(|r| r.span.start.offset),
            )
            .collect();
        incomplete.sort_unstable();
        for node in &mut tree.nodes {
            for region in &mut node.regions {
                let i = incomplete.partition_point(|offset| *offset < region.span.start.offset);
                if region.status == nexora_core::RegionStatus::Parsed
                    && incomplete
                        .get(i)
                        .is_some_and(|offset| *offset < region.span.end.offset)
                {
                    region.status = nexora_core::RegionStatus::Partial;
                }
            }
        }
        crate::MeasuredParse {
            tree,
            steps: matcher.steps(),
            budget_exhausted: matcher.budget_exhausted(),
        }
    }
    fn recover(&self, matcher: &mut PatternMatcher<'_>, start: usize) -> usize {
        let mut cursor = start;
        while cursor < matcher.tokens.len() {
            if matcher.spend(cursor).is_err() {
                return matcher.tokens.len();
            }
            let token = matcher.tokens[cursor].1;
            if cursor > start
                && self.grammar.recover_line_break
                && token.span.start.line > matcher.tokens[cursor - 1].1.span.end.line
            {
                return cursor;
            }
            if self
                .grammar
                .recovery
                .iter()
                .any(|s| PatternMatcher::selected(token, s))
            {
                return cursor + 1;
            }
            if let Some(group) =
                self.grammar.groups.iter().find(|g| {
                    g.open == token.text && g.kind.as_ref().is_none_or(|k| *k == token.kind)
                })
            {
                match matcher.balanced(cursor, &group.open) {
                    Ok(end) => {
                        if self.grammar.recovery_groups.contains(&group.open) {
                            return end;
                        }
                        cursor = end;
                        continue;
                    }
                    Err(_) => return matcher.tokens.len(),
                }
            }
            cursor += 1;
        }
        cursor
    }
}
