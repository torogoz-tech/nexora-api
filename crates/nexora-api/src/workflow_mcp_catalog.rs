use serde_json::{Value, json};

pub(crate) struct WorkflowMcpCatalog;
impl WorkflowMcpCatalog {
    pub fn list() -> Vec<Value> {
        let id = json!({"type":"string","minLength":1,"maxLength":128});
        let name = json!({"type":"string","minLength":1,"maxLength":160});
        let text = json!({"type":"string","minLength":1,"maxLength":4000});
        let purpose = json!({"type":"string","maxLength":4000});
        let key =
            json!({"type":"string","minLength":1,"maxLength":64,"pattern":"^[A-Za-z0-9_-]+$"});
        let guard = json!({"oneOf":[
            {"type":"object","required":["kind","field"],"properties":{"kind":{"const":"required_field"},"field":{"type":"string","minLength":1,"maxLength":64,"pattern":"^[A-Za-z0-9_]+$"}},"additionalProperties":false},
            {"type":"object","required":["kind"],"properties":{"kind":{"type":"string","enum":["assignee","resolution","reason","dependencies_satisfied","evidence_valid","approved_document_version","human_approval","working_branch_reported"]}},"additionalProperties":false}
        ]});
        let mut graph = json!({"type":"object","additionalProperties":false,"required":["states","transitions"],"properties":{
            "states":{"type":"array","minItems":1,"maxItems":64,"items":{"type":"object","additionalProperties":false,"required":["id","name"],"properties":{"id":key,"name":name,"color":{"anyOf":[{"type":"string","pattern":"^#[0-9A-Fa-f]{6}$"},{"type":"null"}],"description":"Optional RGB display metadata, pinned to this workflow version."},"initial":{"type":"boolean"},"terminal":{"type":"boolean"},"publishable":{"type":"boolean","default":false,"description":"Explicit document publication eligibility; terminal alone does not grant publication."}}}},
            "transitions":{"type":"array","minItems":0,"maxItems":256,"items":{"type":"object","additionalProperties":false,"required":["id","from","to","name"],"properties":{"id":key,"from":key,"to":key,"name":{"type":"string","maxLength":160,"description":"Empty for a simple connection; optional action label."},"guards":{"type":"array","maxItems":32,"items":guard}}}}
        }});
        let state = &mut graph["properties"]["states"]["items"]["properties"];
        state["required_base_branch"] = json!({"type":["string","null"],"maxLength":512,"description":"Required work base ref, verified at the provider before entering or leaving this state; never created remotely."});
        state["progress_order"] = json!({"type":["integer","null"],"minimum":0,"maximum":65535});
        state["prevent_backtracking"] = json!({"type":"boolean","default":false,"description":"Requires explicit progress_order; rejects transitions to earlier or unranked states."});
        let transition = &mut graph["properties"]["transitions"]["items"];
        transition["required"] = json!(["id", "from", "to"]);
        transition["properties"]["steps"] = json!({"type":"array","maxItems":32,"description":"Ordered task actions: preconditions and approval, provider actions, exactly one advance_state, then notifications.","items":{"oneOf":[
            {"type":"object","additionalProperties":false,"required":["kind"],"properties":{"kind":{"enum":["require_approval","advance_state"]}}},
            {"type":"object","additionalProperties":false,"required":["kind"],"properties":{"kind":{"const":"verify_push"},"repository_id":id}},
            {"type":"object","additionalProperties":false,"required":["kind","source_branch","target_branch"],"properties":{"kind":{"const":"create_pull_request"},"repository_id":id,"source_branch":{"type":"string","minLength":1,"maxLength":512},"target_branch":{"type":"string","minLength":1,"maxLength":512}}},
            {"type":"object","additionalProperties":false,"required":["kind","channel"],"properties":{"kind":{"const":"notify"},"channel":{"enum":["email","in_app"]}}}
        ]}});
        let icon = json!({"type":"string","enum":["workflow","git-branch","code","bug","shield-check","rocket","layers","list-todo","briefcase","lightbulb","flask-conical","git-pull-request","palette","infinity","clipboard-check","app-window","server","smartphone","database","compass","headset","swatch-book","pen-tool","mouse-pointer","accessibility","monitor","component","terminal","cloud","container","network","cpu","blocks","test-tube","circle-check","gauge","scan-search","lock-keyhole","target","presentation","calendar-days","users","handshake","megaphone","shopping-cart","graduation-cap","heart-pulse","banknote"],"description":"Lucide icon on the current workflow definition; does not alter published versions."});
        let color = json!({"type":"string","pattern":"^(#[A-Fa-f0-9]{6})?$","description":"Workflow display color; empty string restores the theme color. Does not change published state colors."});
        let definition = json!({"type":"object","additionalProperties":false,"required":["name","graph"],"properties":{"name":name,"purpose":purpose,"icon":icon,"color":color,"graph":graph}});
        let mut change = json!({"type":"object","additionalProperties":false,"required":["expected_revision","reason"],"properties":{"expected_revision":{"type":"integer","minimum":1},"name":name,"purpose":purpose,"icon":icon,"color":color,"graph":graph,"archived":{"type":"boolean"},"reason":text}});
        for field in ["name", "purpose", "icon", "graph", "archived"] {
            let schema = change["properties"][field].take();
            change["properties"][field] = json!({"anyOf":[schema,{"type":"null"}]});
        }
        let publish = json!({"type":"object","additionalProperties":false,"required":["expected_revision","reason"],"properties":{"expected_revision":{"type":"integer","minimum":1},"reason":text}});
        [
            ("nexora_workflows_list","List global task workflow summaries, without full draft graphs.",true,vec![],json!({"after":id,"limit":{"type":"integer","minimum":1,"maximum":100},"include_archived":{"type":"boolean"}})),
            ("nexora_workflow_get","Read a global developer workflow and editable draft.",true,vec!["workflow_id"],json!({"workflow_id":id})),
            ("nexora_workflow_create","Create a project-independent task workflow draft. No roles or implicit default matrix. Use stable request_id for exact retry.",false,vec!["request_id","definition"],json!({"request_id":id,"definition":definition})),
            ("nexora_workflow_change","Change draft metadata or archive with expected_revision and reason; published versions remain immutable.",false,vec!["workflow_id","request_id","change"],json!({"workflow_id":id,"request_id":id,"change":change})),
            ("nexora_workflow_publish","Publish immutable workflow configuration with expected_revision and reason. Does not migrate existing task state.",false,vec!["workflow_id","request_id","publish"],json!({"workflow_id":id,"request_id":id,"publish":publish})),
            ("nexora_workflow_version_get","Read a published immutable version of this global task workflow.",true,vec!["workflow_id","version_id"],json!({"workflow_id":id,"version_id":id})),
            ("nexora_workflow_versions_list","List published version summaries newest first with a bounded before-number cursor.",true,vec!["workflow_id"],json!({"workflow_id":id,"before":{"type":"integer","minimum":1},"limit":{"type":"integer","minimum":1,"maximum":100}})),
            ("nexora_workflow_developers_list","List the developers bound to this global task workflow.",true,vec!["workflow_id"],json!({"workflow_id":id})),
            ("nexora_developer_workflows_list","List assigned workflows for a member; defaults to the current actor. Developers have one; technical leads, PMO, managers and superadministrators may have several. Choose an assigned workflow_id when taking a task.",true,vec![],json!({"actor_id":id})),
            ("nexora_developer_workflow_get","Read a single workflow membership. For multiple memberships use nexora_developer_workflows_list.",true,vec![],json!({"actor_id":id})),
            ("nexora_developer_workflow_assign","Assign a published workflow version. Developers can have one; technical leads, PMO, managers and superadministrators can have several. Use revision 0 for a new membership, or that membership revision to update its version.",false,vec!["actor_id","request_id","assignment"],json!({"actor_id":id,"request_id":id,"assignment":{"type":"object","additionalProperties":false,"required":["expected_revision","workflow_version_id"],"properties":{"expected_revision":{"type":"integer","minimum":0},"workflow_version_id":id,"reason":text}}})),
            ("nexora_developer_tasks_list","List the current developer's tasks across authorized projects, retaining their pinned workflow versions.",true,vec![],json!({"after":id,"limit":{"type":"integer","minimum":1,"maximum":100}})),
        ].into_iter().map(|(tool,description,read,extra,properties)|{
            let mut required=Vec::<&str>::new();required.extend(extra);
            json!({"name":tool,"description":description,"inputSchema":{"type":"object","additionalProperties":false,"required":required,"properties":properties},"annotations":{"readOnlyHint":read,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}})
        }).collect()
    }
}
