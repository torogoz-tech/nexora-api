use crate::*;
use nexora_graph::{CodeNode, EdgeKind};
use sha2::{Digest, Sha256};
pub struct PolicyEvaluator;
impl PolicyEvaluator {
    pub fn evaluate(
        context: &EvaluationContext<'_>,
        policies: &[CompiledPolicy],
    ) -> Result<EvaluationResult, GovernanceError> {
        if policies.is_empty() || policies.len() > 128 {
            return Err(GovernanceError::Configuration(
                "require 1..128 compiled policies".into(),
            ));
        }
        context.validate_coverage()?;
        let mut ordered: Vec<_> = policies.iter().collect();
        ordered.sort_by_key(|p| p.policy.metadata.qualified_id());
        if ordered
            .windows(2)
            .any(|p| p[0].policy.metadata.qualified_id() == p[1].policy.metadata.qualified_id())
        {
            return Err(GovernanceError::Configuration(
                "duplicate compiled policy id".into(),
            ));
        }
        for p in &ordered {
            context.authorize(&p.policy.spec.scope)?;
        }
        let mut budget = EvaluationBudget::default();
        let mut findings = Vec::new();
        let mut fingerprints = Vec::new();
        for p in ordered {
            fingerprints.push(&p.fingerprint);
            let before = findings.len();
            for node in &context.snapshot.graph.nodes {
                if !budget.consume() {
                    return Err(GovernanceError::Limit(
                        "evaluation operation budget exhausted".into(),
                    ));
                }
                if p.target.kind.as_ref().is_some_and(|k| *k != node.kind) {
                    continue;
                }
                let selected = Self::select(context, node, &p.target, &mut budget);
                if selected == EvaluationStatus::Fail {
                    continue;
                }
                let mut evidence = Vec::new();
                let status = if selected == EvaluationStatus::Unknown {
                    evidence.push(Self::evidence(context,Some(node),"target.where",PredicateObservation{status:selected,edge_ids:vec![],exhaustive:false,message:"selection predicate has insufficient evidence or a different analyzed language".into()}));
                    selected
                } else {
                    Self::plan(
                        context,
                        node,
                        &p.plan,
                        &mut evidence,
                        &mut budget,
                        "spec.assert",
                    )?
                };
                findings.push(Self::finding(context, p, Some(node), status, evidence)?);
                if findings.len() > 10_000 {
                    return Err(GovernanceError::Limit("findings > 10000".into()));
                }
            }
            if findings.len() == before || !context.snapshot.diagnostics.is_empty() {
                let evidence = vec![Self::evidence(
                    context,
                    None,
                    "target",
                    PredicateObservation {
                        status: EvaluationStatus::Unknown,
                        edge_ids: vec![],
                        exhaustive: false,
                        message: if findings.len() == before {
                            "no selected source-backed symbols; no vacuous compliance"
                        } else {
                            "analysis diagnostics may hide additional selected declarations"
                        }
                        .into(),
                    },
                )];
                findings.push(Self::finding(
                    context,
                    p,
                    None,
                    EvaluationStatus::Unknown,
                    evidence,
                )?);
            }
        }
        findings.sort_by(|a, b| (&a.policy_id, &a.node_id).cmp(&(&b.policy_id, &b.node_id)));
        let policy_set_hash = format!("{:x}", Sha256::digest(serde_json::to_vec(&fingerprints)?));
        let status = EvaluationStatus::all(&findings.iter().map(|f| f.status).collect::<Vec<_>>());
        let mut result = EvaluationResult {
            id: String::new(),
            engine_version: ENGINE_VERSION.into(),
            policy_set_hash,
            tenant_id: context.snapshot.tenant_id.clone(),
            repository_id: context.snapshot.repository_id.clone(),
            snapshot_id: context.snapshot.id.clone(),
            language: context.language.into(),
            coverage: context.coverage.clone(),
            analysis_diagnostics: context.snapshot.diagnostics.clone(),
            status,
            findings,
        };
        result.id = format!("{:x}", Sha256::digest(serde_json::to_vec(&result)?));
        Ok(result)
    }
    fn finding(
        context: &EvaluationContext<'_>,
        p: &CompiledPolicy,
        node: Option<&CodeNode>,
        status: EvaluationStatus,
        evidence: Vec<GovernanceEvidence>,
    ) -> Result<GovernanceFinding, GovernanceError> {
        let seed = (
            &context.snapshot.tenant_id,
            &context.snapshot.repository_id,
            &context.snapshot.id,
            &p.fingerprint,
            node.map(|n| &n.id),
            status,
            &evidence,
        );
        Ok(GovernanceFinding {
            id: format!("{:x}", Sha256::digest(serde_json::to_vec(&seed)?)),
            policy_id: p.policy.metadata.qualified_id(),
            policy_version: p.policy.metadata.version.clone(),
            severity: p.policy.metadata.severity,
            node_id: node.map(|n| n.id.clone()),
            status,
            on_unknown: p.policy.spec.on_unknown,
            evidence,
            diagnostics: context
                .snapshot
                .diagnostics
                .iter()
                .filter(|d| {
                    node.is_none_or(|n| context.anchor(n).path.as_deref() == d.path.as_deref())
                })
                .take(16)
                .cloned()
                .collect(),
        })
    }
    fn select(
        context: &EvaluationContext<'_>,
        node: &CodeNode,
        target: &CompiledTarget,
        budget: &mut EvaluationBudget,
    ) -> EvaluationStatus {
        if let Some(requested) = target.target.language.as_deref() {
            if let Some(origin) = node
                .syntax_unit
                .as_ref()
                .and_then(|id| context.snapshot.graph.syntax_units.get(id))
            {
                if requested != origin.language.0 {
                    return EvaluationStatus::Fail;
                }
            } else if requested != context.language {
                return EvaluationStatus::Unknown;
            }
        }
        let anchor = context.anchor(node);
        let mut states = vec![if anchor.verified {
            EvaluationStatus::Pass
        } else {
            EvaluationStatus::Unknown
        }];
        if let Some(filter) = &target.target.filter {
            if filter.name.as_ref().is_some_and(|n| *n != node.name)
                || target
                    .name_pattern
                    .as_ref()
                    .is_some_and(|r| !r.is_match(&node.name))
            {
                return EvaluationStatus::Fail;
            }
            if let Some(pattern) = &target.file_pattern {
                states.push(match &anchor.path {
                    Some(p) => {
                        if pattern.is_match(p) {
                            EvaluationStatus::Pass
                        } else {
                            EvaluationStatus::Fail
                        }
                    }
                    None => EvaluationStatus::Unknown,
                });
            }
            if let Some(name) = &filter.inherits {
                states.push(RelationEvaluator::named(
                    context,
                    node,
                    EdgeKind::Extends,
                    name,
                    budget,
                ));
            }
            if let Some(name) = &filter.implements {
                states.push(RelationEvaluator::named(
                    context,
                    node,
                    EdgeKind::Implements,
                    name,
                    budget,
                ));
            }
        }
        EvaluationStatus::all(&states)
    }
    fn plan(
        context: &EvaluationContext<'_>,
        node: &CodeNode,
        plan: &EvaluationPlan,
        evidence: &mut Vec<GovernanceEvidence>,
        budget: &mut EvaluationBudget,
        path: &str,
    ) -> Result<EvaluationStatus, GovernanceError> {
        if !budget.consume() {
            return Err(GovernanceError::Limit(
                "evaluation operation budget exhausted".into(),
            ));
        }
        match plan {
            EvaluationPlan::All(children)
            | EvaluationPlan::Any(children)
            | EvaluationPlan::None(children) => {
                let statuses = children
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        Self::plan(context, node, p, evidence, budget, &format!("{path}[{i}]"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(match plan {
                    EvaluationPlan::All(_) => EvaluationStatus::all(&statuses),
                    EvaluationPlan::Any(_) => EvaluationStatus::any(&statuses),
                    _ => EvaluationStatus::none(&statuses),
                })
            }
            EvaluationPlan::Attribute(a) => {
                let anchor = context.anchor(node);
                let value = match a.attribute.as_str() {
                    "name" => Some(node.name.clone()),
                    "kind" => Some(format!("{:?}", node.kind).to_lowercase()),
                    "file" => anchor.path.clone(),
                    _ => None,
                };
                let status = if !anchor.verified {
                    EvaluationStatus::Unknown
                } else {
                    match value {
                        None => EvaluationStatus::Unknown,
                        Some(value) => {
                            let pass = match a.operator.as_str() {
                                "exists" => true,
                                "equals" => a.value.as_ref() == Some(&value),
                                "notEquals" => a.value.as_ref() != Some(&value),
                                "matches" => a.pattern.as_ref().is_some_and(|r| r.is_match(&value)),
                                _ => false,
                            };
                            if pass {
                                EvaluationStatus::Pass
                            } else {
                                EvaluationStatus::Fail
                            }
                        }
                    }
                };
                evidence.push(Self::evidence(
                    context,
                    Some(node),
                    path,
                    PredicateObservation {
                        status,
                        edge_ids: vec![],
                        exhaustive: status != EvaluationStatus::Unknown,
                        message: format!("attribute {} {} {:?}", a.attribute, a.operator, a.value),
                    },
                ));
                Ok(status)
            }
            EvaluationPlan::Relation(r) => {
                let mut observed = RelationEvaluator::evaluate(context, node, r, budget);
                observed.message.push_str(&format!(
                    "; maxDepth={}; maxVisits={}",
                    r.traversal.max_depth, r.traversal.max_visits
                ));
                let status = observed.status;
                evidence.push(Self::evidence(
                    context,
                    Some(node),
                    &format!("{path}.{:?}", r.kind),
                    observed,
                ));
                Ok(status)
            }
        }
    }
    fn evidence(
        context: &EvaluationContext<'_>,
        node: Option<&CodeNode>,
        predicate: &str,
        observed: PredicateObservation,
    ) -> GovernanceEvidence {
        let mut anchor = node.map(|n| context.anchor(n));
        if observed.status == EvaluationStatus::Pass
            && let Some(id) = observed.edge_ids.first()
            && let Some(edge) = context.snapshot.graph.edges.iter().find(|e| e.id == *id)
        {
            anchor = Some(NodeAnchor {
                path: Some(edge.evidence.path.clone()),
                span: Some(edge.evidence.span),
                verified: true,
            });
        }
        GovernanceEvidence {
            predicate: predicate.into(),
            result: observed.status,
            polarity: match observed.status {
                EvaluationStatus::Pass => "positive",
                EvaluationStatus::Fail => "negative",
                EvaluationStatus::Unknown => "insufficient",
            }
            .into(),
            node_id: node.map(|n| n.id.clone()),
            edge_ids: observed.edge_ids,
            path: anchor.as_ref().and_then(|a| a.path.clone()),
            span: anchor.and_then(|a| a.span),
            inspected_scope: format!(
                "tenant={};repository={};snapshot={};recognized top-level declarations",
                context.snapshot.tenant_id.0,
                context.snapshot.repository_id.0,
                context.snapshot.id.0
            ),
            exhaustive: observed.exhaustive,
            message: observed.message,
        }
    }
}
