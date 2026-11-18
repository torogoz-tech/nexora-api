use crate::*;
use serde_json::json;
fn request() -> AnalysisRequest {
    AnalysisRequest {
        purpose: "requirement_connection".into(),
        objective: "Check acceptance".into(),
        criteria: vec![Criterion {
            id: "C1".into(),
            description: "Reject negative amounts".into(),
        }],
    }
}
#[test]
fn rejects_fabricated_evidence_and_missing_criteria() {
    let mut report: AnalysisReport = serde_json::from_value(json!({"summary":"Checked", "criteria":[{"criterion_id":"C1","outcome":"supported","explanation":"A guard exists","evidence_ids":["made-up"],"missing_evidence":[]}],"limitations":[]})).unwrap();
    assert!(matches!(
        report.validate(&request(), &[]),
        Err(ReasoningError::Evidence)
    ));
    report.criteria.clear();
    assert!(report.validate(&request(), &[]).is_err());
}
#[test]
fn insufficient_evidence_requires_an_explicit_gap() {
    let mut report: AnalysisReport = serde_json::from_value(json!({"summary":"Not established", "criteria":[{"criterion_id":"C1","outcome":"insufficient_evidence","explanation":"No test run","evidence_ids":[],"missing_evidence":[]}],"limitations":[]})).unwrap();
    assert!(report.validate(&request(), &[]).is_err());
    report.criteria[0]
        .missing_evidence
        .push("Executed tests".into());
    report.validate(&request(), &[]).unwrap();
}
#[test]
#[cfg(feature = "repository-tools")]
fn bounded_loop_preserves_reasoning_and_binds_evidence() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let task = std::thread::spawn(move || {
        use std::io::{Read, Write};
        for round in 0..2 {
            let (mut socket, _) = listener.accept().unwrap();
            socket
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0u8; 4096];
            let body = loop {
                let n = socket.read(&mut buffer).unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&buffer[..n]);
                if let Some(end) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                    let head = String::from_utf8_lossy(&bytes[..end]);
                    let size: usize = head
                        .lines()
                        .find_map(|l| {
                            l.to_lowercase()
                                .strip_prefix("content-length:")
                                .map(|v| v.trim().parse().unwrap())
                        })
                        .unwrap();
                    if bytes.len() >= end + 4 + size {
                        break serde_json::from_slice::<serde_json::Value>(
                            &bytes[end + 4..end + 4 + size],
                        )
                        .unwrap();
                    }
                }
            };
            assert_eq!(body["store"], false);
            assert_eq!(body["text"]["format"]["strict"], true);
            let response = if round == 0 {
                json!({"status":"completed","output":[{"type":"reasoning","id":"rs_1","summary":[],"encrypted_content":"encrypted"},{"type":"function_call","call_id":"call_1","name":"nexora_search_code","arguments":"{\"query\":\"negative\",\"offset\":0,\"limit\":10}"}]})
            } else {
                let input = body["input"].as_array().unwrap();
                assert!(input.iter().any(|i| i["encrypted_content"] == "encrypted"));
                assert!(
                    input
                        .iter()
                        .any(|i| i["type"] == "function_call_output" && i["call_id"] == "call_1")
                );
                json!({"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":json!({"summary":"Guard found", "criteria":[{"criterion_id":"C1","outcome":"supported","explanation":"Negative input is rejected","evidence_ids":["E1"],"missing_evidence":[]}],"limitations":["Static evidence only"]}).to_string()}]}]})
            };
            let payload = response.to_string();
            write!(socket,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{}",payload.len(),payload).unwrap();
        }
    });
    let client = OpenAiClient::test_client(format!("http://{address}/v1/responses"));
    let session = ReasoningSession::new(client);
    let (report, evidence) = session
        .evaluate(
            &request(),
            RegisteredRepository::tools(),
            |name, args, _| {
                assert_eq!(name, "nexora_search_code");
                assert!(args.get("project_id").is_none());
                Ok(json!({"code":"if amount < 0 { return Err(...) }"}))
            },
        )
        .unwrap();
    assert_eq!(report.criteria[0].outcome, "supported");
    assert_eq!(evidence.len(), 1);
    task.join().unwrap();
}
#[test]
#[cfg(feature = "repository-tools")]
fn contribution_conclusions_require_actual_comparison_evidence() {
    let mut input = request();
    input.purpose = "contribution_review".into();
    let report:AnalysisReport=serde_json::from_value(json!({"summary":"Changes improve guard","criteria":[{"criterion_id":"C1","outcome":"supported","explanation":"Negative check added","evidence_ids":["E1"],"missing_evidence":[]}],"limitations":[]})).unwrap();
    let mut evidence = vec![Evidence {
        id: "E1".into(),
        tool: "nexora_get_source_context".into(),
        arguments: json!({}),
        sha256: "fixture".into(),
        result: json!({}),
    }];
    assert!(matches!(
        report.validate(&input, &evidence),
        Err(ReasoningError::Evidence)
    ));
    evidence[0].tool = ComparisonTool::NAME.into();
    report.validate(&input, &evidence).unwrap();
}
#[test]
#[cfg(feature = "repository-tools")]
fn comparison_arguments_cannot_change_authority_or_expand_the_window() {
    assert!(
        ComparisonQuery::parse(json!({"path":"payment.rs","start_line":1,"line_count":80})).is_ok()
    );
    for value in [
        json!({"path":"payment.rs","start_line":1,"line_count":81}),
        json!({"path":"payment.rs","start_line":1,"line_count":20,"snapshot":"other"}),
        json!({"path":"payment.rs","start_line":1,"line_count":20,"project_id":"other"}),
    ] {
        assert!(ComparisonQuery::parse(value).is_err());
    }
}

#[test]
#[cfg(feature = "repository-tools")]
fn an_unrelated_comparison_does_not_support_a_contribution_conclusion() {
    let mut input = request();
    input.purpose = "contribution_review".into();
    let report:AnalysisReport=serde_json::from_value(json!({"summary":"Quality","criteria":[{"criterion_id":"C1","outcome":"supported","explanation":"An overview is not a change comparison","evidence_ids":["E2"],"missing_evidence":[]}],"limitations":[]})).unwrap();
    let evidence = vec![
        Evidence {
            id: "E1".into(),
            tool: ComparisonTool::NAME.into(),
            arguments: json!({}),
            sha256: "fixture".into(),
            result: json!({}),
        },
        Evidence {
            id: "E2".into(),
            tool: "nexora_repository_overview".into(),
            arguments: json!({}),
            sha256: "fixture".into(),
            result: json!({}),
        },
    ];
    assert!(matches!(
        report.validate(&input, &evidence),
        Err(ReasoningError::Evidence)
    ));
}
