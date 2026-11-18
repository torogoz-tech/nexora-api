use crate::{
    CompositionLimits, CompositionResult, RegionInterpretation, SyntaxUnit,
    compiled_composition_profile::CompiledCompositionProfile,
    composition_budget::CompositionBudget, composition_task::CompositionTask,
};
use nexora_ast::AbstractSyntaxTree;
use nexora_core::{AnalysisDiagnostic, NexoraError, RegionStatus, SourceSpan};
use nexora_language::LanguageDefinition;
use nexora_symbols::SymbolTable;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub struct CompositionAnalyzer {
    root: String,
    profiles: BTreeMap<String, CompiledCompositionProfile>,
    limits: CompositionLimits,
    configuration: String,
}
impl CompositionAnalyzer {
    pub fn new(
        host: LanguageDefinition,
        dependencies: Vec<LanguageDefinition>,
    ) -> Result<Self, NexoraError> {
        Self::with_limits(host, dependencies, CompositionLimits::default())
    }
    pub fn with_limits(
        host: LanguageDefinition,
        dependencies: Vec<LanguageDefinition>,
        limits: CompositionLimits,
    ) -> Result<Self, NexoraError> {
        limits.validate()?;
        if dependencies.len() >= 64 {
            return Err(NexoraError::Limit(
                "composition catalog > 64 profiles".into(),
            ));
        }
        let root = host.manifest.id.0.clone();
        let mut profiles = BTreeMap::new();
        for language in std::iter::once(host).chain(dependencies) {
            let id = language.manifest.id.0.clone();
            if profiles.contains_key(&id) {
                return Err(NexoraError::Configuration(
                    "duplicate composition profile".into(),
                ));
            }
            let plan = crate::AnalysisPlan::compile(&language)?;
            let fingerprint = format!(
                "{:x}",
                Sha256::digest(format!("{}:{language:?}", crate::ANALYSIS_ENGINE_VERSION))
            );
            profiles.insert(
                id,
                CompiledCompositionProfile {
                    language,
                    plan,
                    fingerprint,
                },
            );
        }
        for profile in profiles.values() {
            if let Some(composition) = &profile.language.manifest.composition {
                for route in composition
                    .routes
                    .values()
                    .chain(composition.header_routes.values())
                {
                    let target = profiles.get(&route.language.0).ok_or_else(|| {
                        NexoraError::Configuration(format!(
                            "missing composition profile {}",
                            route.language.0
                        ))
                    })?;
                    if route.lexical_context.as_ref().is_some_and(|name| {
                        !target.language.lexer.entry_contexts.contains_key(name)
                    }) {
                        return Err(NexoraError::Configuration(
                            "unknown composition lexical entry context".into(),
                        ));
                    }
                    if !target
                        .language
                        .grammar
                        .contexts
                        .contains_key(&route.context)
                    {
                        return Err(NexoraError::Configuration(format!(
                            "unknown composition context {} in {}",
                            route.context, route.language.0
                        )));
                    }
                }
            }
        }
        let configuration = format!(
            "{:x}",
            Sha256::digest(format!(
                "{}:{root}:{:?}:{limits:?}",
                crate::ANALYSIS_ENGINE_VERSION,
                profiles
                    .iter()
                    .map(|(id, p)| (id, &p.fingerprint))
                    .collect::<Vec<_>>()
            ))
        );
        Ok(Self {
            root,
            profiles,
            limits,
            configuration,
        })
    }
    pub fn configuration_fingerprint(&self) -> &str {
        &self.configuration
    }
    pub(crate) fn resolve_modules(
        &self,
        graph: &mut nexora_graph::CodeGraph,
        diagnostics: &mut Vec<AnalysisDiagnostic>,
    ) -> Result<(), NexoraError> {
        for profile in self.profiles.values() {
            crate::ModuleResolver::resolve_prepared(
                graph,
                &profile.language.manifest,
                diagnostics,
                profile.plan.resolution.clone(),
            )?;
        }
        Ok(())
    }
    pub fn analyze_source(
        &self,
        path: &str,
        source: &str,
    ) -> Result<CompositionResult, NexoraError> {
        if path.is_empty() || path.len() > 4096 {
            return Err(NexoraError::Configuration(
                "source identity is empty or too long".into(),
            ));
        }
        let mut budget = CompositionBudget::new(self.limits.clone());
        budget.account_bytes(source.len())?;
        let positions = nexora_core::SourcePositionIndex::new(source)?;
        let end = positions.position(source.len())?;
        let root = CompositionTask {
            id: "unit:root".into(),
            language: self.root.clone(),
            context: None,
            lexical_context: None,
            span: SourceSpan {
                start: Default::default(),
                end,
            },
            scope: None,
        };
        let mut units = vec![self.analyze_unit(path, &positions, &root, &mut budget)?];
        let mut known = BTreeMap::from([(root.id, 0usize)]);
        let mut interpretations = Vec::new();
        let mut cursor = 0;
        while cursor < units.len() {
            let unit = &units[cursor];
            let profile = &self.profiles[&unit.language.0];
            // Discover from projected nodes, so every link has a real graph endpoint.
            let mut requests = Vec::new();
            if let Some(composition) = &profile.language.manifest.composition {
                for node in &unit.graph.nodes {
                    if let Some(region) = &node.lexical_region {
                        for (component, routes) in [
                            (
                                nexora_core::LexicalRegionComponent::Content,
                                &composition.routes,
                            ),
                            (
                                nexora_core::LexicalRegionComponent::Header,
                                &composition.header_routes,
                            ),
                        ] {
                            if let Some(route) = routes.get(&region.mode) {
                                let span = component.input(region).ok_or_else(|| {
                                    NexoraError::Configuration(
                                        "missing component input span".into(),
                                    )
                                })?;
                                requests.push((
                                    node.id.clone(),
                                    span,
                                    component.closed(region),
                                    route.clone(),
                                    component,
                                ));
                            }
                        }
                    }
                }
            }
            let source_unit = unit.id.clone();
            let enclosing = unit.span;
            for (region_id, span, closed, route, component) in requests {
                budget.account(0, 1)?;
                if span.start.offset < enclosing.start.offset
                    || span.end.offset > enclosing.end.offset
                    || span.end.offset - span.start.offset
                        >= enclosing.end.offset - enclosing.start.offset
                {
                    return Err(NexoraError::Configuration(
                        "embedded interpretation must reduce its source range".into(),
                    ));
                }
                let id = nexora_graph::SyntaxUnitIdentity::for_interpretation(
                    span,
                    &route.language.0,
                    &route.context,
                    route.lexical_context.as_deref(),
                    &route.scope,
                    &self.profiles[&route.language.0].fingerprint,
                )?;
                let target_index = if let Some(index) = known.get(&id) {
                    *index
                } else {
                    let task = CompositionTask {
                        id: id.clone(),
                        language: route.language.0.clone(),
                        context: Some(route.context),
                        lexical_context: route.lexical_context,
                        span,
                        scope: Some(route.scope),
                    };
                    let target = match self.analyze_unit(path, &positions, &task, &mut budget) {
                        Ok(unit) => unit,
                        Err(error @ NexoraError::Lexical(_)) => {
                            budget.account(0, 3)?;
                            self.failed_unit(path, &task, error)
                        }
                        Err(error) => return Err(error),
                    };
                    let index = units.len();
                    known.insert(id.clone(), index);
                    units.push(target);
                    index
                };
                interpretations.push(RegionInterpretation {
                    source_unit: source_unit.clone(),
                    region_id,
                    target_unit: id,
                    boundary_closed: closed,
                    component,
                    status: if closed {
                        units[target_index].status
                    } else {
                        RegionStatus::Partial
                    },
                });
            }
            cursor += 1;
        }
        crate::composition_coverage_evaluator::CompositionCoverageEvaluator::apply(
            path,
            &mut units,
            &mut interpretations,
            &self.profiles,
            &mut budget,
        )?;
        let complete = units.iter().all(|u| u.status == RegionStatus::Parsed)
            && interpretations
                .iter()
                .all(|link| link.status == RegionStatus::Parsed);
        let output_items = units
            .iter()
            .map(|u| u.graph.nodes.len() + u.graph.edges.len() + 3)
            .sum::<usize>()
            + interpretations.len();
        budget.account(0, output_items)?;
        let mut graph = crate::composition_graph_projector::CompositionGraphProjector::project(
            path,
            &units,
            &interpretations,
        )?;
        budget.account(
            0,
            graph
                .edges
                .iter()
                .filter(|e| e.kind == nexora_graph::EdgeKind::ScopeBridge)
                .count(),
        )?;
        crate::composition_binding_resolver::CompositionBindingResolver::apply(
            &mut graph,
            &mut units,
            &self.profiles,
            &mut budget,
        )?;
        crate::composition_call_resolver::CompositionCallResolver::apply(
            &mut graph,
            &mut units,
            &self.profiles,
            &mut budget,
        )?;
        Ok(CompositionResult {
            graph,
            schema_version: 7,
            analysis_engine: crate::ANALYSIS_ENGINE_VERSION.into(),
            configuration_fingerprint: self.configuration.clone(),
            source: nexora_core::SourceFile {
                path: path.into(),
                language: self.profiles[&self.root].language.manifest.id.clone(),
                content: source.into(),
            },
            units,
            interpretations,
            complete,
        })
    }
    fn analyze_unit(
        &self,
        path: &str,
        positions: &nexora_core::SourcePositionIndex<'_>,
        task: &CompositionTask,
        budget: &mut CompositionBudget,
    ) -> Result<SyntaxUnit, NexoraError> {
        let profile = &self.profiles[&task.language];
        budget.admit(task.span.end.offset - task.span.start.offset)?;
        let tokens = profile.plan.lexer.tokenize_indexed_range_in_context(
            positions,
            task.span.start.offset..task.span.end.offset,
            task.lexical_context.as_deref(),
        )?;
        budget.account(tokens.len(), 0)?;
        let allowance = budget.remaining_parser_steps();
        let measured = if let Some(context) = &task.context {
            profile.plan.parser.parse_in_context_measured(
                &tokens,
                context,
                task.span.start,
                allowance,
            )?
        } else {
            profile.plan.parser.parse_measured(&tokens, allowance)
        };
        budget.account_parser_steps(measured.steps)?;
        if measured.budget_exhausted && allowance <= profile.language.grammar.max_steps {
            return Err(NexoraError::Limit(
                "composition parser operations budget".into(),
            ));
        }
        let mut cst = measured.tree;
        if task.context.is_some() {
            cst.file_scope = task.scope.clone();
        }
        budget.account(0, cst.nodes.len())?;
        let ast = AbstractSyntaxTree::from_cst(&cst);
        let symbols = SymbolTable::from_ast(&ast);
        budget.account(
            0,
            ast.nodes.len() + symbols.symbols.len() + symbols.scopes.len(),
        )?;
        let projection = nexora_graph::CodeGraph::project_with_symbols(
            &ast,
            &symbols,
            path,
            &profile.language,
            cst.diagnostics.is_empty(),
        )?;
        budget.account(
            0,
            projection.graph.nodes.len() + projection.graph.edges.len(),
        )?;
        let mut diagnostics: Vec<_> = cst
            .diagnostics
            .iter()
            .map(|diagnostic| AnalysisDiagnostic {
                stage: "parser".into(),
                path: Some(path.into()),
                span: Some(diagnostic.span),
                message: diagnostic.message.clone(),
            })
            .collect();
        diagnostics.extend(projection.diagnostics);
        let status = if !ast.complete {
            RegionStatus::Partial
        } else if !symbols.complete {
            RegionStatus::Opaque
        } else {
            RegionStatus::Parsed
        };
        Ok(SyntaxUnit {
            direct_calls: profile
                .language
                .manifest
                .composition
                .as_ref()
                .is_some_and(|c| c.direct_calls),
            mutation_analysis: profile.language.graph.schema_version >= 10
                && profile
                    .language
                    .grammar
                    .contexts
                    .values()
                    .filter_map(|c| c.expression.as_ref())
                    .all(|e| {
                        profile
                            .language
                            .graph
                            .expression_bindings
                            .contains_key(&e.ast_kind)
                    }),
            id: task.id.clone(),
            language: profile.language.manifest.id.clone(),
            rule_version: profile.language.manifest.version.clone(),
            profile_fingerprint: profile.fingerprint.clone(),
            context: task.context.clone(),
            lexical_context: task.lexical_context.clone(),
            span: task.span,
            status,
            tokens,
            cst,
            ast,
            symbols,
            graph: projection.graph,
            diagnostics,
        })
    }
    fn failed_unit(&self, path: &str, task: &CompositionTask, error: NexoraError) -> SyntaxUnit {
        let profile = &self.profiles[&task.language];
        let message = error.to_string();
        let cst = nexora_parser::ConcreteSyntaxTree {
            context: task.context.clone(),
            hierarchical: true,
            file_scope: task.scope.clone(),
            diagnostics: vec![nexora_parser::ParseDiagnostic {
                message: message.clone(),
                span: task.span,
            }],
            ..Default::default()
        };
        let ast = AbstractSyntaxTree::from_cst(&cst);
        let symbols = SymbolTable::from_ast(&ast);
        SyntaxUnit {
            direct_calls: false,
            mutation_analysis: false,
            id: task.id.clone(),
            language: profile.language.manifest.id.clone(),
            rule_version: profile.language.manifest.version.clone(),
            profile_fingerprint: profile.fingerprint.clone(),
            context: task.context.clone(),
            lexical_context: task.lexical_context.clone(),
            span: task.span,
            status: RegionStatus::Partial,
            tokens: Vec::new(),
            cst,
            ast,
            symbols,
            graph: Default::default(),
            diagnostics: vec![AnalysisDiagnostic {
                stage: "lexer".into(),
                path: Some(path.into()),
                span: Some(task.span),
                message,
            }],
        }
    }
}
