use crate::LexerDefinition;
use nexora_core::NexoraError;
pub struct LexicalDynamicValidator;
impl LexicalDynamicValidator {
    pub fn validate(d: &LexerDefinition) -> Result<(), NexoraError> {
        let fail = || {
            NexoraError::Configuration("invalid dynamic lexical region or incoming capture".into())
        };
        for r in d.regions.values() {
            if (d.schema_version < 4
                && (r.opening_follow.is_some()
                    || r.header.is_some()
                    || r.close_capture.is_some()
                    || r.close_tail.is_some()
                    || r.close_ascii_case_insensitive
                    || !r.recovery_prefixes.is_empty()
                    || !r.recovery_barrier))
                || r.close_capture
                    .as_ref()
                    .is_some_and(|s| s.is_empty() || s.len() > 128)
                || r.recovery_prefixes.len() > 16
                || r.recovery_prefixes
                    .iter()
                    .any(|s| s.is_empty() || s.len() > 128)
            {
                return Err(fail());
            }
            for tail in r.close_tail.iter().chain(r.opening_follow.iter()) {
                if tail.len() > 4096 {
                    return Err(fail());
                }
                regex::RegexBuilder::new(&format!("^(?:{tail})"))
                    .size_limit(1_000_000)
                    .build()
                    .map_err(|_| fail())?;
            }
            if let Some(h) = &r.header
                && (h.end.is_empty()
                    || h.end.len() > 128
                    || h.terminal
                        .as_ref()
                        .is_some_and(|t| t.is_empty() || t.len() > 128 || t == &h.end)
                    || h.escape.as_ref().is_some_and(|s| s.chars().count() != 1)
                    || h.children.len() > 32)
            {
                return Err(fail());
            }
        }
        let incoming = d
            .tokens
            .iter()
            .filter_map(|t| t.region.as_ref().map(|r| (&t.pattern, r)))
            .chain(
                d.regions
                    .values()
                    .flat_map(|r| {
                        r.children
                            .iter()
                            .chain(r.header.iter().flat_map(|h| h.children.iter()))
                    })
                    .filter(|t| !t.reject)
                    .map(|t| (&t.pattern, &t.region)),
            );
        let mut compiled = std::collections::BTreeMap::new();
        for (pattern, target) in incoming {
            if !compiled.contains_key(pattern.as_str()) {
                let regex = regex::RegexBuilder::new(&format!("^(?:{pattern})"))
                    .size_limit(1_000_000)
                    .build()
                    .map_err(|_| fail())?;
                compiled.insert(pattern.as_str(), regex);
            }
            let names: Vec<_> = compiled[pattern.as_str()]
                .capture_names()
                .flatten()
                .collect();
            if names.len() > 32
                || d.regions.get(target).is_some_and(|r| {
                    r.counted_children
                        .iter()
                        .any(|c| !names.contains(&c.capture.as_str()))
                })
                || d.regions
                    .get(target)
                    .and_then(|r| r.close_capture.as_deref())
                    .is_some_and(|name| !names.contains(&name))
            {
                return Err(fail());
            }
        }
        Ok(())
    }
}
