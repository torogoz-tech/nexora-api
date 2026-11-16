use crate::{
    AnalysisAttachment, AnalysisRequest, Criterion, OpenAiClient, ReasoningSession,
    http_fixture::{receive, reply},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::json;
use sha2::{Digest, Sha256};
#[test]
fn attachments_are_sent_as_data_images_and_quoted_by_reference_without_echoing_bytes() {
    let bytes = b"fixture-image-data";
    let attachment = AnalysisAttachment {
        id: "evd_1".into(),
        media_type: "image/png".into(),
        data_base64: STANDARD.encode(bytes),
        sha256: format!("{:x}", Sha256::digest(bytes)),
        metadata: json!({"phase":"after","trust":"actor_provided"}),
    };
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let task = std::thread::spawn(move || {
        let (socket, _, body) = receive(&listener);
        let content = body["input"][0]["content"].as_array().unwrap();
        assert!(content.iter().any(|c| {
            c["type"] == "input_image"
                && c["image_url"]
                    .as_str()
                    .unwrap()
                    .starts_with("data:image/png;base64,")
        }));
        reply(
            socket,
            200,
            json!({"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":json!({"summary":"Image supplied","criteria":[{"criterion_id":"C1","outcome":"insufficient_evidence","explanation":"Image alone cannot prove exact CSS delta","evidence_ids":["A1"],"missing_evidence":["Same-viewport DOM measurements"]}],"limitations":["Actor-provided capture"]}).to_string()}]}]}),
        );
    });
    let request = AnalysisRequest {
        purpose: "requirement_connection".into(),
        objective: "Logo10px".into(),
        criteria: vec![Criterion {
            id: "C1".into(),
            description: "Logo moved10 CSSpx".into(),
        }],
    };
    let (report, evidence) =
        ReasoningSession::new(OpenAiClient::test_client(format!("http://{address}")))
            .evaluate_with_attachments(&request, vec![], &[attachment], |_, _, _| {
                panic!("no tools required for insufficient evidence")
            })
            .unwrap();
    assert_eq!(report.criteria[0].outcome, "insufficient_evidence");
    assert_eq!(evidence[0].arguments["evidence_id"], "evd_1");
    assert!(
        !serde_json::to_string(&evidence)
            .unwrap()
            .contains("fixture-image-data")
    );
    task.join().unwrap();
}
#[test]
fn altered_attachment_hash_is_rejected_before_provider_contact() {
    let attachment = AnalysisAttachment {
        id: "evd_1".into(),
        media_type: "text/plain".into(),
        data_base64: STANDARD.encode(b"test"),
        sha256: "wrong".into(),
        metadata: json!({}),
    };
    assert!(attachment.input(0).is_err());
}
