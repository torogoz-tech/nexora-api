use crate::{McpConfiguration, SnapshotQuery, ToolCatalog};
use nexora_core::{NexoraError, NodeId, SnapshotId};
use nexora_indexer::{Indexer, SourceReader};
use nexora_language::LanguageRegistry;
use nexora_runtime::{DatabaseConnections, DatabaseRuntime};
use serde_json::{Value, json};

pub(crate) struct McpService {
    config: McpConfiguration,
    runtime: Option<DatabaseRuntime>,
}
impl McpService {
    pub fn new(config: McpConfiguration) -> Self {
        Self {
            config,
            runtime: None,
        }
    }
    pub fn call(&mut self, name: &str, args: &Value) -> Result<Value, String> {
        ToolCatalog::validate_with_knowledge(
            name,
            args,
            self.config.allow_index,
            self.config.knowledge_actor.is_some(),
        )?;
        let result = self.execute(name, args);
        if result.is_err() {
            self.runtime = None;
        }
        result.map_err(|e|match e {
            NexoraError::Limit(_)=>"operation exceeded an engine or storage limit; narrow the request".into(),
            NexoraError::InvalidPath(_)=>"configured source root is unavailable or contains a rejected path".into(),
            NexoraError::Configuration(_) if name.ends_with("_knowledge")=>"knowledge request rejected; reread expected_revision, check snapshot/evidence and revise stale claims before acceptance".into(),
            NexoraError::Storage(_) if name.ends_with("_knowledge")=>"knowledge unavailable or conflicting request; verify services and scope, reread the contribution, and reuse request_id only for an exact retry".into(),
            NexoraError::Configuration(_)=>"configuration unavailable or invalid; check server connection and language settings".into(),
            _=>"snapshot or node unavailable, or storage operation failed; verify the published scope and local services".into(),
        })
    }
    fn execute(&mut self, name: &str, args: &Value) -> Result<Value, NexoraError> {
        if self.runtime.is_none() {
            self.runtime = Some(DatabaseRuntime::connect(&DatabaseConnections::load(
                &self.config.connections,
            )?)?);
        }
        let runtime = self
            .runtime
            .as_mut()
            .ok_or_else(|| NexoraError::Storage("runtime initialization failed".into()))?;
        if name == "nexora_index_repository" {
            // Revalidate the configured root before every source-reading operation.
            if SourceReader::validate_path(&self.config.root)? != self.config.root {
                return Err(NexoraError::InvalidPath("root changed".into()));
            }
            let registry = LanguageRegistry::load(&self.config.language_dir)?;
            let language = if let Some(id) = &self.config.language {
                registry.get(id)
            } else if registry.languages().count() == 1 {
                registry.languages().next()
            } else {
                None
            }
            .cloned()
            .ok_or_else(|| {
                NexoraError::Configuration("select --language for a multi-pack catalog".into())
            })?;
            let indexer = if language.manifest.composition.is_some() {
                let deps = registry.composition_dependencies(&language)?;
                Indexer::with_catalog(language, deps)?
            } else {
                Indexer::new(language)
            };
            let result = runtime.index(&indexer, &self.config.root, self.config.tenant.clone())?;
            let mut summary =
                SnapshotQuery::run(&result.snapshot, "nexora_repository_overview", &json!({}))
                    .map_err(NexoraError::Configuration)?;
            summary["reuse"] = json!(result.reuse);
            summary["operation_ms"] = json!(result.operation_ms);
            summary["storage_ms"] = json!(result.storage_ms);
            return Ok(summary);
        }
        let id = args["snapshot"].as_str().map(|s| SnapshotId(s.into()));
        let offset = args["offset"].as_u64().unwrap_or(0) as usize;
        let limit = args["limit"].as_u64().unwrap_or(50) as usize;
        if name == "nexora_list_pending" {
            return runtime.knowledge_pending(
                &self.config.tenant,
                &self.config.repository,
                id.as_ref(),
                offset,
                limit,
            );
        }
        if matches!(
            name,
            "nexora_contribute_knowledge"
                | "nexora_revise_knowledge"
                | "nexora_review_knowledge"
                | "nexora_list_knowledge"
                | "nexora_get_knowledge"
        ) {
            let command = crate::knowledge_tool_catalog::KnowledgeToolCatalog::command(name, args)
                .map_err(NexoraError::Configuration)?;
            return runtime.knowledge(
                &self.config.tenant,
                &self.config.repository,
                id.as_ref(),
                self.config.knowledge_actor.as_deref().unwrap_or(""),
                &command,
            );
        }
        let query = match name {
            "nexora_repository_overview" => {
                nexora_runtime::KnowledgeQuery::Overview { offset, limit }
            }
            "nexora_find_symbol" => nexora_runtime::KnowledgeQuery::FindSymbol {
                name: args
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or_else(|| NexoraError::Configuration("name argument required".into()))?
                    .into(),
                offset,
                limit,
            },
            "nexora_find_references" => nexora_runtime::KnowledgeQuery::References {
                node: NodeId(
                    args.get("node")
                        .and_then(Value::as_str)
                        .ok_or_else(|| NexoraError::Configuration("node argument required".into()))?
                        .into(),
                ),
                offset,
                limit,
            },
            "nexora_get_dependencies" => nexora_runtime::KnowledgeQuery::Dependencies {
                node: NodeId(
                    args.get("node")
                        .and_then(Value::as_str)
                        .ok_or_else(|| NexoraError::Configuration("node argument required".into()))?
                        .into(),
                ),
                offset,
                limit,
            },
            "nexora_traverse_graph" => nexora_runtime::KnowledgeQuery::Traverse {
                node: NodeId(
                    args.get("node")
                        .and_then(Value::as_str)
                        .ok_or_else(|| NexoraError::Configuration("node argument required".into()))?
                        .into(),
                ),
                depth: args["depth"].as_u64().unwrap_or(3) as usize,
                limit,
            },
            "nexora_search_code" => nexora_runtime::KnowledgeQuery::Search {
                query: args
                    .get("query")
                    .and_then(Value::as_str)
                    .ok_or_else(|| NexoraError::Configuration("query argument required".into()))?
                    .into(),
                offset,
                limit,
            },
            "nexora_get_source_context" => nexora_runtime::KnowledgeQuery::Context {
                path: args
                    .get("path")
                    .and_then(Value::as_str)
                    .ok_or_else(|| NexoraError::Configuration("path argument required".into()))?
                    .into(),
                start_line: args["start_line"].as_u64().unwrap_or(1) as usize,
                line_count: args["line_count"].as_u64().unwrap_or(40) as usize,
            },
            _ => return Err(NexoraError::Configuration("unknown query".into())),
        };
        let result = runtime.query(
            &self.config.tenant,
            &self.config.repository,
            id.as_ref(),
            &query,
        );
        if let Some(metrics) = runtime.query_metrics() {
            tracing::debug!(cache_hit=metrics.cache_hit,cache_available=metrics.cache_available,metadata_us=metrics.metadata_us,cache_us=metrics.cache_us,backend_us=metrics.backend_us,backend=%metrics.backend,"selective query completed");
        }
        result
    }
}
