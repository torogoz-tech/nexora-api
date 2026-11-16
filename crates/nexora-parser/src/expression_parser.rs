use crate::{ConcreteSyntaxNode, MatchFailure, PatternMatcher, expression_term::ExpressionTerm};
use nexora_core::{ExpressionKind as K, ExpressionShape, NodeId, SourceSpan, SyntaxRole};
use nexora_language::{
    ApplicationOperator, ConditionalOperator, DelimiterPair, ExpressionDefinition, InfixOperator,
    OperatorAssociativity as A, TokenSelector, UnaryOperator,
};
pub(crate) struct ExpressionParser<'m, 't> {
    matcher: &'m mut PatternMatcher<'t>,
    definition: &'t ExpressionDefinition,
    cursor: usize,
    end: usize,
    base_depth: usize,
    terms: Vec<ExpressionTerm>,
    conditional_stop: Option<TokenSelector>,
    receiver_policy: Option<crate::expression_receiver_matcher::ExpressionReceiverMatcher>,
}
impl<'m, 't> ExpressionParser<'m, 't> {
    pub fn parse(
        matcher: &'m mut PatternMatcher<'t>,
        definition: &'t ExpressionDefinition,
        range: std::ops::Range<usize>,
        depth: usize,
        parent: Option<NodeId>,
    ) -> Result<Vec<ConcreteSyntaxNode>, MatchFailure> {
        let receiver_policy = matcher
            .receiver_policy(definition, range.start)?
            .map(crate::expression_receiver_matcher::ExpressionReceiverMatcher::new);
        let mut parser = Self {
            matcher,
            definition,
            cursor: range.start,
            end: range.end,
            base_depth: depth,
            terms: Vec::new(),
            conditional_stop: None,
            receiver_policy,
        };
        let root = parser.expression(0, 0)?;
        if parser.cursor != parser.end {
            return Err(parser.failure("unexpected token after expression", false));
        }
        parser.nodes(root, parent)
    }
    fn failure(&self, message: &str, fatal: bool) -> MatchFailure {
        MatchFailure {
            at: self.cursor,
            message: message.into(),
            fatal,
        }
    }
    fn take(&mut self) -> Result<usize, MatchFailure> {
        if self.cursor >= self.end {
            return Err(self.failure("expected expression operand", false));
        }
        self.matcher.spend(self.cursor)?;
        let index = self.cursor;
        self.cursor += 1;
        Ok(index)
    }
    fn selected(&self, s: &TokenSelector) -> bool {
        self.matcher
            .tokens
            .get(self.cursor)
            .is_some_and(|(_, t)| PatternMatcher::selected(t, s))
    }
    fn at(&self, text: &str, kind: Option<&str>) -> bool {
        self.matcher
            .tokens
            .get(self.cursor)
            .is_some_and(|(_, t)| t.text == text && kind.is_none_or(|k| k == t.kind))
    }
    fn unary(
        &mut self,
        operators: &[UnaryOperator],
    ) -> Result<Option<UnaryOperator>, MatchFailure> {
        for op in operators {
            self.matcher.spend(self.cursor)?;
            if self.selected(&op.token) {
                return Ok(Some(op.clone()));
            }
        }
        Ok(None)
    }
    fn infix(&mut self) -> Result<Option<InfixOperator>, MatchFailure> {
        for op in &self.definition.infix {
            self.matcher.spend(self.cursor)?;
            if self.selected(&op.token) {
                return Ok(Some(op.clone()));
            }
        }
        Ok(None)
    }
    fn grouping(&mut self) -> Result<Option<DelimiterPair>, MatchFailure> {
        for d in &self.definition.grouping {
            self.matcher.spend(self.cursor)?;
            if self.at(&d.open, d.kind.as_deref()) {
                return Ok(Some(d.clone()));
            }
        }
        Ok(None)
    }
    fn application(&mut self) -> Result<Option<(usize, ApplicationOperator)>, MatchFailure> {
        for (index, d) in self.definition.application.iter().enumerate() {
            self.matcher.spend(self.cursor)?;
            if self.at(&d.open, d.kind.as_deref()) {
                return Ok(Some((index, d.clone())));
            }
        }
        Ok(None)
    }
    fn conditional(&mut self) -> Result<Option<ConditionalOperator>, MatchFailure> {
        for operator in &self.definition.conditional {
            self.matcher.spend(self.cursor)?;
            if self.selected(&operator.token) {
                return Ok(Some(operator.clone()));
            }
        }
        Ok(None)
    }
    fn conditional_boundary(&mut self) -> Result<bool, MatchFailure> {
        if let Some(separator) = &self.conditional_stop {
            self.matcher.spend(self.cursor)?;
            return Ok(self.selected(separator));
        }
        Ok(false)
    }
    fn delimited_expression(&mut self, depth: usize) -> Result<usize, MatchFailure> {
        let stop = self.conditional_stop.take();
        let result = self.expression(0, depth);
        self.conditional_stop = stop;
        result
    }
    fn terminator(&self) -> bool {
        self.definition
            .grouping
            .iter()
            .any(|g| self.at(&g.close, g.kind.as_deref()))
            || self
                .definition
                .application
                .iter()
                .any(|a| self.at(&a.close, a.kind.as_deref()) || self.selected(&a.separator))
    }
    fn add(
        &mut self,
        kind: K,
        operator: Option<String>,
        start: usize,
        operands: Vec<usize>,
    ) -> Result<usize, MatchFailure> {
        self.matcher.spend(start)?;
        if self.terms.len() >= 100_000 {
            return Err(self.failure("expression node budget exhausted", true));
        }
        let height = operands
            .iter()
            .map(|i| self.terms[*i].height)
            .max()
            .unwrap_or(0)
            + 1;
        if height + self.base_depth > self.matcher.grammar.max_nesting {
            return Err(self.failure("expression tree height budget exhausted", true));
        }
        let id = self.terms.len();
        if let Some(policy) = &mut self.receiver_policy {
            policy.add(
                operator.as_deref(),
                &operands,
                &self.terms,
                self.matcher,
                start,
            )?;
        }
        self.terms.push(ExpressionTerm {
            kind,
            operator,
            start,
            end: self.cursor,
            operands,
            height,
        });
        Ok(id)
    }
    fn expression(&mut self, minimum: u16, depth: usize) -> Result<usize, MatchFailure> {
        if depth + self.base_depth >= self.matcher.grammar.max_nesting {
            return Err(self.failure("expression nesting budget exhausted", true));
        }
        self.matcher.spend(self.cursor)?;
        let mut left = self.atom(depth)?;
        let definition = self.definition;
        while self.cursor < self.end {
            if self.conditional_boundary()? {
                break;
            }
            if let Some(op) = self.unary(&definition.member)? {
                if op.precedence < minimum {
                    break;
                }
                let start = self.terms[left].start;
                self.take()?;
                let index = self.cursor;
                let Some((_, token)) = self.matcher.tokens.get(index) else {
                    return Err(self.failure("expected member name", false));
                };
                if !definition.identifiers.contains(&token.kind) {
                    return Err(self.failure("expected member name", false));
                }
                self.take()?;
                let member = self.add(K::Identifier, None, index, Vec::new())?;
                left = self.add(K::Member, Some(op.token.text), start, vec![left, member])?;
                continue;
            }
            if let Some((application, op)) = self.application()? {
                if op.precedence < minimum {
                    break;
                }
                if let Some(policy) = &self.receiver_policy
                    && policy.excludes(application, left, &self.terms, self.matcher)?
                {
                    return Err(self.failure(
                        "delimited receiver form requires another declared interpretation",
                        false,
                    ));
                }
                let start = self.terms[left].start;
                self.take()?;
                let mut operands = vec![left];
                let result_kind = op.expression_kind.unwrap_or(K::Application);
                let maximum = usize::from(op.max_arguments.unwrap_or(1024));
                if !self.at(&op.close, op.kind.as_deref()) {
                    loop {
                        if operands.len() > 1024 {
                            return Err(self.failure("application operand budget exhausted", true));
                        }
                        if operands.len() > maximum {
                            return Err(
                                self.failure("delimited operand count is not permitted", false)
                            );
                        }
                        operands.push(self.delimited_expression(depth + 1)?);
                        if self.selected(&op.separator) {
                            self.take()?;
                            continue;
                        }
                        break;
                    }
                }
                if result_kind == K::Index && operands.len() == 1 {
                    return Err(self.failure("expected index operand", false));
                }
                if !self.at(&op.close, op.kind.as_deref()) {
                    return Err(self.failure("expected application closer", false));
                }
                self.take()?;
                left = self.add(result_kind, Some(op.open), start, operands)?;
                continue;
            }
            if let Some(op) = self.unary(&definition.postfix)? {
                if op.precedence < minimum {
                    break;
                }
                let start = self.terms[left].start;
                self.take()?;
                left = self.add(K::Postfix, Some(op.token.text), start, vec![left])?;
                continue;
            }
            if let Some(op) = self.conditional()? {
                if op.precedence < minimum {
                    break;
                }
                let start = self.terms[left].start;
                self.take()?;
                let stop = self.conditional_stop.replace(op.separator.clone());
                let consequent = self.expression(op.consequent_precedence, depth + 1);
                self.conditional_stop = stop;
                let consequent = consequent?;
                if !self.selected(&op.separator) {
                    return Err(self.failure("expected conditional separator", false));
                }
                self.take()?;
                let alternative = self.expression(op.alternative_precedence, depth + 1)?;
                left = self.add(
                    K::Conditional,
                    Some(op.token.text),
                    start,
                    vec![left, consequent, alternative],
                )?;
                continue;
            }
            let Some(op) = self.infix()? else {
                break;
            };
            if op.precedence < minimum {
                break;
            }
            if op.kind == K::Assignment
                && !definition
                    .assignment_targets
                    .contains(&self.terms[left].kind)
            {
                return Err(self.failure("assignment target form is not permitted", false));
            }
            let start = self.terms[left].start;
            self.take()?;
            let right = self.expression(
                if op.associativity == A::Right {
                    op.precedence
                } else {
                    op.precedence + 1
                },
                depth + 1,
            )?;
            left = self.add(op.kind, Some(op.token.text), start, vec![left, right])?;
            if op.associativity == A::NonAssociative
                && self
                    .infix()?
                    .is_some_and(|next| next.precedence == op.precedence)
            {
                return Err(self.failure("non-associative operators require grouping", false));
            }
        }
        Ok(left)
    }
    fn atom(&mut self, depth: usize) -> Result<usize, MatchFailure> {
        let definition = self.definition;
        let start = self.cursor;
        if self.conditional_boundary()? {
            return Err(self.failure("expected conditional branch operand", false));
        }
        if let Some(op) = self.unary(&definition.prefix)? {
            self.take()?;
            let operand = self.expression(op.precedence, depth + 1)?;
            return self.add(K::Prefix, Some(op.token.text), start, vec![operand]);
        }
        if let Some(group) = self.grouping()? {
            self.take()?;
            let operand = self.delimited_expression(depth + 1)?;
            if !self.at(&group.close, group.kind.as_deref()) {
                return Err(self.failure("expected expression group closer", false));
            }
            self.take()?;
            return self.add(K::Group, Some(group.open), start, vec![operand]);
        }
        if self.terminator() {
            return Err(self.failure("expected operand before expression boundary", false));
        }
        let Some((_, token)) = self.matcher.tokens.get(start).copied() else {
            return Err(self.failure("expected expression operand", false));
        };
        let mut literal = false;
        for selector in &definition.literal_selectors {
            self.matcher.spend(start)?;
            if PatternMatcher::selected(token, selector) {
                literal = true;
                break;
            }
        }
        let kind = if literal {
            K::Literal
        } else if definition.identifiers.contains(&token.kind) {
            K::Identifier
        } else if definition.literals.contains(&token.kind) {
            K::Literal
        } else {
            return Err(self.failure("unsupported expression operand", false));
        };
        self.take()?;
        self.add(kind, None, start, Vec::new())
    }
    fn nodes(
        &self,
        root: usize,
        parent: Option<NodeId>,
    ) -> Result<Vec<ConcreteSyntaxNode>, MatchFailure> {
        let ids: Vec<_> = self
            .terms
            .iter()
            .enumerate()
            .map(|(i, t)| {
                NodeId(format!(
                    "cst:{}:{}:expr:{i}",
                    self.matcher.tokens[t.start].1.span.start.offset,
                    self.matcher.tokens[t.end - 1].1.span.end.offset
                ))
            })
            .collect();
        let mut parents = vec![None; ids.len()];
        for (i, t) in self.terms.iter().enumerate() {
            for operand in &t.operands {
                parents[*operand] = Some(ids[i].clone());
            }
        }
        parents[root] = parent;
        Ok(self
            .terms
            .iter()
            .enumerate()
            .map(|(i, t)| ConcreteSyntaxNode {
                id: ids[i].clone(),
                production: "expression".into(),
                ast_kind: self.definition.ast_kind.clone(),
                name: t
                    .operator
                    .clone()
                    .unwrap_or_else(|| self.matcher.tokens[t.start].1.text.clone()),
                lexical_region: None,
                expression: Some(ExpressionShape {
                    kind: t.kind,
                    operator: t.operator.clone(),
                    operands: t.operands.iter().map(|i| ids[*i].clone()).collect(),
                }),
                parent_id: parents[i].clone(),
                scope: None,
                captures: Default::default(),
                regions: Vec::new(),
                role: SyntaxRole::Expression,
                span: SourceSpan {
                    start: self.matcher.tokens[t.start].1.span.start,
                    end: self.matcher.tokens[t.end - 1].1.span.end,
                },
                token_start: self.matcher.tokens[t.start].0,
                token_end: self.matcher.tokens[t.end - 1].0 + 1,
            })
            .collect())
    }
}
