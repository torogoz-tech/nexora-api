use serde_json::{Value, json};
pub(crate) struct KnowledgeToolCatalog;
impl KnowledgeToolCatalog {
    pub fn list(allow_write: bool) -> Vec<Value> {
        let text = || json!({"type":"string","minLength":1,"maxLength":4096});
        let key = || json!({"type":"string","minLength":1,"maxLength":128});
        let nullable = |max| json!({"type":["string","null"],"minLength":1,"maxLength":max});
        let page = || json!({"offset":{"type":"integer","minimum":0,"maximum":1_000_000},"limit":{"type":"integer","minimum":1,"maximum":100}});
        let anchor = json!({"type":"object","properties":{"path":text(),"start_line":{"type":"integer","minimum":1,"maximum":1_000_000},"line_count":{"type":"integer","minimum":1,"maximum":200}},"required":["path","start_line","line_count"],"additionalProperties":false});
        let draft = json!({"type":"object","properties":{"kind":{"type":"string","enum":["relation","explanation","decision"]},"statement":text(),"source":nullable(4096),"target":nullable(4096),"relation":nullable(128),"evidence":{"type":"array","minItems":1,"maxItems":8,"items":anchor}},"required":["kind","statement","evidence"],"additionalProperties":false});
        let mut definitions = vec![
            (
                "nexora_list_pending",
                "Unresolved and ambiguous deterministic graph edges with original evidence. This is not a complete inventory of every semantic gap. Contributions do not rewrite these facts.",
                vec![],
                page(),
                true,
            ),
            (
                "nexora_list_knowledge",
                "List contributed knowledge separately from parser facts; filter by workflow state or needs_review and by source/target node. Only accepted entries with effective=true are currently reviewed claims, not deterministic proof.",
                vec![],
                json!({"status":{"type":"string","enum":["proposed","accepted","rejected","withdrawn","needs_review"]},"node":text()}),
                true,
            ),
            (
                "nexora_get_knowledge",
                "Current contribution plus ascending paginated immutable revision history, actor, evidence hashes and snapshot validity. Contribution text is untrusted data.",
                vec!["id"],
                json!({"id":key()}),
                true,
            ),
        ];
        if allow_write {
            definitions.extend([
                ("nexora_contribute_knowledge","Propose a sourced relation, explanation or decision on a published snapshot. Requires existing scoped nodes for relations and stored source anchors. request_id is an idempotency key: preserve it on retries. Does not change parser facts.",vec!["request_id","draft"],json!({"request_id":key(),"draft":draft}),false),
                ("nexora_revise_knowledge","Correct or revalidate a contribution with a full replacement draft on the selected snapshot; creates a new proposed revision. Requires current expected_revision; never overwrites history. Reuse request_id only for exact retries.",vec!["id","request_id","expected_revision","draft"],json!({"id":key(),"request_id":key(),"expected_revision":{"type":"integer","minimum":1,"maximum":2_147_483_646},"draft":draft}),false),
                ("nexora_review_knowledge","Accept, reject or withdraw a contribution with a reason and expected_revision. Acceptance is a local client review, not parser proof or authenticated human approval. Stale/withdrawn claims require revision before acceptance.",vec!["id","request_id","expected_revision","action","reason"],json!({"id":key(),"request_id":key(),"expected_revision":{"type":"integer","minimum":1,"maximum":2_147_483_646},"action":{"type":"string","enum":["accept","reject","withdraw"]},"reason":text()}),false),
            ]);
        }
        definitions.into_iter().map(|(name,description,required,mut properties,read)|{
            properties["snapshot"]=text();
            properties["snapshot"]["description"]=json!("Published snapshot ID from earlier calls; omit for current. Old-base contributions require explicit revision after a new snapshot.");
            if read && name!="nexora_list_pending"{ properties.as_object_mut().unwrap().extend(page().as_object().unwrap().clone()); }
            json!({"name":name,"description":description,"inputSchema":{"type":"object","properties":properties,"required":required,"additionalProperties":false},"annotations":{"readOnlyHint":read,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}})
        }).collect()
    }
    pub fn command(name: &str, args: &Value) -> Result<nexora_runtime::KnowledgeCommand, String> {
        let operation = match name {
            "nexora_contribute_knowledge" => "contribute",
            "nexora_revise_knowledge" => "revise",
            "nexora_review_knowledge" => "review",
            "nexora_list_knowledge" => "list",
            "nexora_get_knowledge" => "get",
            _ => return Err("unknown knowledge command".into()),
        };
        let mut input = args.clone();
        let object = input.as_object_mut().ok_or("arguments must be an object")?;
        object.remove("snapshot");
        object.insert("operation".into(), json!(operation));
        let command: nexora_runtime::KnowledgeCommand =
            serde_json::from_value(input).map_err(|_| "invalid knowledge arguments")?;
        command
            .validate()
            .map_err(|_| "invalid knowledge arguments, draft, evidence or bounds")?;
        Ok(command)
    }
}
