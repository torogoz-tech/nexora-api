use crate::*;
use nexora_graph::EdgeKind;
use sha2::{Digest, Sha256};
pub struct PolicyCompiler;
impl PolicyCompiler {
    pub fn compile(policy: Policy) -> Result<CompiledPolicy, GovernanceError> {
        if policy.api_version != "nexora.dev/v1alpha1" || policy.kind != "Policy" {
            return Err(Self::error("expected nexora.dev/v1alpha1 Policy"));
        }
        let m = &policy.metadata;
        m.version.validate()?;
        if !Self::identifier(&m.id)
            || m.package.as_ref().is_some_and(|p| !Self::identifier(p))
            || m.name.trim().is_empty()
            || m.description.trim().is_empty()
            || m.category.trim().is_empty()
        {
            return Err(Self::error(
                "metadata id/package/name/description/category invalid",
            ));
        }
        if policy.spec.requires.len() > 32
            || policy.spec.scope.tenants.len() > 128
            || policy.spec.scope.repositories.len() > 128
        {
            return Err(GovernanceError::Limit("policy references/scope".into()));
        }
        let scope = &policy.spec.scope;
        if scope
            .organization
            .as_ref()
            .is_some_and(|v| v.trim().is_empty())
            || scope
                .tenants
                .iter()
                .chain(scope.repositories.iter())
                .any(|v| v.trim().is_empty())
        {
            return Err(Self::error(
                "scope organization/tenants/repositories must not be blank",
            ));
        }
        let target = &policy.spec.target;
        if target
            .language
            .as_ref()
            .is_some_and(|s| !Self::identifier(s))
        {
            return Err(Self::error("target.language invalid"));
        }
        let filter = target.filter.as_ref();
        if let Some(filter) = filter {
            if filter
                .inherits
                .as_ref()
                .is_some_and(|v| v.trim().is_empty())
                || filter
                    .implements
                    .as_ref()
                    .is_some_and(|v| v.trim().is_empty())
            {
                return Err(Self::error(
                    "target.where inherits/implements must not be blank",
                ));
            }
            if (filter.inherits.is_some() || filter.implements.is_some())
                && target.symbol.as_ref().is_some_and(|k| {
                    !matches!(
                        k,
                        nexora_graph::NodeKind::Class | nexora_graph::NodeKind::Interface
                    )
                })
            {
                return Err(Self::error(
                    "target.where: inheritance predicates require class/interface",
                ));
            }
            if let (Some(name), Some(pattern)) = (&filter.name, &filter.name_matches)
                && !Self::regex(pattern)?.is_match(name)
            {
                return Err(Self::error(
                    "target.where: name and nameMatches are incompatible",
                ));
            }
        }
        let compiled_target = CompiledTarget {
            kind: target.symbol.clone(),
            name_pattern: filter
                .and_then(|f| f.name_matches.as_ref())
                .map(|p| Self::regex(p))
                .transpose()?,
            file_pattern: filter
                .and_then(|f| f.file_matches.as_ref())
                .map(|p| Self::regex(p))
                .transpose()?,
            target: target.clone(),
        };
        let mut count = 0;
        let plan = Self::assertion(&policy.spec.assert, 0, &mut count, "spec.assert")?;
        let fingerprint = format!("{:x}", Sha256::digest(serde_json::to_vec(&policy)?));
        Ok(CompiledPolicy {
            policy,
            target: compiled_target,
            plan,
            fingerprint,
        })
    }
    pub fn identifier(value: &str) -> bool {
        !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
            && value != "."
            && value != ".."
    }
    fn error(message: &str) -> GovernanceError {
        GovernanceError::Configuration(message.into())
    }
    pub(crate) fn regex(pattern: &str) -> Result<regex::Regex, GovernanceError> {
        if pattern.len() > 1024 {
            return Err(GovernanceError::Limit("regex length".into()));
        }
        regex::RegexBuilder::new(pattern)
            .size_limit(1_000_000)
            .build()
            .map_err(|e| Self::error(&format!("regex: {e}")))
    }
    fn assertion(
        value: &PolicyAssertion,
        depth: usize,
        count: &mut usize,
        path: &str,
    ) -> Result<EvaluationPlan, GovernanceError> {
        *count += 1;
        if depth > 16 || *count > 256 {
            return Err(GovernanceError::Limit(format!(
                "{path}: assertion depth/complexity"
            )));
        }
        match value {
            PolicyAssertion::Group(group) => {
                let groups = [group.all.as_ref(), group.any.as_ref(), group.none.as_ref()];
                if groups.iter().filter(|v| v.is_some()).count() != 1 {
                    return Err(Self::error(&format!(
                        "{path}: exactly one of all/any/none required"
                    )));
                }
                let (kind, children) = groups
                    .iter()
                    .enumerate()
                    .find_map(|(i, v)| v.map(|x| (i, x)))
                    .ok_or_else(|| Self::error(path))?;
                if children.is_empty() {
                    return Err(Self::error(&format!("{path}: empty logical operator")));
                }
                let children = children
                    .iter()
                    .enumerate()
                    .map(|(i, c)| Self::assertion(c, depth + 1, count, &format!("{path}[{i}]")))
                    .collect::<Result<Vec<_>, _>>()?;
                if kind == 0 {
                    Self::compatible(&children, path)?;
                }
                Ok(match kind {
                    0 => EvaluationPlan::All(children),
                    1 => EvaluationPlan::Any(children),
                    _ => EvaluationPlan::None(children),
                })
            }
            PolicyAssertion::Predicate(PolicyPredicate::Attribute(a)) => {
                if !["name", "kind", "file"].contains(&a.attribute.as_str()) {
                    return Err(Self::error(&format!(
                        "{path}: unknown attribute {}",
                        a.attribute
                    )));
                }
                if !["equals", "notEquals", "matches", "exists"].contains(&a.operator.as_str()) {
                    return Err(Self::error(&format!(
                        "{path}: unknown operator {}",
                        a.operator
                    )));
                }
                if (a.operator == "exists") != a.value.is_none() {
                    return Err(Self::error(&format!(
                        "{path}: exists forbids value; other operators require value"
                    )));
                }
                if a.attribute == "kind" && a.operator == "matches" {
                    return Err(Self::error(&format!(
                        "{path}: kind requires exact comparison, not matches"
                    )));
                }
                let pattern = if a.operator == "matches" {
                    a.value.as_ref().map(|v| Self::regex(v)).transpose()?
                } else {
                    None
                };
                Ok(EvaluationPlan::Attribute(CompiledAttribute {
                    attribute: a.attribute.clone(),
                    operator: a.operator.clone(),
                    value: a.value.clone(),
                    pattern,
                }))
            }
            PolicyAssertion::Predicate(PolicyPredicate::Relation(r)) => {
                let kind = Self::relation(&r.relation)?;
                if r.target
                    .inherits
                    .as_ref()
                    .is_some_and(|v| v.trim().is_empty())
                    || r.target
                        .implements
                        .as_ref()
                        .is_some_and(|v| v.trim().is_empty())
                {
                    return Err(Self::error(&format!(
                        "{path}: relation target inherits/implements must not be blank"
                    )));
                }
                if !(1..=32).contains(&r.traversal.max_depth)
                    || !(1..=1000).contains(&r.traversal.max_visits)
                {
                    return Err(Self::error(&format!(
                        "{path}: maxDepth 1..32, maxVisits 1..1000"
                    )));
                }
                if (r.target.inherits.is_some() || r.target.implements.is_some())
                    && r.target.symbol.as_ref().is_some_and(|k| {
                        !matches!(
                            k,
                            nexora_graph::NodeKind::Class | nexora_graph::NodeKind::Interface
                        )
                    })
                {
                    return Err(Self::error(&format!(
                        "{path}: incompatible relation target inheritance"
                    )));
                }
                Ok(EvaluationPlan::Relation(CompiledRelation {
                    kind,
                    target: r.target.clone(),
                    traversal: r.traversal.clone(),
                    target_file_pattern: r
                        .target
                        .file_matches
                        .as_ref()
                        .map(|p| Self::regex(p))
                        .transpose()?,
                }))
            }
        }
    }
    fn compatible(children: &[EvaluationPlan], path: &str) -> Result<(), GovernanceError> {
        let attributes: Vec<_> = children
            .iter()
            .filter_map(|p| {
                if let EvaluationPlan::Attribute(a) = p {
                    Some(a)
                } else {
                    None
                }
            })
            .collect();
        for a in attributes.iter().filter(|a| a.operator == "equals") {
            for b in attributes.iter().filter(|b| b.attribute == a.attribute) {
                let incompatible = match b.operator.as_str() {
                    "equals" => a.value != b.value,
                    "notEquals" => a.value == b.value,
                    "matches" => a
                        .value
                        .as_ref()
                        .is_some_and(|v| b.pattern.as_ref().is_some_and(|p| !p.is_match(v))),
                    _ => false,
                };
                if incompatible {
                    return Err(Self::error(&format!(
                        "{path}: contradictory conditions on {}",
                        a.attribute
                    )));
                }
            }
        }
        Ok(())
    }
    pub fn relation(name: &str) -> Result<EdgeKind, GovernanceError> {
        match name {
            "INTERPRETS" => Ok(EdgeKind::Interprets),
            "SCOPE_BRIDGE" => Ok(EdgeKind::ScopeBridge),
            "CONTAINS" => Ok(EdgeKind::Contains),
            "DECLARES" => Ok(EdgeKind::Declares),
            "IMPORTS" => Ok(EdgeKind::Imports),
            "EXPORTS" => Ok(EdgeKind::Exports),
            "CALLS" => Ok(EdgeKind::Calls),
            "REFERENCES" => Ok(EdgeKind::References),
            "EXTENDS" => Ok(EdgeKind::Extends),
            "IMPLEMENTS" => Ok(EdgeKind::Implements),
            "DEPENDS_ON" => Ok(EdgeKind::DependsOn),
            _ => Err(Self::error(&format!("unknown relation {name}"))),
        }
    }
}
