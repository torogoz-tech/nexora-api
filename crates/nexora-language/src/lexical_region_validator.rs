use crate::LexerDefinition;
use nexora_core::NexoraError;
pub struct LexicalRegionValidator;
impl LexicalRegionValidator {
    pub fn validate(d: &LexerDefinition) -> Result<(), NexoraError> {
        crate::LexicalBoundaryValidator::validate(d)?;
        crate::lexical_counted_validator::LexicalCountedValidator::validate(d)?;
        crate::LexicalTransitionBudget::validate(d)?;
        let fail =
            || NexoraError::Configuration("invalid lexical regions or resource limits".into());
        if !(1..=100_000).contains(&d.max_region_nodes)
            || !(1..=2_000_000).contains(&d.max_region_steps)
            || !(1..=256).contains(&d.max_region_depth)
            || d.regions.len() > 64
            || (d.schema_version < 3
                && (!d.regions.is_empty()
                    || d.tokens.iter().any(|r| r.region.is_some())
                    || d.max_region_steps != 2_000_000
                    || d.max_region_depth != 128
                    || d.max_region_nodes != 100_000))
        {
            return Err(fail());
        }
        let mut validated_patterns = std::collections::BTreeSet::new();
        for (mode, r) in &d.regions {
            if mode.is_empty()
                || mode.len() > 128
                || r.ast_kind.is_empty()
                || r.ast_kind.len() > 128
                || (r.close.is_empty()
                    && (d.schema_version < 6
                        || (!r.eof_closes && r.close_before.is_empty())
                        || !r.alternate_closes.is_empty()
                        || r.close_capture.is_some()
                        || r.close_tail.is_some()))
                || r.close.len() > 128
                || r.escape.as_ref().is_some_and(|e| e.chars().count() != 1)
                || (d.schema_version < 7 && !r.literal_escapes.is_empty())
                || r.literal_escapes.len() > 16
                || r.literal_escapes
                    .iter()
                    .any(|s| s.is_empty() || s.len() > 128)
                || r.literal_escapes
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != r.literal_escapes.len()
                || r.children.len() > 32
                || r.alternate_closes.len() > 16
                || r.alternate_closes
                    .iter()
                    .any(|s| s.is_empty() || s.len() > 128)
                || std::iter::once(&r.close)
                    .chain(r.alternate_closes.iter())
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != r.alternate_closes.len() + 1
            {
                return Err(fail());
            }
            for phase in [
                r.children.as_slice(),
                r.header.as_ref().map_or(&[][..], |h| h.children.as_slice()),
            ] {
                let mut patterns = std::collections::BTreeSet::new();
                for child in phase {
                    if child.pattern.len() > 4096
                        || !d.regions.contains_key(&child.region)
                        || !patterns.insert(&child.pattern)
                    {
                        return Err(fail());
                    }
                    if !validated_patterns.insert(child.pattern.as_str()) {
                        continue;
                    }
                    let regex = regex::RegexBuilder::new(&format!("^(?:{})", child.pattern))
                        .size_limit(1_000_000)
                        .build()
                        .map_err(|_| fail())?;
                    let hir = regex_syntax::parse(&child.pattern).map_err(|_| fail())?;
                    if regex.is_match("") || !hir.properties().minimum_len().is_some_and(|n| n > 0)
                    {
                        return Err(fail());
                    }
                }
            }
        }
        for rule in d.tokens.iter().filter(|r| r.region.is_some()) {
            let hir = regex_syntax::parse(&rule.pattern).map_err(|_| fail())?;
            if !hir.properties().minimum_len().is_some_and(|n| n > 0) {
                return Err(fail());
            }
        }
        if d.tokens.iter().any(|r| {
            r.region.as_ref().is_some_and(|name| {
                !d.regions.contains_key(name) || r.reject || (r.trivia && d.schema_version < 11)
            })
        }) {
            return Err(fail());
        }
        crate::LexicalDynamicValidator::validate(d)?;
        Ok(())
    }
}
