use crate::evidence_validation::EvidenceValidation;
use base64::{Engine, engine::general_purpose::STANDARD};
use nexora_platform::EvidenceUpload;
use serde_json::json;
#[test]
fn evidence_validates_content_and_rejects_executable_or_unbounded_uploads() {
    let mut input:EvidenceUpload=serde_json::from_value(json!({"repository_id":"repo","name":"measurement.json","media_type":"application/json","data_base64":STANDARD.encode(b"{\"dy\":10}"),"phase":"supporting","description":"DOM measurements"})).unwrap();
    EvidenceValidation::decode(&input).unwrap();
    input.media_type = "text/html".into();
    assert!(EvidenceValidation::decode(&input).is_err());
    input.media_type = "image/png".into();
    assert!(EvidenceValidation::decode(&input).is_err());
    input.media_type = "application/json".into();
    input.data_base64 = "not base64".into();
    assert!(EvidenceValidation::decode(&input).is_err());
    input.data_base64 = STANDARD.encode(b"{}");
    input.document_id = Some("doc".into());
    assert!(EvidenceValidation::decode(&input).is_err());
    input.document_id = None;
    input.phase = "approved".into();
    assert!(EvidenceValidation::decode(&input).is_err());
}
