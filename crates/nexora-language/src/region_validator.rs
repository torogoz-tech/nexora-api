use crate::{GrammarDefinition, GrammarPattern, ProductionRule};
use nexora_core::NexoraError;
use std::collections::BTreeSet;
pub(crate) struct RegionValidator;
impl RegionValidator {
    pub fn header(g: &GrammarDefinition) -> Result<(), NexoraError> {
        let fail = || NexoraError::Configuration("invalid hierarchical contexts or roots".into());
        if g.schema_version < 6 && g.contexts.values().any(|c| c.stop_on_error) {
            return Err(fail());
        }
        if g.schema_version < 4 && g.contexts.values().any(|c| c.expression.is_some()) {
            return Err(fail());
        }
        if g.schema_version == 2 {
            if g.file_scope.is_some()
                || !g.contexts.is_empty()
                || !g.root_productions.is_empty()
                || g.productions.iter().any(|p| {
                    p.projection.scope.is_some()
                        || !p.projection.regions.is_empty()
                        || p.projection.role == nexora_core::SyntaxRole::Container
                })
            {
                return Err(fail());
            }
            return Ok(());
        }
        if g.file_scope.is_none() || g.root_productions.is_empty() || g.contexts.len() > 64 {
            return Err(fail());
        }
        if g.productions
            .iter()
            .any(|p| p.name.len() > 128 || p.projection.ast_kind.len() > 128)
        {
            return Err(fail());
        }
        let mut remaining = 8192;
        for pattern in g
            .ignored
            .iter()
            .chain(g.contexts.values().flat_map(|c| c.ignored.iter()))
        {
            Self::skips(
                pattern,
                g,
                &BTreeSet::new(),
                false,
                &mut BTreeSet::new(),
                &mut remaining,
                0,
            )?;
        }
        let names: BTreeSet<_> = g.productions.iter().map(|p| p.name.as_str()).collect();
        for list in
            std::iter::once(&g.root_productions).chain(g.contexts.values().map(|c| &c.productions))
        {
            if list.len() > 128
                || list.iter().collect::<BTreeSet<_>>().len() != list.len()
                || list.iter().any(|n| !names.contains(n.as_str()))
            {
                return Err(fail());
            }
        }
        if g.contexts.iter().any(|(n, c)| {
            n.is_empty()
                || n.len() > 64
                || c.ignored.len() > 32
                || (c.expression.is_some() != c.productions.is_empty())
                || (c.expression.is_some() && !c.ignored.is_empty())
        }) {
            return Err(fail());
        }
        Ok(())
    }
    pub fn projection(
        p: &ProductionRule,
        required: &BTreeSet<String>,
        g: &GrammarDefinition,
    ) -> Result<(), NexoraError> {
        let fail =
            || NexoraError::Configuration("invalid structured region capture/context".into());
        if p.projection.role == nexora_core::SyntaxRole::Reference
            && (p.projection.scope.is_some() || !p.projection.regions.is_empty())
        {
            return Err(fail());
        }
        let mut regions = BTreeSet::new();
        for r in &p.projection.regions {
            if !regions.insert(r.capture.as_str())
                || !required.contains(&r.capture)
                || r.trim_start > 256
                || r.trim_end > 256
                || r.context
                    .as_ref()
                    .is_some_and(|c| !g.contexts.contains_key(c))
            {
                return Err(fail());
            }
        }
        if regions.len() > 16 {
            return Err(fail());
        }
        if g.schema_version >= 3 {
            Self::skips(
                &p.pattern,
                g,
                &regions,
                false,
                &mut BTreeSet::new(),
                &mut 8192,
                0,
            )?;
        }
        Ok(())
    }
    fn skips(
        pattern: &GrammarPattern,
        g: &GrammarDefinition,
        regions: &BTreeSet<&str>,
        covered: bool,
        active: &mut BTreeSet<String>,
        remaining: &mut usize,
        depth: usize,
    ) -> Result<(), NexoraError> {
        if *remaining == 0 || depth > 32 {
            return Err(NexoraError::Limit(
                "region validation complexity/depth budget".into(),
            ));
        }
        *remaining -= 1;
        match pattern {
            GrammarPattern::Balanced { .. }
            | GrammarPattern::Until { .. }
            | GrammarPattern::ScanUntil { .. }
                if !covered =>
            {
                return Err(NexoraError::Configuration(
                    "schema3 groups must be captured and declared parsed or opaque".into(),
                ));
            }
            GrammarPattern::Capture { name, item, .. } => Self::skips(
                item,
                g,
                regions,
                covered || regions.contains(name.as_str()),
                active,
                remaining,
                depth + 1,
            )?,
            GrammarPattern::Rule { name } => {
                if active.insert(name.clone()) {
                    if let Some(p) = g.rules.get(name) {
                        Self::skips(p, g, regions, covered, active, remaining, depth + 1)?;
                    }
                    active.remove(name);
                }
            }
            GrammarPattern::Sequence { items }
            | GrammarPattern::Choice {
                alternatives: items,
            } => {
                for item in items {
                    Self::skips(item, g, regions, covered, active, remaining, depth + 1)?;
                }
            }
            GrammarPattern::Repeat { item, .. } | GrammarPattern::Optional { item } => {
                Self::skips(item, g, regions, covered, active, remaining, depth + 1)?
            }
            _ => {}
        }
        Ok(())
    }
}
