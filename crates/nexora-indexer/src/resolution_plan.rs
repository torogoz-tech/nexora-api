use crate::{ModuleResolution, compiled_resolution_route::CompiledResolutionRoute};
use nexora_core::{NexoraError, NodeId};
use nexora_language::{CandidateCondition, ResolutionAnchor, ResolutionDefinition};
use std::collections::BTreeMap;
#[derive(Clone)]
pub struct ResolutionPlan {
    routes: Vec<CompiledResolutionRoute>,
    extensions: Vec<String>,
    remaining: usize,
}
impl ResolutionPlan {
    pub fn compile(
        definition: &ResolutionDefinition,
        extensions: &[String],
    ) -> Result<Self, NexoraError> {
        nexora_language::ResolutionValidator::validate(definition)?;
        let routes = definition
            .routes
            .iter()
            .map(|r| {
                regex::RegexBuilder::new(&r.accept)
                    .size_limit(1_000_000)
                    .build()
                    .map(|accept| CompiledResolutionRoute {
                        route: r.clone(),
                        accept,
                    })
                    .map_err(|e| NexoraError::Configuration(e.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            routes,
            extensions: extensions.to_vec(),
            remaining: definition.max_steps,
        })
    }
    fn normalize(base: &str, value: &str) -> Result<String, String> {
        if value.starts_with('/') || value.contains('\\') || value.chars().any(char::is_control) {
            return Err("absolute or invalid normalized path".into());
        }
        let mut parts: Vec<_> = base.split('/').filter(|s| !s.is_empty()).collect();
        for part in value.split('/') {
            match part {
                "" | "." => {}
                ".." => {
                    if parts.pop().is_none() {
                        return Err("reference path escapes repository root".into());
                    }
                }
                value => parts.push(value),
            }
        }
        let path = parts.join("/");
        if path.len() > 4096 {
            return Err("normalized path too large".into());
        }
        Ok(path)
    }
    pub fn find(
        &mut self,
        source: &str,
        specifier: &str,
        files: &BTreeMap<String, NodeId>,
    ) -> ModuleResolution {
        let mut candidates: BTreeMap<String, u16> = BTreeMap::new();
        let mut reason = "no configured route resolves reference in snapshot".to_owned();
        for compiled in &self.routes {
            let route = &compiled.route;
            if !compiled.accept.is_match(specifier)
                || route.reject.iter().any(|r| specifier.contains(r))
            {
                continue;
            }
            let mut value = specifier.to_owned();
            for rewrite in &route.rewrites {
                value = value.replace(&rewrite.from, &rewrite.to);
                if value.len() > 1024 {
                    return ModuleResolution::Unresolved("reference rewrite exceeds budget".into());
                }
            }
            let base = match route.anchor {
                ResolutionAnchor::Repository => "",
                ResolutionAnchor::SourceDirectory => {
                    source.rsplit_once('/').map_or("", |(parent, _)| parent)
                }
            };
            let module = match Self::normalize(base, &value) {
                Ok(path) => path,
                Err(error) => {
                    reason = error;
                    continue;
                }
            };
            for candidate in &route.candidates {
                if matches!(candidate.condition, CandidateCondition::WithoutExtension)
                    && std::path::Path::new(&module).extension().is_some()
                {
                    continue;
                }
                let extensions: Vec<&str> = if candidate.template.contains("{extension}") {
                    self.extensions.iter().map(String::as_str).collect()
                } else {
                    vec![""]
                };
                for extension in extensions {
                    if self.remaining == 0 {
                        return ModuleResolution::Unresolved(
                            "resolution operation budget exhausted".into(),
                        );
                    }
                    self.remaining -= 1;
                    let expanded = match crate::CandidateRenderer::render(
                        &candidate.template,
                        &module,
                        extension,
                    ) {
                        Ok(value) => value,
                        Err(error) => {
                            reason = error;
                            continue;
                        }
                    };
                    let path = match Self::normalize("", &expanded) {
                        Ok(path) => path,
                        Err(error) => {
                            reason = error;
                            continue;
                        }
                    };
                    if files.contains_key(&path) {
                        candidates
                            .entry(path)
                            .and_modify(|p| *p = (*p).min(candidate.priority))
                            .or_insert(candidate.priority);
                    }
                }
            }
        }
        let best = candidates.values().min().copied();
        let candidates: Vec<_> = candidates
            .into_iter()
            .filter(|(_, p)| Some(*p) == best)
            .map(|(path, _)| path)
            .collect();
        match candidates.as_slice() {
            [path] => ModuleResolution::Resolved(files[path].clone()),
            [] => ModuleResolution::Unresolved(reason),
            _ => ModuleResolution::Ambiguous(candidates),
        }
    }
}
