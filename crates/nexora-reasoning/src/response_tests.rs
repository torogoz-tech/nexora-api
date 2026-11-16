use crate::{
    AnalysisRequest, Criterion, OpenAiClient, ReasoningError, ReasoningSession,
    RegisteredRepository,
    http_fixture::{receive, reply},
};
use serde_json::{Value, json};
use std::time::Duration;
fn request() -> AnalysisRequest {
    AnalysisRequest {
        purpose: "code_review".into(),
        objective: "Review".into(),
        criteria: vec![Criterion {
            id: "C".into(),
            description: "Guard".into(),
        }],
    }
}
fn client_response(response: Value, status: u16) -> (OpenAiClient, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let task = std::thread::spawn(move || {
        let (socket, _, body) = receive(&listener);
        assert!(!body.to_string().contains("fixture-secret"));
        reply(socket, status, response)
    });
    (
        OpenAiClient::test_client(format!("http://{address}/v1/responses")),
        task,
    )
}
#[test]
fn refuses_incomplete_refusal_and_http_errors() {
    for (response, status) in [
        (json!({"status":"incomplete","output":[]}), 200),
        (
            json!({"status":"completed","output":[{"type":"message","content":[{"type":"refusal","refusal":"Cannot comply"}]}]}),
            200,
        ),
        (json!({"sensitive_provider_error":"not surfaced"}), 429),
    ] {
        let (client, task) = client_response(response, status);
        assert!(
            ReasoningSession::new(client)
                .evaluate(&request(), RegisteredRepository::tools(), |_, _, _| panic!(
                    "must not read"
                ))
                .is_err()
        );
        task.join().unwrap();
    }
}
#[test]
fn expired_budget_does_not_accept_a_late_tool_result() {
    let (client, task) = client_response(
        json!({"status":"completed","output":[{"type":"function_call","name":"nexora_search_code","call_id":"c","arguments":"{\"query\":\"guard\"}"}]}),
        200,
    );
    let result = ReasoningSession::new(client)
        .with_budget(Duration::from_millis(100))
        .evaluate(&request(), RegisteredRepository::tools(), |_, _, _| {
            std::thread::sleep(Duration::from_millis(130));
            Ok(json!({"code":"x"}))
        });
    assert!(matches!(result, Err(ReasoningError::Budget)));
    task.join().unwrap();
}
#[test]
fn empty_allowlist_never_executes_a_model_tool() {
    let (client, task) = client_response(
        json!({"status":"completed","output":[{"type":"function_call","name":"run_shell","call_id":"c","arguments":"{}"}]}),
        200,
    );
    assert!(
        ReasoningSession::new(client)
            .evaluate(&request(), vec![], |_, _, _| panic!("must not execute"))
            .is_err()
    );
    task.join().unwrap();
}
