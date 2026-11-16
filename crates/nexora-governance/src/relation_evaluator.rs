use crate::*;
use nexora_graph::{CodeNode, EdgeKind, ResolutionState};
use std::collections::{HashSet, VecDeque};
pub struct RelationEvaluator;
impl RelationEvaluator {
    pub fn evaluate(
        context: &EvaluationContext<'_>,
        node: &CodeNode,
        relation: &CompiledRelation,
        budget: &mut EvaluationBudget,
    ) -> PredicateObservation {
        let mut queue = VecDeque::from([(node, 0, Vec::new())]);
        let mut visited = HashSet::new();
        let mut complete = true;
        let mut evidence = Vec::new();
        while let Some((current, depth, path)) = queue.pop_front() {
            if !visited.insert(&current.id) {
                continue;
            }
            if visited.len() > relation.traversal.max_visits {
                complete = false;
                break;
            }
            if depth >= relation.traversal.max_depth {
                continue;
            }
            complete &= context.exhaustive(current, &relation.kind);
            for edge in &context.snapshot.graph.edges {
                if !budget.consume() {
                    return Self::unknown("evaluation operation budget exhausted", evidence);
                }
                if edge.source != current.id || edge.kind != relation.kind {
                    continue;
                }
                if evidence.len() < 1000 {
                    evidence.push(edge.id.clone());
                }
                let valid_evidence = context
                    .snapshot
                    .sources
                    .get(&edge.evidence.path)
                    .and_then(|s| {
                        s.get(edge.evidence.span.start.offset..edge.evidence.span.end.offset)
                    })
                    .is_some();
                if edge.resolution != ResolutionState::Resolved || !valid_evidence {
                    complete = false;
                    continue;
                }
                let Some(target) = context.node(&edge.target) else {
                    complete = false;
                    continue;
                };
                let matches = Self::matches(
                    context,
                    target,
                    &relation.target,
                    relation.target_file_pattern.as_ref(),
                    budget,
                );
                let mut target_path = path.clone();
                target_path.push(edge.id.clone());
                if matches == EvaluationStatus::Pass {
                    return PredicateObservation {
                        status: EvaluationStatus::Pass,
                        edge_ids: target_path,
                        exhaustive: false,
                        message: "resolved source-backed relation path exists".into(),
                    };
                }
                if matches == EvaluationStatus::Unknown {
                    complete = false;
                }
                if queue.len() >= relation.traversal.max_visits {
                    complete = false;
                } else {
                    queue.push_back((target, depth + 1, target_path));
                }
            }
        }
        PredicateObservation {
            status: if complete {
                EvaluationStatus::Fail
            } else {
                EvaluationStatus::Unknown
            },
            edge_ids: evidence,
            exhaustive: complete,
            message: if complete {
                "no matching relation in exhaustively inspected bounded scope"
            } else {
                "relation coverage, resolution, evidence or traversal budget is incomplete"
            }
            .into(),
        }
    }
    fn unknown(message: &str, edge_ids: Vec<nexora_core::EdgeId>) -> PredicateObservation {
        PredicateObservation {
            status: EvaluationStatus::Unknown,
            edge_ids,
            exhaustive: false,
            message: message.into(),
        }
    }
    pub fn named(
        context: &EvaluationContext<'_>,
        node: &CodeNode,
        kind: EdgeKind,
        name: &str,
        budget: &mut EvaluationBudget,
    ) -> EvaluationStatus {
        Self::evaluate(
            context,
            node,
            &CompiledRelation {
                kind,
                target: PolicyRelationTarget {
                    name: Some(name.into()),
                    ..Default::default()
                },
                traversal: Default::default(),
                target_file_pattern: None,
            },
            budget,
        )
        .status
    }
    fn matches(
        context: &EvaluationContext<'_>,
        node: &CodeNode,
        target: &PolicyRelationTarget,
        file_pattern: Option<&regex::Regex>,
        budget: &mut EvaluationBudget,
    ) -> EvaluationStatus {
        if target.symbol.as_ref().is_some_and(|k| *k != node.kind)
            || target.name.as_ref().is_some_and(|n| *n != node.name)
        {
            return EvaluationStatus::Fail;
        }
        let anchor = context.anchor(node);
        if let Some(pattern) = file_pattern {
            match anchor.path.as_ref() {
                Some(path) if !pattern.is_match(path) => return EvaluationStatus::Fail,
                None => return EvaluationStatus::Unknown,
                _ => {}
            }
        }
        if !anchor.verified {
            return EvaluationStatus::Unknown;
        }
        let mut checks = vec![EvaluationStatus::Pass];
        if let Some(name) = &target.inherits {
            checks.push(Self::named(context, node, EdgeKind::Extends, name, budget));
        }
        if let Some(name) = &target.implements {
            checks.push(Self::named(
                context,
                node,
                EdgeKind::Implements,
                name,
                budget,
            ));
        }
        EvaluationStatus::all(&checks)
    }
}
