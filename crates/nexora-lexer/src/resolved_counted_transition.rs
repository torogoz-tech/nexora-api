use crate::{
    compiled_lexical_region::CompiledLexicalRegion, lexical_region_budget::LexicalRegionBudget,
};
use nexora_core::{LexicalRegion, NexoraError};
use nexora_language::LexicalCountedTransition;
use std::collections::BTreeMap;

pub(crate) struct ResolvedCountedTransition {
    pub definition: LexicalCountedTransition,
    pub closing: String,
    count: usize,
}
impl ResolvedCountedTransition {
    pub fn resolve(
        source: &str,
        parent: &LexicalRegion,
        transitions: &[LexicalCountedTransition],
        modes: &BTreeMap<String, CompiledLexicalRegion>,
        budget: &mut LexicalRegionBudget,
    ) -> Result<Vec<Self>, NexoraError> {
        let fail =
            || NexoraError::Configuration("missing or nonrepeated counted opening capture".into());
        let mut resolved = Vec::with_capacity(transitions.len());
        for transition in transitions {
            budget.spend()?;
            let span = parent.captures.get(&transition.capture).ok_or_else(fail)?;
            let value = source
                .get(span.start.offset..span.end.offset)
                .ok_or_else(fail)?;
            if value.is_empty() {
                return Err(fail());
            }
            if value.len() > 256 {
                return Err(NexoraError::Limit("counted opening capture length".into()));
            }
            let mut suffix = value;
            let mut count = 0;
            while !suffix.is_empty() {
                budget.spend()?;
                suffix = suffix.strip_prefix(&transition.unit).ok_or_else(fail)?;
                count += 1;
            }
            let close = &modes[&transition.region].definition.close;
            if transition
                .open
                .len()
                .checked_mul(count)
                .is_none_or(|n| n > 256)
                || close.len().checked_mul(count).is_none_or(|n| n > 256)
            {
                return Err(NexoraError::Limit(
                    "counted delimiter materialization length".into(),
                ));
            }
            resolved.push(Self {
                definition: transition.clone(),
                closing: close.repeat(count),
                count,
            });
        }
        Ok(resolved)
    }

    /// Returns (literal prefix length, opening length); a zero opening is literal text.
    pub fn lengths(
        &self,
        rest: &str,
        budget: &mut LexicalRegionBudget,
    ) -> Result<Option<(usize, usize)>, NexoraError> {
        budget.spend()?;
        let mut suffix = rest;
        let mut run = 0;
        while let Some(next) = suffix.strip_prefix(&self.definition.open) {
            budget.spend()?;
            run += 1;
            if self
                .definition
                .max_run_factor
                .is_some_and(|factor| run >= usize::from(factor) * self.count)
            {
                return Err(NexoraError::Lexical(
                    "counted opening run exceeds declared bound".into(),
                ));
            }
            suffix = next;
            if !self.definition.open_at_end && run == self.count {
                // A bound, if declared, still applies to the complete run.
                if self.definition.max_run_factor.is_none() {
                    break;
                }
            }
        }
        if run == 0 {
            if self.definition.reject_unmatched_close && rest.starts_with(&self.closing) {
                return Err(NexoraError::Lexical(
                    "unmatched counted closing delimiter".into(),
                ));
            }
            return Ok(None);
        }
        let unit = self.definition.open.len();
        if run < self.count {
            return Ok(Some((run * unit, 0)));
        }
        let prefix = if self.definition.open_at_end {
            (run - self.count) * unit
        } else {
            0
        };
        Ok(Some((prefix, self.count * unit)))
    }
}
