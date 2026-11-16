use crate::LexerDefinition;
use nexora_core::NexoraError;
use std::collections::BTreeSet;

pub(crate) struct LexicalCountedValidator;
impl LexicalCountedValidator {
    pub fn validate(d: &LexerDefinition) -> Result<(), NexoraError> {
        let fail =
            || NexoraError::Configuration("invalid counted lexical transition contract".into());
        for r in d.regions.values() {
            if r.counted_children.len() > 16
                || (d.schema_version < 14 && !r.counted_children.is_empty())
            {
                return Err(fail());
            }
            let mut openings = BTreeSet::new();
            for c in &r.counted_children {
                if [&c.capture, &c.unit, &c.open]
                    .iter()
                    .any(|s| s.is_empty() || s.len() > 128)
                    || !openings.insert(&c.open)
                    || c.max_run_factor.is_some_and(|n| !(2..=16).contains(&n))
                {
                    return Err(fail());
                }
                let child = d.regions.get(&c.region).ok_or_else(fail)?;
                if child.close.is_empty()
                    || child.close_capture.is_some()
                    || child.close_tail.is_some()
                    || !child.close_before.is_empty()
                    || !child.alternate_closes.is_empty()
                    || child.eof_closes
                    || !child.counted_children.is_empty()
                    || child
                        .header
                        .as_ref()
                        .and_then(|h| h.terminal.as_ref())
                        .is_some_and(|t| t != &child.close)
                {
                    return Err(fail());
                }
            }
        }
        Ok(())
    }
}
