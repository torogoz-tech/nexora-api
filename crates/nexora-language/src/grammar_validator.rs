use crate::{
    GrammarDefinition, GrammarPattern, GraphDefinition, LexerDefinition,
    pattern_properties::PatternProperties,
};
use nexora_core::{NexoraError, SyntaxRole};
use std::collections::BTreeSet;
pub struct GrammarValidator;
impl GrammarValidator {
    fn error(message: &str) -> NexoraError {
        NexoraError::Configuration(format!("grammar: {message}"))
    }
    pub fn validate(
        grammar: &GrammarDefinition,
        lexer: &LexerDefinition,
        graph: &GraphDefinition,
    ) -> Result<(), NexoraError> {
        if ![2, 3, 4, 5, 6, 7, 8, 9, 10].contains(&grammar.schema_version)
            || grammar.productions.is_empty()
            || grammar.productions.len() > 128
            || grammar.rules.len() > 128
            || !(1..=256).contains(&grammar.max_nesting)
            || !(1..=2_000_000).contains(&grammar.max_steps)
            || grammar.groups.len() > 64
            || grammar.opaque_references.len() > 64
            || grammar.ignored.len() > 32
            || grammar.recovery.len() > 64
        {
            return Err(Self::error("unsupported schema or rule/resource limits"));
        }
        crate::region_validator::RegionValidator::header(grammar)?;
        crate::ExpressionValidator::validate(grammar, lexer, graph)?;
        let kinds: BTreeSet<_> = lexer
            .tokens
            .iter()
            .map(|t| t.kind.as_str())
            .chain(std::iter::once("keyword"))
            .collect();
        let mut opens = BTreeSet::new();
        for group in &grammar.groups {
            if group.open.is_empty()
                || group.close.is_empty()
                || group.open == group.close
                || !opens.insert((&group.open, &group.kind))
                || group
                    .kind
                    .as_ref()
                    .is_some_and(|k| !kinds.contains(k.as_str()))
            {
                return Err(Self::error("invalid or duplicate delimiter pair"));
            }
        }
        for selector in grammar
            .opaque_references
            .iter()
            .chain(grammar.recovery.iter())
        {
            if selector.text.is_empty()
                || selector
                    .kind
                    .as_ref()
                    .is_some_and(|k| !kinds.contains(k.as_str()))
            {
                return Err(Self::error("invalid token selector"));
            }
        }
        let nullable = crate::RuleGraphValidator::validate(grammar)?;
        let mut count = 0;
        let mut names = BTreeSet::new();
        for (name, pattern) in &grammar.rules {
            if name.is_empty() {
                return Err(Self::error("empty rule name"));
            }
            Self::properties(
                pattern,
                grammar,
                &kinds,
                &nullable,
                &mut BTreeSet::from([name.clone()]),
                0,
                &mut count,
            )?;
        }
        for production in &grammar.productions {
            if production.name.is_empty()
                || !names.insert(&production.name)
                || !graph.mappings.contains_key(&production.projection.ast_kind)
            {
                return Err(Self::error("duplicate production or missing AST mapping"));
            }
            let properties = Self::properties(
                &production.pattern,
                grammar,
                &kinds,
                &nullable,
                &mut BTreeSet::new(),
                0,
                &mut count,
            )?;
            if properties.possible.len() > 64
                || properties.nullable
                || !properties
                    .required
                    .contains(&production.projection.name_capture)
            {
                return Err(Self::error(
                    "production must advance and always capture its projected name",
                ));
            }
            crate::region_validator::RegionValidator::projection(
                production,
                &properties.required,
                grammar,
            )?;
            if production.projection.role == SyntaxRole::Reference
                && !graph
                    .reference_relations
                    .contains_key(&production.projection.ast_kind)
            {
                return Err(Self::error(
                    "reference projection requires relation mappings",
                ));
            }
        }
        for pattern in grammar
            .ignored
            .iter()
            .chain(grammar.contexts.values().flat_map(|c| c.ignored.iter()))
        {
            if Self::properties(
                pattern,
                grammar,
                &kinds,
                &nullable,
                &mut BTreeSet::new(),
                0,
                &mut count,
            )?
            .nullable
            {
                return Err(Self::error("ignored pattern must advance"));
            }
        }
        Ok(())
    }
    fn properties(
        pattern: &GrammarPattern,
        grammar: &GrammarDefinition,
        kinds: &BTreeSet<&str>,
        nullable: &std::collections::BTreeMap<String, bool>,
        active: &mut BTreeSet<String>,
        depth: usize,
        count: &mut usize,
    ) -> Result<PatternProperties, NexoraError> {
        *count += 1;
        if depth > 32 || *count > 8192 {
            return Err(Self::error("expanded pattern depth/complexity budget"));
        }
        let empty = || PatternProperties::default();
        match pattern {
            GrammarPattern::Literal { text, kind } => {
                if text.is_empty()
                    || text.len() > 1024
                    || kind.as_ref().is_some_and(|k| !kinds.contains(k.as_str()))
                {
                    return Err(Self::error("invalid literal/kind"));
                }
                Ok(empty())
            }
            GrammarPattern::Token { kind } => {
                if !kinds.contains(kind.as_str()) {
                    return Err(Self::error("unknown token kind"));
                }
                Ok(empty())
            }
            GrammarPattern::GapWithout { characters } => {
                if grammar.schema_version < 6
                    || characters.is_empty()
                    || characters.chars().count() > 16
                {
                    return Err(Self::error(
                        "gap constraint requires schema6 and 1..=16 characters",
                    ));
                }
                Ok(PatternProperties {
                    nullable: true,
                    ..empty()
                })
            }
            GrammarPattern::EndOfContext => {
                if grammar.schema_version < 7 {
                    return Err(Self::error("end_of_context requires schema7"));
                }
                Ok(PatternProperties {
                    nullable: true,
                    ..empty()
                })
            }
            GrammarPattern::LineEnd => Ok(PatternProperties {
                nullable: true,
                ..empty()
            }),
            GrammarPattern::Balanced { open } => {
                if !grammar.groups.iter().any(|p| p.open == *open) {
                    return Err(Self::error("balanced opener is not configured"));
                }
                Ok(empty())
            }
            GrammarPattern::ScanUntil { stop, allow_eof } => {
                if grammar.schema_version < 4
                    || (*allow_eof && grammar.schema_version < 5)
                    || (stop.is_empty() && !allow_eof)
                    || stop.len() > 16
                    || stop.iter().any(|s| {
                        s.text.is_empty()
                            || s.text.len() > 128
                            || s.kind.as_ref().is_some_and(|k| !kinds.contains(k.as_str()))
                    })
                {
                    return Err(Self::error("invalid scan_until boundary"));
                }
                Ok(empty())
            }
            GrammarPattern::Until { close } => {
                if !grammar.groups.iter().any(|p| p.close == *close) {
                    return Err(Self::error("until closer is not configured"));
                }
                Ok(empty())
            }
            GrammarPattern::Rule { name } => {
                let rule = grammar
                    .rules
                    .get(name)
                    .ok_or_else(|| Self::error("unknown rule reference"))?;
                if !active.insert(name.clone()) {
                    return Ok(PatternProperties {
                        nullable: *nullable.get(name).unwrap_or(&false),
                        ..empty()
                    });
                }
                let result =
                    Self::properties(rule, grammar, kinds, nullable, active, depth + 1, count);
                active.remove(name);
                result
            }
            GrammarPattern::Capture { name, item, join } => {
                if name.is_empty() || name.len() > 64 || join.len() > 16 {
                    return Err(Self::error("invalid capture"));
                }
                let mut p =
                    Self::properties(item, grammar, kinds, nullable, active, depth + 1, count)?;
                if p.nullable || p.possible.contains(name) {
                    return Err(Self::error(
                        "capture must consume input and have unique name",
                    ));
                }
                p.required.insert(name.clone());
                p.possible.insert(name.clone());
                Ok(p)
            }
            GrammarPattern::Optional { item } => {
                let mut p =
                    Self::properties(item, grammar, kinds, nullable, active, depth + 1, count)?;
                p.nullable = true;
                p.required.clear();
                Ok(p)
            }
            GrammarPattern::Repeat { item, min, max } => {
                if min > max || *max > 4096 || *max == 0 {
                    return Err(Self::error("invalid repetition bounds"));
                }
                let mut p =
                    Self::properties(item, grammar, kinds, nullable, active, depth + 1, count)?;
                if p.nullable || (*max > 1 && !p.possible.is_empty()) {
                    return Err(Self::error(
                        "repetition must advance; capture outside repeated item",
                    ));
                }
                if *min == 0 {
                    p.nullable = true;
                    p.required.clear();
                }
                Ok(p)
            }
            GrammarPattern::Sequence { items } => {
                if items.is_empty() || items.len() > 256 {
                    return Err(Self::error("empty or oversized sequence"));
                }
                let mut result = PatternProperties {
                    nullable: true,
                    ..empty()
                };
                for item in items {
                    let p =
                        Self::properties(item, grammar, kinds, nullable, active, depth + 1, count)?;
                    if !result.possible.is_disjoint(&p.possible) {
                        return Err(Self::error("duplicate capture on sequence path"));
                    }
                    result.nullable &= p.nullable;
                    result.required.extend(p.required);
                    result.possible.extend(p.possible);
                }
                Ok(result)
            }
            GrammarPattern::Choice { alternatives } => {
                if alternatives.is_empty() || alternatives.len() > 128 {
                    return Err(Self::error("empty or oversized choice"));
                }
                let mut result = empty();
                let mut first = true;
                for item in alternatives {
                    let p =
                        Self::properties(item, grammar, kinds, nullable, active, depth + 1, count)?;
                    result.nullable |= p.nullable;
                    if first {
                        result.required = p.required;
                        first = false;
                    } else {
                        result.required =
                            result.required.intersection(&p.required).cloned().collect();
                    }
                    result.possible.extend(p.possible);
                }
                Ok(result)
            }
        }
    }
}
