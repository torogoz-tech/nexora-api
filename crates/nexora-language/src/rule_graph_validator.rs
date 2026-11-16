use crate::{GrammarDefinition, GrammarPattern as P};
use nexora_core::NexoraError;
use std::collections::{BTreeMap, BTreeSet};
pub struct RuleGraphValidator;
impl RuleGraphValidator {
    fn facts(
        pattern: &P,
        nullable: &BTreeMap<String, bool>,
        productive: &BTreeMap<String, bool>,
    ) -> (bool, bool) {
        match pattern {
            P::Rule { name } => (
                *nullable.get(name).unwrap_or(&false),
                *productive.get(name).unwrap_or(&false),
            ),
            P::Sequence { items } => {
                let values: Vec<_> = items
                    .iter()
                    .map(|p| Self::facts(p, nullable, productive))
                    .collect();
                (values.iter().all(|v| v.0), values.iter().all(|v| v.1))
            }
            P::Choice { alternatives } => {
                let values: Vec<_> = alternatives
                    .iter()
                    .map(|p| Self::facts(p, nullable, productive))
                    .collect();
                (values.iter().any(|v| v.0), values.iter().any(|v| v.1))
            }
            P::Optional { .. } | P::LineEnd | P::EndOfContext | P::GapWithout { .. } => {
                (true, true)
            }
            P::Repeat { item, min, .. } => {
                let f = Self::facts(item, nullable, productive);
                (*min == 0 || f.0, *min == 0 || f.1)
            }
            P::Capture { item, .. } => Self::facts(item, nullable, productive),
            _ => (false, true),
        }
    }
    fn leading(pattern: &P, nullable: &BTreeMap<String, bool>, result: &mut BTreeSet<String>) {
        match pattern {
            P::Rule { name } => {
                result.insert(name.clone());
            }
            P::Sequence { items } => {
                for item in items {
                    Self::leading(item, nullable, result);
                    if !Self::facts(item, nullable, &BTreeMap::new()).0 {
                        break;
                    }
                }
            }
            P::Choice { alternatives } => {
                for item in alternatives {
                    Self::leading(item, nullable, result);
                }
            }
            P::Capture { item, .. } | P::Optional { item } | P::Repeat { item, .. } => {
                Self::leading(item, nullable, result)
            }
            _ => {}
        }
    }
    fn visit(
        name: &str,
        edges: &BTreeMap<String, BTreeSet<String>>,
        active: &mut BTreeSet<String>,
        done: &mut BTreeSet<String>,
    ) -> Result<(), NexoraError> {
        if done.contains(name) {
            return Ok(());
        }
        if !active.insert(name.into()) {
            return Err(NexoraError::Configuration(
                "grammar: left-recursive rule; factor the production".into(),
            ));
        }
        if let Some(children) = edges.get(name) {
            for child in children {
                Self::visit(child, edges, active, done)?;
            }
        }
        active.remove(name);
        done.insert(name.into());
        Ok(())
    }
    pub fn validate(grammar: &GrammarDefinition) -> Result<BTreeMap<String, bool>, NexoraError> {
        let mut nullable: BTreeMap<_, _> =
            grammar.rules.keys().map(|k| (k.clone(), false)).collect();
        let mut productive = nullable.clone();
        for _ in 0..=grammar.rules.len() {
            let mut changed = false;
            for (name, pattern) in &grammar.rules {
                let (n, p) = Self::facts(pattern, &nullable, &productive);
                if n && !nullable[name] {
                    nullable.insert(name.clone(), true);
                    changed = true;
                }
                if p && !productive[name] {
                    productive.insert(name.clone(), true);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        if productive.values().any(|p| !*p) {
            return Err(NexoraError::Configuration(
                "grammar: rule has no finite derivation".into(),
            ));
        }
        let mut edges = BTreeMap::new();
        for (name, pattern) in &grammar.rules {
            let mut leading = BTreeSet::new();
            Self::leading(pattern, &nullable, &mut leading);
            edges.insert(name.clone(), leading);
        }
        let mut done = BTreeSet::new();
        for name in grammar.rules.keys() {
            Self::visit(name, &edges, &mut BTreeSet::new(), &mut done)?;
        }
        Ok(nullable)
    }
}
