use crate::{ExpressionDefinition, GrammarDefinition, LexerDefinition, TokenSelector};
use nexora_core::NexoraError;
pub(crate) struct ConditionalOperatorValidator;
impl ConditionalOperatorValidator {
    pub fn validate(
        e: &ExpressionDefinition,
        g: &GrammarDefinition,
        l: &LexerDefinition,
        valid: &impl Fn(&TokenSelector) -> bool,
    ) -> Result<(), NexoraError> {
        let fail = || NexoraError::Configuration("invalid conditional expression contract".into());
        if e.conditional.len() > 16 || (!e.conditional.is_empty() && g.schema_version < 9) {
            return Err(fail());
        }
        if e.conditional.is_empty() {
            return Ok(());
        }
        let mut literal_patterns = Vec::new();
        if e.conditional
            .iter()
            .any(|c| c.token.kind.is_none() || c.separator.kind.is_none())
        {
            let mut unique = std::collections::BTreeMap::new();
            for rule in l
                .tokens
                .iter()
                .filter(|r| !r.trivia && e.literals.contains(&r.kind))
            {
                unique
                    .entry(rule.pattern.as_str())
                    .and_modify(|region: &mut bool| *region |= rule.region.is_some())
                    .or_insert(rule.region.is_some());
            }
            for (pattern, region) in unique {
                let compiled = regex::RegexBuilder::new(&format!(r"\A(?:{pattern})"))
                    .size_limit(1_000_000)
                    .build()
                    .map_err(|_| fail())?;
                literal_patterns.push((compiled, region));
            }
        }
        let literal_conflict = |selector: &TokenSelector| {
            if let Some(kind) = &selector.kind {
                return e.literals.contains(kind);
            }
            (e.literals.iter().any(|k| k == "keyword") && l.keywords.contains(&selector.text))
                || literal_patterns.iter().any(|(pattern, region)| {
                    pattern.find(&selector.text).is_some_and(|m| {
                        m.start() == 0 && (*region || m.end() == selector.text.len())
                    })
                })
        };
        let operators: Vec<_> = e
            .prefix
            .iter()
            .chain(&e.postfix)
            .chain(&e.member)
            .map(|o| &o.token)
            .chain(e.infix.iter().map(|o| &o.token))
            .chain(e.conditional.iter().map(|o| &o.token))
            .collect();
        for c in &e.conditional {
            if literal_conflict(&c.token)
                || literal_conflict(&c.separator)
                || !valid(&c.token)
                || !valid(&c.separator)
                || !(1..=1024).contains(&c.precedence)
                || c.consequent_precedence > 1024
                || c.alternative_precedence > 1024
                || operators.iter().any(|op| c.separator.overlaps(op))
                || e.literal_selectors
                    .iter()
                    .any(|s| s.overlaps(&c.token) || s.overlaps(&c.separator))
                || e.application
                    .iter()
                    .any(|a| a.separator.overlaps(&c.separator))
                || g.groups.iter().any(|group| {
                    [&group.open, &group.close].iter().any(|text| {
                        c.separator.overlaps(&TokenSelector {
                            text: (*text).clone(),
                            kind: group.kind.clone(),
                        })
                    })
                })
            {
                return Err(fail());
            }
        }
        Ok(())
    }
}
