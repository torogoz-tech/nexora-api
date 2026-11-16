use crate::{
    MatchFailure, PatternMatcher,
    compiled_expression_receiver_policy::CompiledExpressionReceiverPolicy as Policy,
    expression_shape_summary::ExpressionShapeSummary, expression_term::ExpressionTerm,
};
use std::sync::Arc;

pub(crate) struct ExpressionReceiverMatcher {
    policy: Arc<Policy>,
    summaries: Vec<ExpressionShapeSummary>,
}
impl ExpressionReceiverMatcher {
    pub fn new(policy: Arc<Policy>) -> Self {
        Self {
            policy,
            summaries: Vec::new(),
        }
    }
    pub fn add(
        &mut self,
        operator: Option<&str>,
        operands: &[usize],
        terms: &[ExpressionTerm],
        matcher: &mut PatternMatcher<'_>,
        at: usize,
    ) -> Result<(), MatchFailure> {
        matcher.spend(at)?;
        let mut summary = ExpressionShapeSummary::default();
        if let Some(operator) = operator {
            summary.operator = Some(
                *self
                    .policy
                    .operators
                    .get(operator)
                    .ok_or_else(|| Policy::failure(at))?,
            );
        }
        for operand in operands {
            matcher.spend(at)?;
            let child = terms.get(*operand).ok_or_else(|| Policy::failure(at))?;
            let data = self
                .summaries
                .get(*operand)
                .ok_or_else(|| Policy::failure(at))?;
            summary.descendant_kinds |= Policy::kind_bit(child.kind, at)? | data.descendant_kinds;
            for (out, part) in summary
                .descendant_operators
                .iter_mut()
                .zip(data.descendant_operators)
            {
                *out |= part;
            }
            if let Some(id) = data.operator {
                let id = usize::from(id);
                summary.descendant_operators[id / 64] |= 1u64 << (id % 64);
            }
        }
        if self.summaries.len() != terms.len() {
            return Err(Policy::failure(at));
        }
        self.summaries.push(summary);
        Ok(())
    }
    pub fn excludes(
        &self,
        application: usize,
        receiver: usize,
        terms: &[ExpressionTerm],
        matcher: &mut PatternMatcher<'_>,
    ) -> Result<bool, MatchFailure> {
        let term = terms.get(receiver).ok_or_else(|| Policy::failure(0))?;
        let summary = self
            .summaries
            .get(receiver)
            .ok_or_else(|| Policy::failure(term.start))?;
        let filters = self
            .policy
            .filters
            .get(application)
            .ok_or_else(|| Policy::failure(term.start))?;
        for filter in filters {
            matcher.spend(term.start)?;
            if filter.kinds & Policy::kind_bit(term.kind, term.start)? == 0
                || filter
                    .operator
                    .is_some_and(|operator| summary.operator != Some(operator))
                || filter.token.as_ref().is_some_and(|selector| {
                    matcher
                        .tokens
                        .get(term.start)
                        .is_none_or(|(_, token)| !PatternMatcher::selected(token, selector))
                })
                || filter
                    .descendant_kinds
                    .is_some_and(|allowed| summary.descendant_kinds & !allowed != 0)
                || filter.descendant_operators.is_some_and(|allowed| {
                    summary
                        .descendant_operators
                        .iter()
                        .zip(allowed)
                        .any(|(present, allowed)| present & !allowed != 0)
                })
            {
                continue;
            }
            return Ok(true);
        }
        Ok(false)
    }
}
