use crate::{LiteralMode, ResolutionDefinition};
use nexora_core::NexoraError;
pub struct ResolutionValidator;
impl ResolutionValidator {
    pub fn validate(definition: &ResolutionDefinition) -> Result<(), NexoraError> {
        let error = |s: &str| NexoraError::Configuration(format!("resolution: {s}"));
        if !(1..=2_000_000).contains(&definition.max_steps) {
            return Err(error("resolution operation budget"));
        }
        let literal = &definition.literal;
        if literal.quotes.len() > 16
            || literal.quotes.iter().any(|q| q.chars().count() != 1)
            || literal
                .escape
                .as_ref()
                .is_some_and(|e| e.chars().count() != 1)
            || literal.escapes.len() > 64
            || literal
                .escapes
                .iter()
                .any(|(k, v)| k.chars().count() != 1 || v.len() > 32)
        {
            return Err(error("invalid literal decoding configuration"));
        }
        if matches!(literal.mode, LiteralMode::Quoted) && literal.quotes.is_empty() {
            return Err(error("quoted mode needs explicit delimiters"));
        }
        if definition.routes.is_empty() || definition.routes.len() > 16 {
            return Err(error("route budget"));
        }
        for route in &definition.routes {
            if route.reject.len() > 32
                || route.reject.iter().any(|s| s.is_empty() || s.len() > 32)
                || route.accept.len() > 1024
                || route.rewrites.len() > 16
                || route
                    .rewrites
                    .iter()
                    .any(|r| r.from.is_empty() || r.from.len() > 64 || r.to.len() > 64)
                || route.candidates.is_empty()
                || route.candidates.len() > 32
            {
                return Err(error("route configuration budget"));
            }
            regex::RegexBuilder::new(&route.accept)
                .size_limit(1_000_000)
                .build()
                .map_err(|e| error(&e.to_string()))?;
            for candidate in &route.candidates {
                let remainder = candidate
                    .template
                    .replace("{module}", "")
                    .replace("{extension}", "");
                if candidate.template.len() > 1024
                    || !candidate.template.contains("{module}")
                    || remainder.contains(['{', '}'])
                {
                    return Err(error(
                        "candidate must contain {module}; only {extension} additionally allowed",
                    ));
                }
            }
        }
        Ok(())
    }
}
