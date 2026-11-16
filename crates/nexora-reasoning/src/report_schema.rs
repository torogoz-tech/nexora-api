use serde_json::{Value, json};
pub(crate) fn report_schema() -> Value {
    json!({"type":"object","additionalProperties":false,"required":["summary","criteria","limitations"],"properties":{
        "summary":{"type":"string"},
        "criteria":{"type":"array","items":{"type":"object","additionalProperties":false,"required":["criterion_id","outcome","explanation","evidence_ids","missing_evidence"],"properties":{
            "criterion_id":{"type":"string"},"outcome":{"type":"string","enum":["supported","contradicted","insufficient_evidence"]},"explanation":{"type":"string"},"evidence_ids":{"type":"array","items":{"type":"string"}},"missing_evidence":{"type":"array","items":{"type":"string"}}
        }}},"limitations":{"type":"array","items":{"type":"string"}}
    }})
}
