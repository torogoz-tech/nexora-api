use crate::LexerDefinition;
use nexora_core::NexoraError;
use std::collections::BTreeSet;
pub struct LexicalEntryValidator;
impl LexicalEntryValidator {
    pub fn validate(definition: &LexerDefinition) -> Result<(), NexoraError> {
        let fail = || {
            NexoraError::Configuration(
                "invalid lexical entry context or expanded rule budget".into(),
            )
        };
        if definition.entry_contexts.len() > 64
            || (definition.schema_version < 13 && !definition.entry_contexts.is_empty())
        {
            return Err(fail());
        }
        let declared: BTreeSet<_> = definition
            .tokens
            .iter()
            .map(|rule| rule.kind.as_str())
            .collect();
        let mut expanded = 0;
        for (name, kinds) in &definition.entry_contexts {
            if name.is_empty()
                || name.len() > 64
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
                || kinds.is_empty()
                || kinds.len() > 128
            {
                return Err(fail());
            }
            let selected: BTreeSet<_> = kinds.iter().map(String::as_str).collect();
            if selected.len() != kinds.len()
                || selected
                    .iter()
                    .any(|kind| kind.is_empty() || kind.len() > 128 || !declared.contains(kind))
            {
                return Err(fail());
            }
            expanded += definition
                .tokens
                .iter()
                .filter(|rule| selected.contains(rule.kind.as_str()))
                .count();
            if expanded > 1024 {
                return Err(fail());
            }
        }
        Ok(())
    }
}
