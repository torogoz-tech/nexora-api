use crate::ReasoningError;
use serde_json::{Value, json};
use std::time::Duration;
use zeroize::Zeroizing;
/// Authenticated read-only bridge. Model arguments never choose installation or scope.
pub struct RegisteredRepository {
    agent: ureq::Agent,
    endpoint: String,
    key: Zeroizing<String>,
    project: String,
    repository: String,
    snapshot: Option<String>,
    identity: Option<Value>,
    sequence: u64,
}
impl RegisteredRepository {
    pub fn new(
        port: u16,
        key: Zeroizing<String>,
        project: String,
        repository: String,
        snapshot: Option<String>,
    ) -> Result<Self, ReasoningError> {
        if port == 0
            || key.is_empty()
            || key.len() > 4096
            || !key.bytes().all(|c| c.is_ascii_graphic())
            || [&project, &repository]
                .iter()
                .any(|s| s.is_empty() || s.len() > 128)
            || snapshot
                .as_ref()
                .is_some_and(|s| s.is_empty() || s.len() > 4096)
        {
            return Err(ReasoningError::InvalidInput);
        }
        Ok(Self {
            agent: ureq::Agent::config_builder()
                .proxy(None)
                .max_redirects(0)
                .http_status_as_error(false)
                .timeout_global(Some(Duration::from_secs(15)))
                .build()
                .into(),
            endpoint: format!("http://127.0.0.1:{port}/mcp"),
            key,
            project,
            repository,
            snapshot,
            identity: None,
            sequence: 0,
        })
    }
    pub fn tools() -> Vec<Value> {
        nexora_mcp::ToolCatalog::list(false).into_iter()
            .filter(|t|Self::allowed(t["name"].as_str().unwrap_or("")))
            .map(|t|{
                let mut schema=t["inputSchema"].clone();
                let properties=schema["properties"].as_object_mut().expect("static tool schema");
                properties.remove("snapshot");
                let names:Vec<Value>=properties.keys().cloned().map(Value::String).collect();
                schema["required"]=json!(names);
                json!({"type":"function","name":t["name"],"description":t["description"],"parameters":schema,"strict":true})
            }).collect()
    }
    fn allowed(name: &str) -> bool {
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
    pub fn read(&mut self, name: &str, args: Value) -> Result<Value, ReasoningError> {
        self.read_with_timeout(name, args, Duration::from_secs(15))
    }
    pub fn read_with_timeout(
        &mut self,
        name: &str,
        args: Value,
        timeout: Duration,
    ) -> Result<Value, ReasoningError> {
        if !Self::allowed(name) {
            return Err(ReasoningError::Repository);
        }
        let mut args = args
            .as_object()
            .cloned()
            .ok_or(ReasoningError::InvalidInput)?;
        if ["project_id", "repository_id", "snapshot"]
            .iter()
            .any(|k| args.contains_key(*k))
        {
            return Err(ReasoningError::Repository);
        }
        nexora_mcp::ToolCatalog::validate(name, &Value::Object(args.clone()), false)
            .map_err(|_| ReasoningError::InvalidInput)?;
        args.insert("project_id".into(), json!(self.project));
        args.insert("repository_id".into(), json!(self.repository));
        if let Some(snapshot) = &self.snapshot {
            args.insert("snapshot".into(), json!(snapshot));
        }
        self.sequence += 1;
        let body = json!({"jsonrpc":"2.0","id":self.sequence,"method":"tools/call","params":{"name":name,"arguments":args}});
        let bearer = Zeroizing::new(format!("Bearer {}", self.key.as_str()));
        let mut auth =
            ureq::http::HeaderValue::from_str(&bearer).map_err(|_| ReasoningError::Credential)?;
        auth.set_sensitive(true);
        let mut response = self
            .agent
            .post(&self.endpoint)
            .config()
            .timeout_global(Some(timeout.min(Duration::from_secs(15))))
            .build()
            .header("Authorization", auth)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json, text/event-stream")
            .header("MCP-Protocol-Version", "2025-11-25")
            .send_json(&body)
            .map_err(|_| ReasoningError::Transport)?;
        if response.status().as_u16() != 200 {
            return Err(ReasoningError::Http(response.status().as_u16()));
        }
        let bytes = response
            .body_mut()
            .with_config()
            .limit(1048576)
            .read_to_vec()
            .map_err(|_| ReasoningError::Repository)?;
        let message: Value =
            serde_json::from_slice(&bytes).map_err(|_| ReasoningError::Repository)?;
        if message["id"] != self.sequence
            || message.get("error").is_some()
            || message["result"]["isError"] != false
        {
            return Err(ReasoningError::Repository);
        }
        let result = if let Some(v) = message["result"].get("structuredContent") {
            v.clone()
        } else {
            serde_json::from_str(
                message["result"]["content"][0]["text"]
                    .as_str()
                    .ok_or(ReasoningError::Repository)?,
            )
            .map_err(|_| ReasoningError::Repository)?
        };
        let snapshot = result["snapshot"]
            .as_str()
            .ok_or(ReasoningError::Repository)?;
        if result["project_id"] != self.project
            || result["repository_id"] != self.repository
            || self.snapshot.as_ref().is_some_and(|s| s != snapshot)
        {
            return Err(ReasoningError::Repository);
        }
        let identity = json!({"project_id":self.project,"repository_id":self.repository,"snapshot":snapshot,"commit":result["commit"],"local_origin":result["local_origin"]});
        if self.identity.as_ref().is_some_and(|i| i != &identity) {
            return Err(ReasoningError::Repository);
        }
        self.snapshot = Some(snapshot.into());
        self.identity = Some(identity);
        Ok(result)
    }
    pub fn compare_with(
        &mut self,
        baseline: &mut Self,
        args: Value,
        timeout: Duration,
    ) -> Result<Value, ReasoningError> {
        if self.project != baseline.project
            || self.repository != baseline.repository
            || self.snapshot.is_none()
            || baseline.snapshot.is_none()
            || self.snapshot == baseline.snapshot
        {
            return Err(ReasoningError::Repository);
        }
        let query = crate::ComparisonQuery::parse(args)?;
        let started = std::time::Instant::now();
        let before = baseline.read_with_timeout(
            "nexora_get_source_context",
            query.context_arguments(),
            timeout,
        )?;
        let remaining = timeout
            .checked_sub(started.elapsed())
            .filter(|d| !d.is_zero())
            .ok_or(ReasoningError::Budget)?;
        let after = self.read_with_timeout(
            "nexora_get_source_context",
            query.context_arguments(),
            remaining,
        )?;
        Ok(
            json!({"comparison":"selected_source_windows","before":before,"after":after,"authorship":"not_independently_verified"}),
        )
    }
    pub fn identity(&self) -> Option<&Value> {
        self.identity.as_ref()
    }
}
