use crate::LexerDefinition;
use nexora_core::NexoraError;
use std::collections::BTreeSet;
pub struct LexicalTransitionBudget;
impl LexicalTransitionBudget {
    pub fn validate(definition: &LexerDefinition) -> Result<(), NexoraError> {
        let shared = definition.schema_version >= 12;
        let maximum = if shared {
            1024
        } else if definition.schema_version >= 8 {
            256
        } else {
            128
        };
        let mut references: usize = definition
            .regions
            .values()
            .map(|r| r.counted_children.len())
            .sum();
        if references > maximum {
            return Err(NexoraError::Configuration(
                "lexical transition definition/reference budget exceeded".into(),
            ));
        }
        let mut distinct = BTreeSet::new();
        for transition in definition.regions.values().flat_map(|region| {
            region.children.iter().chain(
                region
                    .header
                    .iter()
                    .flat_map(|header| header.children.iter()),
            )
        }) {
            references += 1;
            if shared {
                distinct.insert((
                    transition.pattern.as_str(),
                    transition.region.as_str(),
                    transition.reject,
                ));
            }
            if references > maximum || distinct.len() > 256 {
                return Err(NexoraError::Configuration(
                    "lexical transition definition/reference budget exceeded".into(),
                ));
            }
        }
        Ok(())
    }
}
