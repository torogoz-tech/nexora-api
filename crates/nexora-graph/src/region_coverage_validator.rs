use crate::{CodeGraph, CodeNode, EdgeKind};
use nexora_core::{
    LexicalRegionComponent as C, NexoraError, NodeId, RegionCoverageMethod as M, RegionStatus as S,
};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) struct RegionCoverageValidator;
impl RegionCoverageValidator {
    pub fn validate(
        graph: &CodeGraph,
        nodes: &BTreeMap<&NodeId, &CodeNode>,
    ) -> Result<(), NexoraError> {
        let fail = || NexoraError::Configuration("invalid lexical component coverage proof".into());
        let mut children: BTreeMap<NodeId, BTreeSet<NodeId>> = BTreeMap::new();
        let interpretations: BTreeSet<_> = graph
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Interprets)
            .map(|e| {
                (
                    &e.source,
                    &e.target,
                    if e.label.as_deref() == Some("header") {
                        C::Header
                    } else {
                        C::Content
                    },
                )
            })
            .collect();
        for node in &graph.nodes {
            if let Some(region) = &node.lexical_region {
                let mut contents = node
                    .regions
                    .iter()
                    .filter(|r| r.capture == C::Content.capture());
                if contents
                    .next()
                    .is_none_or(|r| r.span != region.content_span)
                    || contents.next().is_some()
                {
                    return Err(fail());
                }
                let mut headers = node
                    .regions
                    .iter()
                    .filter(|r| r.capture == C::Header.capture());
                match (region.header, headers.next()) {
                    (Some(span), Some(row)) => {
                        if row.span != span || headers.next().is_some() {
                            return Err(fail());
                        }
                    }
                    (Some(_), None) => {
                        if region.header_content_span.is_some()
                            || node.regions.iter().any(|r| r.status == S::Parsed)
                            || node
                                .syntax_unit
                                .as_ref()
                                .and_then(|id| graph.syntax_units.get(id))
                                .is_some_and(|u| u.status == S::Parsed)
                        {
                            return Err(fail());
                        }
                    }
                    (None, Some(_)) => return Err(fail()),
                    (None, None) => {}
                }
                if let Some(input) = region.header_content_span {
                    let header = region.header.ok_or_else(fail)?;
                    if input.start.offset <= region.span.start.offset
                        || input.start.offset > input.end.offset
                        || input.end.offset > header.end.offset
                    {
                        return Err(fail());
                    }
                } else if region.header_closed {
                    return Err(fail());
                }
                if node.evidence.as_ref().map(|e| e.span) != Some(region.span)
                    || region.id != node.id
                    || region.content_span.start.offset < region.span.start.offset
                    || region.content_span.end.offset > region.span.end.offset
                    || region.content_span.start.offset > region.content_span.end.offset
                {
                    return Err(fail());
                }
                if let Some(lookahead) = region.lookahead_span
                    && (!region.closed
                        || region.closer.is_some()
                        || lookahead.start != region.span.end
                        || lookahead.end.offset <= lookahead.start.offset
                        || lookahead.end.offset - lookahead.start.offset > 128
                        || node
                            .syntax_unit
                            .as_ref()
                            .and_then(|id| graph.syntax_units.get(id))
                            .is_some_and(|unit| lookahead.end.offset > unit.span.end.offset))
                {
                    return Err(fail());
                }
                if let Some(parent) = &region.parent {
                    let parent_node = nodes.get(parent).ok_or_else(fail)?;
                    let p = parent_node.lexical_region.as_ref().ok_or_else(fail)?;
                    if node.syntax_unit != parent_node.syntax_unit
                        || region.span.start.offset < p.span.start.offset
                        || region.span.end.offset > p.span.end.offset
                        || region.span.end.offset - region.span.start.offset
                            >= p.span.end.offset - p.span.start.offset
                    {
                        return Err(fail());
                    }
                    children
                        .entry(parent.clone())
                        .or_default()
                        .insert(node.id.clone());
                }
            }
        }
        let mut incomplete = BTreeSet::new();
        for node in &graph.nodes {
            for content in &node.regions {
                if content.status != S::Parsed
                    && let Some(owner) = &node.syntax_unit
                {
                    incomplete.insert(owner);
                }
                let Some(proof) = &content.coverage else {
                    if node.lexical_region.is_some()
                        && C::from_capture(&content.capture).is_some()
                        && content.status == S::Parsed
                    {
                        return Err(fail());
                    }
                    continue;
                };
                let region = node.lexical_region.as_ref().ok_or_else(fail)?;
                let component = C::from_capture(&content.capture).ok_or_else(fail)?;
                let input = component.input(region).ok_or_else(fail)?;
                if content.status != S::Parsed
                    || !component.closed(region)
                    || component.coverage(region) != Some(content.span)
                {
                    return Err(fail());
                }
                let proof_children: BTreeSet<_> = proof.regions.iter().cloned().collect();
                let actual_children: BTreeSet<_> = children
                    .get(&node.id)
                    .into_iter()
                    .flatten()
                    .filter(|id| {
                        let span = nodes[id]
                            .lexical_region
                            .as_ref()
                            .expect("child lexical region")
                            .span;
                        input.start.offset <= span.start.offset
                            && span.end.offset <= input.end.offset
                    })
                    .cloned()
                    .collect();
                if proof_children.len() != proof.regions.len() || proof_children != actual_children
                {
                    return Err(fail());
                }
                for child in &proof.regions {
                    if nodes.get(child).is_none_or(|n| {
                        n.regions.is_empty() || n.regions.iter().any(|r| r.status != S::Parsed)
                    }) {
                        return Err(fail());
                    }
                }
                match proof.method {
                    M::Literal => {
                        if !proof.regions.is_empty() || !proof.syntax_units.is_empty() {
                            return Err(fail());
                        }
                    }
                    M::Carrier => {
                        if !proof.syntax_units.is_empty() {
                            return Err(fail());
                        }
                    }
                    M::Grammar | M::Guard => {
                        if proof.syntax_units.len() != 1 {
                            return Err(fail());
                        }
                        let id = &proof.syntax_units[0];
                        let target = graph
                            .syntax_units
                            .get(id)
                            .filter(|u| u.status == S::Parsed)
                            .ok_or_else(fail)?;
                        if proof.method == M::Grammar {
                            if target.span != input
                                || !interpretations.contains(&(&node.id, id, component))
                            {
                                return Err(fail());
                            }
                        } else {
                            let protected = component.protected(region).ok_or_else(fail)?;
                            if protected.start.offset < target.span.start.offset
                                || protected.end.offset > target.span.end.offset
                            {
                                return Err(fail());
                            }
                            let mut parent = region.parent.as_ref();
                            let mut found = false;
                            let mut depth = 0;
                            while let Some(ancestor) = parent {
                                depth += 1;
                                if depth > 256 {
                                    return Err(fail());
                                }
                                if [C::Content, C::Header]
                                    .iter()
                                    .any(|c| interpretations.contains(&(ancestor, id, *c)))
                                {
                                    found = true;
                                    break;
                                }
                                parent = nodes
                                    .get(ancestor)
                                    .and_then(|n| n.lexical_region.as_ref())
                                    .and_then(|r| r.parent.as_ref());
                            }
                            if !found {
                                return Err(fail());
                            }
                        }
                    }
                }
            }
        }
        if graph
            .syntax_units
            .iter()
            .any(|(id, u)| u.status == S::Parsed && incomplete.contains(id))
        {
            return Err(fail());
        }
        Ok(())
    }
}
