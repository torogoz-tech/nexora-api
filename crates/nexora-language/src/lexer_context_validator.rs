use crate::LexerDefinition;
use nexora_core::NexoraError;
use std::collections::BTreeSet;
pub struct LexerContextValidator;
impl LexerContextValidator {
    pub fn validate(definition: &LexerDefinition) -> Result<(), NexoraError> {
        if definition.tokens.len() > 128 {
            return Err(NexoraError::Limit("token rules > 128".into()));
        }
        crate::LexicalEntryValidator::validate(definition)?;
        crate::LexicalRegionValidator::validate(definition)?;
        let kinds: BTreeSet<_> = definition
            .tokens
            .iter()
            .filter(|t| !t.trivia)
            .map(|t| t.kind.as_str())
            .chain(std::iter::once("keyword"))
            .collect();
        if (definition.schema_version < 5
            && (definition.tokens.iter().any(|t| t.follow_exclude.is_some())
                || !definition.keyword_after_exclusions.is_empty()
                || definition
                    .regions
                    .values()
                    .any(|r| !r.token_rules.is_empty() || r.token_kind.is_some() || r.trivia)))
            || definition.keyword_after_exclusions.len() > 32
            || definition.keyword_after_exclusions.iter().any(|s| {
                s.text.is_empty()
                    || s.text.len() > 128
                    || s.kind.as_ref().is_some_and(|k| !kinds.contains(k.as_str()))
            })
            || definition.regions.values().any(|r| {
                r.token_rules.len() > 128
                    || r.token_rules.iter().collect::<BTreeSet<_>>().len() != r.token_rules.len()
                    || r.token_rules
                        .iter()
                        .any(|kind| !definition.tokens.iter().any(|t| &t.kind == kind))
                    || r.token_kind.as_ref().is_some_and(|kind| {
                        !definition.tokens.iter().any(|t| &t.kind == kind) && kind != "keyword"
                    })
            })
            || ![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14].contains(&definition.schema_version)
            || definition.tokens.iter().any(|t| {
                (definition.schema_version < 10 && !t.root)
                    || (definition.schema_version < 4 && t.reject_on_context_mismatch)
                    || (definition.schema_version < 2 && (!t.after.is_empty() || !t.at_start))
                    || (definition.schema_version < 8 && !t.after_kinds.is_empty())
                    || t.after_kinds.len() > 32
                    || t.after_kinds.iter().any(|kind| {
                        kind.is_empty() || kind.len() > 128 || !kinds.contains(kind.as_str())
                    })
                    || t.after_kinds.iter().collect::<BTreeSet<_>>().len() != t.after_kinds.len()
                    || t.after.len() > 32
                    || t.after.iter().any(|s| {
                        s.text.is_empty()
                            || s.text.len() > 128
                            || s.kind.as_ref().is_some_and(|k| !kinds.contains(k.as_str()))
                    })
            })
        {
            return Err(NexoraError::Configuration(
                "invalid lexer context or schema".into(),
            ));
        }
        for pattern in definition
            .tokens
            .iter()
            .filter_map(|t| t.follow_exclude.as_ref())
        {
            if pattern.len() > 4096 {
                return Err(NexoraError::Configuration(
                    "token follow exclusion > 4096".into(),
                ));
            }
            regex::RegexBuilder::new(&format!("^(?:{pattern})"))
                .size_limit(1_000_000)
                .build()
                .map_err(|e| NexoraError::Configuration(e.to_string()))?;
        }
        Ok(())
    }
}
