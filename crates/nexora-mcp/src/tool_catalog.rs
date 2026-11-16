use serde_json::{Value, json};

/// The advertised catalog is also the source of primitive argument validation.
pub struct ToolCatalog;
impl ToolCatalog {
    pub fn list(allow_index: bool) -> Vec<Value> {
        Self::list_with_knowledge(allow_index, false)
    }
    pub fn list_with_knowledge(allow_index: bool, allow_knowledge: bool) -> Vec<Value> {
        let text = || json!({"type":"string","minLength":1,"maxLength":4096});
        let number = |min, max| json!({"type":"integer","minimum":min,"maximum":max});
        let mut tools = Vec::new();
        for (name, description, required, extra) in [
            (
                "nexora_repository_overview",
                "Repository counts, diagnostic counts, coverage and a page of files. Partial coverage is not proof of complete understanding.",
                vec![],
                json!({}),
            ),
            (
                "nexora_find_symbol",
                "Exact-name graph node search; returns original evidence, not inferred types or behavior.",
                vec!["name"],
                json!({"name":text()}),
            ),
            (
                "nexora_find_references",
                "Actual incoming semantic edges, excluding CONTAINS, DECLARES, INTERPRETS and SCOPE_BRIDGE. Preserves unresolved/ambiguous resolution; empty results do not prove absence of references.",
                vec!["node"],
                json!({"node":text()}),
            ),
            (
                "nexora_get_dependencies",
                "Actual outgoing IMPORTS and DEPENDS_ON edges of a node. Unresolved edges may target reference nodes, not resolved files.",
                vec!["node"],
                json!({"node":text()}),
            ),
            (
                "nexora_traverse_graph",
                "Traverse outgoing persisted Neo4j edges; returns node/edge facts and evidence, with explicit truncation.",
                vec!["node"],
                json!({"node":text(),"depth":number(0,32)}),
            ),
            (
                "nexora_search_code",
                "Literal case-sensitive substring search over stored snapshot sources, with bounded Unicode snippets.",
                vec!["query"],
                json!({"query":text()}),
            ),
            (
                "nexora_get_source_context",
                "Stored source lines from a snapshot, never live filesystem reads. Line numbers start at 1; long lines are explicitly truncated.",
                vec!["path"],
                json!({"path":text(),"start_line":number(1,1_000_000),"line_count":number(1,200)}),
            ),
        ] {
            let mut properties = json!({"snapshot":text()});
            properties["snapshot"]["description"] = json!(
                "Published snapshot ID returned by earlier calls. Omit this field to select current; do not invent a snapshot ID."
            );
            if name != "nexora_get_source_context" {
                properties["limit"] = number(1, 100);
            }
            if name != "nexora_traverse_graph" && name != "nexora_get_source_context" {
                properties["offset"] = number(0, 1_000_000);
            }
            properties
                .as_object_mut()
                .unwrap()
                .extend(extra.as_object().unwrap().clone());
            tools.push(json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}));
        }
        if allow_index {
            tools.push(json!({"name":"nexora_index_repository","description":"Analyze only the configured root and publish a new current snapshot using PostgreSQL, Neo4j and Dragonfly. Reads source files but never executes them. Cancellation may not stop publication already in progress.","inputSchema":{"type":"object","properties":{},"additionalProperties":false},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":false,"openWorldHint":false}}));
        }
        tools.extend(crate::knowledge_tool_catalog::KnowledgeToolCatalog::list(
            allow_knowledge,
        ));
        tools
    }

    pub fn validate(name: &str, args: &Value, allow_index: bool) -> Result<(), String> {
        Self::validate_with_knowledge(name, args, allow_index, false)
    }
    pub fn validate_with_knowledge(
        name: &str,
        args: &Value,
        allow_index: bool,
        allow_knowledge: bool,
    ) -> Result<(), String> {
        let tool = Self::list_with_knowledge(allow_index, allow_knowledge)
            .into_iter()
            .find(|t| t["name"] == name)
            .ok_or("unknown or disabled tool")?;
        Self::validate_value(args, &tool["inputSchema"], 0)?;
        if matches!(
            name,
            "nexora_contribute_knowledge"
                | "nexora_revise_knowledge"
                | "nexora_review_knowledge"
                | "nexora_list_knowledge"
                | "nexora_get_knowledge"
        ) {
            crate::knowledge_tool_catalog::KnowledgeToolCatalog::command(name, args)?;
        }
        Ok(())
    }
    fn validate_value(value: &Value, rule: &Value, depth: usize) -> Result<(), String> {
        if depth > 8 {
            return Err("argument nesting too deep".into());
        }
        if let Some(options) = rule["enum"].as_array()
            && !options.contains(value)
        {
            return Err("argument outside advertised choices".into());
        }
        if let Some(types) = rule["type"].as_array() {
            if types.len() != 2
                || !types.iter().any(|t| t == "string")
                || !types.iter().any(|t| t == "null")
            {
                return Err("unsupported nullable argument".into());
            }
            if value.is_null() {
                return Ok(());
            }
            let mut non_null = rule.clone();
            non_null["type"] = json!("string");
            return Self::validate_value(value, &non_null, depth + 1);
        }
        match rule["type"].as_str() {
            Some("object") => {
                let object = value.as_object().ok_or("argument must be an object")?;
                if let Some(required) = rule["required"].as_array() {
                    for key in required {
                        if !object.contains_key(key.as_str().unwrap()) {
                            return Err(format!("missing argument: {}", key.as_str().unwrap()));
                        }
                    }
                }
                for (key, value) in object {
                    let child = rule["properties"].get(key).ok_or("unknown argument")?;
                    Self::validate_value(value, child, depth + 1)?;
                }
            }
            Some("array") => {
                let items = value.as_array().ok_or("argument must be an array")?;
                if items.len() < rule["minItems"].as_u64().unwrap_or(0) as usize
                    || items.len() > rule["maxItems"].as_u64().unwrap_or(8) as usize
                {
                    return Err("array length outside advertised range".into());
                }
                for item in items {
                    Self::validate_value(item, &rule["items"], depth + 1)?;
                }
            }
            Some("string") => {
                let s = value.as_str().ok_or("argument must be a string")?;
                if s.len() < rule["minLength"].as_u64().unwrap_or(1) as usize
                    || s.len() > rule["maxLength"].as_u64().unwrap_or(4096) as usize
                {
                    return Err("string argument length outside advertised range".into());
                }
            }
            Some("integer") => {
                let n = value
                    .as_u64()
                    .ok_or("argument must be an unsigned integer")?;
                if n < rule["minimum"].as_u64().unwrap() || n > rule["maximum"].as_u64().unwrap() {
                    return Err("integer argument outside advertised range".into());
                }
            }
            _ => return Err("unsupported argument".into()),
        }
        Ok(())
    }
}
