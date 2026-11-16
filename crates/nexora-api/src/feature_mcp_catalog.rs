use serde_json::{Value, json};
pub(crate) struct FeatureMcpCatalog;
impl FeatureMcpCatalog {
    pub fn list() -> Vec<Value> {
        let id = json!({"type":"string","minLength":1,"maxLength":128});
        let inputs: Value = serde_json::from_str(include_str!("../schemas/work_item_inputs.json"))
            .expect("work item schemas");
        let mut task = inputs["TaskCreate"].clone();
        task["properties"]["priority"] = inputs["TaskPriority"].clone();
        let proposal = json!({"type":"object","additionalProperties":false,"required":["requirement_id","requirement_version_id","expected_catalog_revision","reason","links","tasks"],"properties":{
         "requirement_id":id,"requirement_version_id":id,"expected_catalog_revision":{"type":"integer","minimum":0},"reason":{"type":"string","minLength":1,"maxLength":4000},
         "links":{"type":"array","minItems":1,"maxItems":32,"items":{"type":"object","additionalProperties":false,"required":["impact"],"properties":{"feature_id":{"type":["string","null"]},"expected_revision":{"type":["integer","null"],"minimum":1},"name":{"type":["string","null"],"maxLength":160},"description":{"type":["string","null"],"maxLength":4000},"impact":{"type":"string","minLength":1,"maxLength":4000}}}},
         "tasks":{"type":"array","maxItems":50,"items":{"type":"object","additionalProperties":false,"required":["feature_index","definition"],"properties":{"feature_index":{"type":"integer","minimum":0,"maximum":31},"definition":task}}}
        }});
        vec![
            json!({"name":"nexora_feature_complete","description":"Mark an existing feature completed directly. Manager or superadministrator only, with documents:write plus feature read authority in the project. Does not approve pending feature proposals or complete linked tasks. Supply current expected_revision and stable request_id.","inputSchema":{"type":"object","additionalProperties":false,"required":["project_id","feature_id","request_id","expected_revision"],"properties":{"project_id":id,"feature_id":id,"request_id":id,"expected_revision":{"type":"integer","minimum":1}}},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}),
            json!({"name":"nexora_features_list","description":"Inspect existing project features, requirement maps, catalog revision and pending proposals before planning a change. Fit requirements into existing features first. Requires documents:read and tasks:read in the project. No inferred grants.","inputSchema":{"type":"object","additionalProperties":false,"required":["project_id"],"properties":{"project_id":id}},"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}),
            json!({"name":"nexora_feature_map_propose","description":"Propose an exact requirement-version to features map after reading the feature catalog. Existing links require feature_id and expected_revision; new features require name and a justified impact. Optional proposed tasks reference a zero-based feature_index and may remain unassigned in the task pool. Creates only a pending proposal: no features or tasks until scoped technical-lead/PMO/manager human-session approval. MCP cannot decide. Use stable request_id for retries.","inputSchema":{"type":"object","additionalProperties":false,"required":["project_id","request_id","proposal"],"properties":{"project_id":id,"request_id":id,"proposal":proposal}},"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}),
        ]
    }
}
