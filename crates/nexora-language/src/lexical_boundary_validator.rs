use crate::LexerDefinition;
use nexora_core::NexoraError;
use std::collections::BTreeSet;
pub struct LexicalBoundaryValidator;
impl LexicalBoundaryValidator {
    pub fn validate(definition: &LexerDefinition) -> Result<(), NexoraError> {
        let fail =
            || NexoraError::Configuration("invalid bounded non-consuming lexical boundary".into());
        if definition
            .regions
            .values()
            .map(|r| r.close_before.len())
            .sum::<usize>()
            > 128
        {
            return Err(fail());
        }
        let kinds: BTreeSet<_> = definition
            .tokens
            .iter()
            .filter(|t| !t.trivia)
            .map(|t| t.kind.as_str())
            .chain(std::iter::once("keyword"))
            .collect();
        for region in definition.regions.values() {
            if region.close_before.len() > 16
                || (definition.schema_version < 9 && !region.close_before.is_empty())
            {
                return Err(fail());
            }
            for boundary in &region.close_before {
                if boundary.pattern.is_empty()
                    || boundary.pattern.len() > 4096
                    || boundary.after_kinds.len() > 32
                    || boundary.after_kinds.iter().any(|kind| {
                        kind.is_empty() || kind.len() > 128 || !kinds.contains(kind.as_str())
                    })
                    || boundary.after_kinds.iter().collect::<BTreeSet<_>>().len()
                        != boundary.after_kinds.len()
                {
                    return Err(fail());
                }
                let hir = regex_syntax::parse(&boundary.pattern).map_err(|_| fail())?;
                if !hir.properties().minimum_len().is_some_and(|n| n > 0)
                    || !hir.properties().maximum_len().is_some_and(|n| n <= 128)
                {
                    return Err(fail());
                }
                regex::RegexBuilder::new(&format!("^(?:{})", boundary.pattern))
                    .size_limit(1_000_000)
                    .build()
                    .map_err(|_| fail())?;
            }
        }
        Ok(())
    }
}
