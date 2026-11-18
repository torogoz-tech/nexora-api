use crate::{
    RegionInterpretation, SyntaxUnit, compiled_composition_profile::CompiledCompositionProfile,
    composition_budget::CompositionBudget, coverage_region::CoverageRegion,
};
use nexora_core::{
    AnalysisDiagnostic, LexicalRegionComponent as C, NexoraError, NodeId,
    RegionCoverageMethod as M, RegionCoverageProof, RegionStatus as S, SourceSpan,
};
use std::collections::BTreeMap;
pub(crate) struct CompositionCoverageEvaluator;
impl CompositionCoverageEvaluator {
    pub fn apply(
        path: &str,
        units: &mut [SyntaxUnit],
        links: &mut [RegionInterpretation],
        profiles: &BTreeMap<String, CompiledCompositionProfile>,
        budget: &mut CompositionBudget,
    ) -> Result<(), NexoraError> {
        let indexes: BTreeMap<_, _> = units
            .iter()
            .enumerate()
            .map(|(i, u)| (u.id.clone(), i))
            .collect();
        let link_indexes: BTreeMap<_, _> = links
            .iter()
            .enumerate()
            .map(|(i, l)| ((l.source_unit.clone(), l.region_id.clone(), l.component), i))
            .collect();
        let mut boundary_changed = vec![false; units.len()];
        for link in links.iter().filter(|link| !link.boundary_closed) {
            let index = indexes[&link.target_unit];
            if !boundary_changed[index] {
                let unit = &mut units[index];
                if unit.cst.diagnostics.len() < 1024 {
                    unit.cst.diagnostics.push(nexora_parser::ParseDiagnostic {
                        message: "unclosed origin boundary; interior coverage is partial".into(),
                        span: SourceSpan {
                            start: unit.span.start,
                            end: unit.span.start,
                        },
                    });
                }
                boundary_changed[index] = true;
                budget.account(0, 1)?;
            }
        }
        let mut order: Vec<_> = (0..units.len()).collect();
        order.sort_by_key(|i| units[*i].span.end.offset - units[*i].span.start.offset);
        for index in order {
            let profile = &profiles[&units[index].language.0];
            let roles = profile
                .language
                .manifest
                .composition
                .as_ref()
                .filter(|c| c.schema_version >= 2)
                .map(|c| &c.roles);
            let mut regions = Vec::new();
            let mut local = BTreeMap::new();
            for (cst_index, node) in units[index].cst.nodes.iter().enumerate() {
                if let Some(region) = &node.lexical_region {
                    let content_index = node
                        .regions
                        .iter()
                        .position(|r| r.capture == "embedded_content")
                        .ok_or_else(|| {
                            NexoraError::Configuration(
                                "lexical region has no content coverage".into(),
                            )
                        })?;
                    local.insert(region.id.clone(), regions.len());
                    let graph_id = NodeId(format!(
                        "{path}:ast:{}",
                        region.id.0.strip_prefix("cst:").unwrap_or(&region.id.0)
                    ));
                    regions.push(CoverageRegion {
                        cst_index,
                        content_index,
                        header_index: node
                            .regions
                            .iter()
                            .position(|r| r.capture == "embedded_header"),
                        header_role: profile
                            .language
                            .manifest
                            .composition
                            .as_ref()
                            .and_then(|c| c.header_roles.get(&region.mode))
                            .copied(),
                        header_link: link_indexes
                            .get(&(units[index].id.clone(), graph_id.clone(), C::Header))
                            .copied(),
                        parent: None,
                        children: Vec::new(),
                        role: roles.and_then(|r| r.get(&region.mode)).copied(),
                        link: link_indexes
                            .get(&(units[index].id.clone(), graph_id, C::Content))
                            .copied(),
                    });
                }
            }
            budget.account(0, regions.len())?;
            for i in 0..regions.len() {
                let region = units[index].cst.nodes[regions[i].cst_index]
                    .lexical_region
                    .as_ref()
                    .expect("region record");
                if let Some(parent) = region.parent.as_ref().and_then(|id| local.get(id)).copied() {
                    regions[i].parent = Some(parent);
                    regions[parent].children.push(i);
                }
            }
            let mut region_order: Vec<_> = (0..regions.len()).collect();
            region_order.sort_by_key(|i| {
                let span = units[index].cst.nodes[regions[*i].cst_index].span;
                span.end.offset - span.start.offset
            });
            let mut changed = boundary_changed[index];
            for i in region_order {
                for component in [C::Content, C::Header] {
                    let record = &regions[i];
                    let Some((content_index, role, link)) = record.component(component) else {
                        continue;
                    };
                    let node = &units[index].cst.nodes[record.cst_index];
                    let region = node.lexical_region.as_ref().expect("region record");
                    let coverage_span = component.coverage(region).expect("component span");
                    let input_span = component.input(region).expect("component input span");
                    let protected_span = component.protected(region).expect("protected span");
                    let children: Vec<_> = record
                        .children
                        .iter()
                        .copied()
                        .filter(|child| {
                            let span = units[index].cst.nodes[regions[*child].cst_index].span;
                            input_span.start.offset <= span.start.offset
                                && span.end.offset <= input_span.end.offset
                        })
                        .collect();
                    let child_partial = children.iter().any(|child| {
                        units[index].cst.nodes[regions[*child].cst_index]
                            .regions
                            .iter()
                            .any(|r| r.status == S::Partial)
                    });
                    let child_complete = children.iter().all(|child| {
                        units[index].cst.nodes[regions[*child].cst_index]
                            .regions
                            .iter()
                            .all(|r| r.status == S::Parsed)
                    });
                    let mut proof_units = Vec::new();
                    let diagnostic = units[index].cst.diagnostics.iter().any(|d| {
                        !d.message.starts_with("unclosed origin boundary;")
                            && !d.message.starts_with("unterminated lexical region ")
                            && Self::overlaps(d.span, coverage_span)
                    });
                    let status = if !component.closed(region) || child_partial || diagnostic {
                        S::Partial
                    } else {
                        match role {
                            Some(M::Literal) => S::Parsed,
                            Some(M::Carrier) => {
                                if child_complete {
                                    S::Parsed
                                } else {
                                    S::Opaque
                                }
                            }
                            Some(M::Grammar) => {
                                let link = link.ok_or_else(|| {
                                    NexoraError::Configuration(
                                        "grammar component has no interpretation".into(),
                                    )
                                })?;
                                let target = &units[indexes[&links[link].target_unit]];
                                if target.status == S::Partial {
                                    S::Partial
                                } else if target.status == S::Parsed && child_complete {
                                    proof_units.push(NodeId(target.id.clone()));
                                    S::Parsed
                                } else {
                                    S::Opaque
                                }
                            }
                            Some(M::Guard) => {
                                let mut ancestor = record.parent;
                                while let Some(parent) = ancestor {
                                    let parent = &regions[parent];
                                    for kind in [C::Content, C::Header] {
                                        if let Some((_, Some(M::Grammar), Some(link))) =
                                            parent.component(kind)
                                        {
                                            let target = &units[indexes[&links[link].target_unit]];
                                            if target.status == S::Parsed
                                                && target.span.start.offset
                                                    <= protected_span.start.offset
                                                && protected_span.end.offset
                                                    <= target.span.end.offset
                                            {
                                                proof_units.push(NodeId(target.id.clone()));
                                                break;
                                            }
                                        }
                                    }
                                    if !proof_units.is_empty() {
                                        break;
                                    }
                                    ancestor = parent.parent;
                                }
                                if child_complete && !proof_units.is_empty() {
                                    S::Parsed
                                } else {
                                    S::Opaque
                                }
                            }
                            None => S::Opaque,
                        }
                    };
                    let proof = if status == S::Parsed {
                        Some(RegionCoverageProof {
                            method: role.expect("parsed role"),
                            regions: children
                                .iter()
                                .map(|child| {
                                    units[index].cst.nodes[regions[*child].cst_index].id.clone()
                                })
                                .collect(),
                            syntax_units: proof_units,
                        })
                    } else {
                        None
                    };
                    if let Some(proof) = &proof {
                        budget
                            .account(0, 4 * (1 + proof.regions.len() + proof.syntax_units.len()))?;
                    }
                    let content =
                        &mut units[index].cst.nodes[record.cst_index].regions[content_index];
                    changed |= content.status != status;
                    content.status = status;
                    content.coverage = proof;
                }
            }
            let unit = &mut units[index];
            let mut partial: Vec<_> = unit
                .cst
                .nodes
                .iter()
                .flat_map(|n| &n.regions)
                .filter(|r| r.status == S::Partial)
                .map(|r| r.span.start.offset)
                .collect();
            partial.sort_unstable();
            budget.account(0, partial.len())?;
            // Lexical components already have independent bottom-up coverage.
            // Propagate failures only into enclosing grammar captures.
            for region in unit
                .cst
                .nodes
                .iter_mut()
                .filter(|n| n.lexical_region.is_none())
                .flat_map(|n| &mut n.regions)
            {
                let at = partial.partition_point(|offset| *offset < region.span.start.offset);
                if region.status == S::Parsed
                    && partial
                        .get(at)
                        .is_some_and(|offset| *offset < region.span.end.offset)
                {
                    region.status = S::Partial;
                    region.coverage = None;
                    changed = true;
                }
            }
            let failed = unit.diagnostics.iter().any(|d| d.stage == "lexer");
            if changed && !failed {
                unit.ast = nexora_ast::AbstractSyntaxTree::from_cst(&unit.cst);
                unit.symbols = nexora_symbols::SymbolTable::from_ast(&unit.ast);
                let projection = nexora_graph::CodeGraph::project_with_symbols(
                    &unit.ast,
                    &unit.symbols,
                    path,
                    &profile.language,
                    unit.cst.diagnostics.is_empty(),
                )?;
                budget.account(
                    0,
                    unit.ast.nodes.len()
                        + unit.symbols.symbols.len()
                        + unit.symbols.scopes.len()
                        + projection.graph.nodes.len()
                        + projection.graph.edges.len(),
                )?;
                unit.graph = projection.graph;
                unit.diagnostics = unit
                    .cst
                    .diagnostics
                    .iter()
                    .map(|d| AnalysisDiagnostic {
                        stage: if d.message.starts_with("unclosed origin boundary;") {
                            "composition-boundary"
                        } else {
                            "parser"
                        }
                        .into(),
                        path: Some(path.into()),
                        span: Some(d.span),
                        message: d.message.clone(),
                    })
                    .collect();
                unit.diagnostics.extend(projection.diagnostics);
            }
            unit.status = if !unit.cst.diagnostics.is_empty()
                || unit
                    .cst
                    .nodes
                    .iter()
                    .flat_map(|n| &n.regions)
                    .any(|r| r.status == S::Partial)
            {
                S::Partial
            } else if !unit.symbols.complete {
                S::Opaque
            } else {
                S::Parsed
            };
        }
        for link in links {
            link.status = if link.boundary_closed {
                units[indexes[&link.target_unit]].status
            } else {
                S::Partial
            };
        }
        Ok(())
    }
    fn overlaps(diagnostic: SourceSpan, region: SourceSpan) -> bool {
        diagnostic.start.offset < region.end.offset && diagnostic.end.offset >= region.start.offset
    }
}
