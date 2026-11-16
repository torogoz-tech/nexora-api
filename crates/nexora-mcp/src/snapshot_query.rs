use crate::ToolCatalog;
use nexora_graph::{EdgeKind, NodeKind};
use nexora_storage::Snapshot;
use serde_json::{Value, json};

/// Queries immutable snapshot facts, without filesystem or provider access.
pub struct SnapshotQuery;
impl SnapshotQuery {
    pub fn run(s: &Snapshot, name: &str, args: &Value) -> Result<Value, String> {
        ToolCatalog::validate(name, args, false)?;
        if args["snapshot"].as_str().is_some_and(|id| id != s.id.0) {
            return Err("snapshot identity mismatch".into());
        }
        let mut result =
            json!({"tenant":s.tenant_id.0,"repository":s.repository_id.0,"snapshot":s.id.0});
        let offset = args["offset"].as_u64().unwrap_or(0) as usize;
        let limit = args["limit"].as_u64().unwrap_or(50) as usize;
        match name {
            "nexora_repository_overview" => {
                result["nodes"] = json!(s.graph.nodes.len());
                result["edges"] = json!(s.graph.edges.len());
                result["files"] = json!(s.sources.len());
                result["diagnostics"] = json!(s.diagnostics.len());
                result["analysis_engine"] = json!(s.analysis_engine);
                result["rule_version"] = json!(s.rule_version);
                result["coverage"] = json!({"parsed_regions":s.graph.syntax_coverage.values().map(|c|c.parsed_regions).sum::<usize>(),"partial_regions":s.graph.syntax_coverage.values().map(|c|c.partial_regions).sum::<usize>(),"opaque_regions":s.graph.syntax_coverage.values().map(|c|c.opaque_regions).sum::<usize>(),"note":"Coverage is limited to the selected template; counts do not prove complete semantics."});
                let values = s.sources.iter().map(|(path,source)| json!({"path":path,"bytes":source.len(),"syntax_coverage":s.graph.syntax_coverage.get(path),"source_coverage":s.source_coverage.iter().filter(|c|c.path==*path).collect::<Vec<_>>()}));
                Self::page(&mut result, values, offset, limit);
            }
            "nexora_find_symbol" => Self::page(
                &mut result,
                s.graph
                    .nodes
                    .iter()
                    .filter(|n| {
                        !matches!(n.kind, NodeKind::File | NodeKind::Repository)
                            && args
                                .get("name")
                                .and_then(Value::as_str)
                                .is_some_and(|name| n.name == name)
                    })
                    .map(|n| json!(n)),
                offset,
                limit,
            ),
            "nexora_find_references" | "nexora_get_dependencies" => {
                let node = Self::required_string(args, "node")?;
                if !s.graph.nodes.iter().any(|n| n.id.0 == node) {
                    return Err("node not found in snapshot".into());
                }
                Self::page(
                    &mut result,
                    s.graph
                        .edges
                        .iter()
                        .filter(|e| {
                            if name == "nexora_find_references" {
                                e.target.0 == node
                                    && !matches!(
                                        e.kind,
                                        EdgeKind::Contains
                                            | EdgeKind::Declares
                                            | EdgeKind::Interprets
                                            | EdgeKind::ScopeBridge
                                    )
                            } else {
                                e.source.0 == node
                                    && matches!(e.kind, EdgeKind::Imports | EdgeKind::DependsOn)
                            }
                        })
                        .map(|e| json!(e)),
                    offset,
                    limit,
                );
            }
            "nexora_search_code" => {
                let query = Self::required_string(args, "query")?;
                Self::page(
                    &mut result,
                    s.sources.iter().flat_map(|(path, source)| {
                        source
                            .lines()
                            .enumerate()
                            .filter(move |(_, line)| line.contains(query))
                            .map(move |(i, line)| Self::line(path, i + 1, line))
                    }),
                    offset,
                    limit,
                );
            }
            "nexora_get_source_context" => {
                let path = Self::required_string(args, "path")?;
                let source = s
                    .sources
                    .get(path)
                    .ok_or("source path not found in snapshot")?;
                let start = args["start_line"].as_u64().unwrap_or(1) as usize;
                let count = args["line_count"].as_u64().unwrap_or(40) as usize;
                let total = source.lines().count();
                let items: Vec<_> = source
                    .lines()
                    .enumerate()
                    .skip(start - 1)
                    .take(count)
                    .map(|(i, line)| Self::line(path, i + 1, line))
                    .collect();
                let end = (start - 1).saturating_add(items.len());
                result["total_lines"] = json!(total);
                result["next_line"] = if end < total {
                    json!(end + 1)
                } else {
                    Value::Null
                };
                result["truncated"] =
                    json!(end < total || items.iter().any(|i| i["text_truncated"] == true));
                result["items"] = json!(items);
            }
            _ => return Err("tool requires runtime".into()),
        }
        Ok(result)
    }
    fn required_string<'a>(args: &'a Value, key: &str) -> Result<&'a str, String> {
        args.get(key)
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{key} argument is required"))
    }
    fn page(result: &mut Value, values: impl Iterator<Item = Value>, offset: usize, limit: usize) {
        let mut total = 0;
        let mut items = Vec::new();
        for value in values {
            if total >= offset && items.len() < limit {
                items.push(value);
            }
            total += 1;
        }
        let next = offset.saturating_add(items.len());
        result["total"] = json!(total);
        result["next_offset"] = if next < total {
            json!(next)
        } else {
            Value::Null
        };
        result["truncated"] = json!(next < total);
        result["items"] = json!(items);
    }
    fn line(path: &str, line: usize, text: &str) -> Value {
        let mut end = text.len().min(8192);
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        json!({"path":path,"line":line,"text":&text[..end],"text_truncated":end<text.len()})
    }
}
