use crate::{ExpressionDefinition, GrammarDefinition, TokenSelector};
use nexora_core::{ExpressionKind, NexoraError};
use std::collections::BTreeSet;

pub(crate) struct ExpressionFormFilterValidator;
impl ExpressionFormFilterValidator {
    pub fn validate(
        e: &ExpressionDefinition,
        g: &GrammarDefinition,
        valid: &impl Fn(&TokenSelector) -> bool,
    ) -> Result<(), NexoraError> {
        let fail =
            || NexoraError::Configuration("invalid expression receiver filter contract".into());
        let total: usize = e
            .application
            .iter()
            .map(|a| a.excluded_receivers.len())
            .sum();
        if total > 64 || (total > 0 && g.schema_version < 10) {
            return Err(fail());
        }
        if total == 0 {
            return Ok(());
        }
        let operators: BTreeSet<_> = e.node_operator_texts().collect();
        if operators.len() > 256
            || operators
                .iter()
                .any(|text| text.is_empty() || text.len() > 128)
        {
            return Err(fail());
        }
        let kinds = |list: &[ExpressionKind]| {
            list.len() <= 11
                && list
                    .iter()
                    .enumerate()
                    .all(|(i, k)| !list[i + 1..].contains(k))
        };
        let operator = |text: &String| {
            !text.is_empty() && text.len() <= 128 && operators.contains(text.as_str())
        };
        for application in &e.application {
            if application.excluded_receivers.len() > 16 {
                return Err(fail());
            }
            for filter in &application.excluded_receivers {
                if filter.kinds.is_empty()
                    || !kinds(&filter.kinds)
                    || filter.operator.as_ref().is_some_and(|s| !operator(s))
                    || filter.token.as_ref().is_some_and(|t| !valid(t))
                    || filter.descendant_kinds.as_ref().is_some_and(|k| !kinds(k))
                    || filter.descendant_operators.as_ref().is_some_and(|list| {
                        list.len() > 256
                            || list.iter().any(|s| !operator(s))
                            || list.iter().collect::<BTreeSet<_>>().len() != list.len()
                    })
                {
                    return Err(fail());
                }
            }
        }
        Ok(())
    }
}
