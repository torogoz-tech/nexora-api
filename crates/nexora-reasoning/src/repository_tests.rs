use crate::{
    ReasoningError, RegisteredRepository,
    http_fixture::{receive, reply},
};
use serde_json::json;
use zeroize::Zeroizing;
#[test]
fn repository_pins_snapshot_rejects_scope_injection_and_revoked_key() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = std::thread::spawn(move || {
        for round in 0..3 {
            let (socket, headers, body) = receive(&listener);
            assert!(
                headers
                    .to_lowercase()
                    .contains("authorization: bearer fixture-key")
            );
            assert_eq!(body["params"]["arguments"]["project_id"], "project");
            assert_eq!(body["params"]["arguments"]["repository_id"], "repo");
            if round > 0 {
                assert_eq!(body["params"]["arguments"]["snapshot"], "snap1");
            }
            if round == 2 {
                reply(socket, 401, json!({"error":"revoked"}));
                continue;
            }
            reply(
                socket,
                200,
                json!({"jsonrpc":"2.0","id":body["id"],"result":{"isError":false,"structuredContent":{"project_id":"project","repository_id":"repo","snapshot":"snap1","commit":"abc","items":[]}}}),
            );
        }
    });
    let mut repository = RegisteredRepository::new(
        port,
        Zeroizing::new("fixture-key".into()),
        "project".into(),
        "repo".into(),
        None,
    )
    .unwrap();
    assert!(matches!(
        repository.read(
            "nexora_search_code",
            json!({"query":"x","project_id":"other"})
        ),
        Err(ReasoningError::Repository)
    ));
    assert!(matches!(
        repository.read("nexora_index_repository", json!({})),
        Err(ReasoningError::Repository)
    ));
    repository
        .read("nexora_repository_overview", json!({"offset":0,"limit":1}))
        .unwrap();
    repository
        .read(
            "nexora_search_code",
            json!({"query":"guard","offset":0,"limit":1}),
        )
        .unwrap();
    assert!(matches!(
        repository.read("nexora_repository_overview", json!({"offset":0,"limit":1})),
        Err(ReasoningError::Http(401))
    ));
    task.join().unwrap();
}
#[test]
fn repository_rejects_changed_snapshot() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = std::thread::spawn(move || {
        let (socket, _, body) = receive(&listener);
        reply(
            socket,
            200,
            json!({"jsonrpc":"2.0","id":body["id"],"result":{"isError":false,"structuredContent":{"project_id":"project","repository_id":"repo","snapshot":"different","commit":"abc"}}}),
        );
    });
    let mut repository = RegisteredRepository::new(
        port,
        Zeroizing::new("fixture-key".into()),
        "project".into(),
        "repo".into(),
        Some("pinned".into()),
    )
    .unwrap();
    assert!(matches!(
        repository.read("nexora_repository_overview", json!({})),
        Err(ReasoningError::Repository)
    ));
    task.join().unwrap();
}
