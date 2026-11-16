use nexora_core::NodeId;
use nexora_runtime::KnowledgeQuery;
use serde_json::{Map, Value};

pub(crate) struct CodeMcpQuery {
    pub project_id: String,
    pub repository_id: String,
    pub snapshot: Option<String>,
    pub query: KnowledgeQuery,
}
impl CodeMcpQuery {
    pub fn supports(name: &str) -> bool {
        matches!(
            name,
            "nexora_repository_overview"
                | "nexora_find_symbol"
                | "nexora_find_references"
                | "nexora_get_dependencies"
                | "nexora_traverse_graph"
                | "nexora_search_code"
                | "nexora_get_source_context"
        )
    }
    pub fn parse(name: &str, args: Value) -> Result<Self, (i64, &'static str)> {
        let invalid = || (-32602, "Invalid registered code query arguments");
        let mut args = args.as_object().cloned().ok_or_else(invalid)?;
        let project_id = args
            .remove("project_id")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(invalid)?;
        let repository_id = args
            .remove("repository_id")
            .and_then(|v| v.as_str().map(str::to_owned))
            .ok_or_else(invalid)?;
        if project_id.is_empty()
            || project_id.len() > 128
            || repository_id.is_empty()
            || repository_id.len() > 128
        {
            return Err(invalid());
        }
        nexora_mcp::ToolCatalog::validate(name, &Value::Object(args.clone()), false)
            .map_err(|_| invalid())?;
        let snapshot = args
            .get("snapshot")
            .and_then(Value::as_str)
            .map(str::to_owned);
        let offset = args.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
        let limit = args.get("limit").and_then(Value::as_u64).unwrap_or(50) as usize;
        let query = match name {
            "nexora_repository_overview" => KnowledgeQuery::Overview { offset, limit },
            "nexora_find_symbol" => KnowledgeQuery::FindSymbol {
                name: Self::text(&args, "name")?,
                offset,
                limit,
            },
            "nexora_find_references" => KnowledgeQuery::References {
                node: NodeId(Self::text(&args, "node")?),
                offset,
                limit,
            },
            "nexora_get_dependencies" => KnowledgeQuery::Dependencies {
                node: NodeId(Self::text(&args, "node")?),
                offset,
                limit,
            },
            "nexora_traverse_graph" => KnowledgeQuery::Traverse {
                node: NodeId(Self::text(&args, "node")?),
                depth: args.get("depth").and_then(Value::as_u64).unwrap_or(3) as usize,
                limit,
            },
            "nexora_search_code" => KnowledgeQuery::Search {
                query: Self::text(&args, "query")?,
                offset,
                limit,
            },
            "nexora_get_source_context" => KnowledgeQuery::Context {
                path: Self::text(&args, "path")?,
                start_line: args.get("start_line").and_then(Value::as_u64).unwrap_or(1) as usize,
                line_count: args.get("line_count").and_then(Value::as_u64).unwrap_or(40) as usize,
            },
            _ => return Err(invalid()),
        };
        Ok(Self {
            project_id,
            repository_id,
            snapshot,
            query,
        })
    }
    fn text(args: &Map<String, Value>, name: &str) -> Result<String, (i64, &'static str)> {
        args.get(name)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or((-32602, "Required code query text missing"))
    }
}
