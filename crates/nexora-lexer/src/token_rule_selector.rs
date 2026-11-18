use crate::{
    compiled_token_rule::CompiledTokenRule, lexical_region_budget::LexicalRegionBudget,
    token_context::TokenContext,
};
use nexora_core::NexoraError;
pub(crate) struct TokenRuleSelector;
impl TokenRuleSelector {
    pub fn select<'a>(
        rules: &'a [CompiledTokenRule],
        candidates: &regex::RegexSet,
        input: &str,
        offset: usize,
        previous: Option<TokenContext<'_>>,
        accepts: impl Fn(&str, &str) -> bool,
        mut budget: Option<&mut LexicalRegionBudget>,
    ) -> Result<(&'a CompiledTokenRule, usize), NexoraError> {
        let mut best: Option<(&CompiledTokenRule, usize, bool)> = None;
        for index in candidates.matches(input).iter() {
            let rule = &rules[index];
            if let Some(budget) = budget.as_mut() {
                budget.spend()?;
            }
            let context = previous.as_ref().map_or(rule.at_start, |p| {
                (rule.after.is_empty() && rule.after_kinds.is_empty())
                    || rule.after.iter().any(|s| p.matches(s))
                    || rule.after_kinds.iter().any(|kind| kind == p.kind)
            });
            if !context && !rule.reject_on_context_mismatch {
                continue;
            }
            if let Some(found) = rule.regex.find(input)
                && found.end() > 0
            {
                let allowed = context
                    && rule
                        .follow_exclude
                        .as_ref()
                        .is_none_or(|pattern| !pattern.is_match(&input[found.end()..]))
                    && rule
                        .region
                        .as_ref()
                        .is_none_or(|mode| accepts(mode, &input[found.end()..]));
                if (allowed || rule.reject_on_context_mismatch)
                    && best.is_none_or(|(_, length, _)| found.end() > length)
                {
                    best = Some((rule, found.end(), allowed));
                }
            }
        }
        let (rule, length, allowed) = best.ok_or_else(|| {
            NexoraError::Lexical(format!("unknown or unterminated token at byte {offset}"))
        })?;
        if !allowed {
            return Err(NexoraError::Lexical(format!(
                "ambiguous token {} in lexical context at byte {offset}",
                rule.kind
            )));
        }
        if rule.reject {
            return Err(NexoraError::Lexical(format!(
                "rejected {} at byte {offset}",
                rule.kind
            )));
        }
        Ok((rule, length))
    }
}
