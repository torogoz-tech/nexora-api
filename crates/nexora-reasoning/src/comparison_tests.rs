use crate::{
    ReasoningError, RegisteredRepository,
    http_fixture::{receive, reply},
};
use serde_json::json;
use std::time::Duration;
use zeroize::Zeroizing;
#[test]
fn comparison_reads_both_fixed_snapshots_and_preserves_their_identity() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = std::thread::spawn(move || {
        for snapshot in ["baseline", "current"] {
            let (socket, _, body) = receive(&listener);
            assert_eq!(body["params"]["name"], "nexora_get_source_context");
            assert_eq!(body["params"]["arguments"]["snapshot"], snapshot);
            assert_eq!(body["params"]["arguments"]["project_id"], "project");
            let source = if snapshot == "baseline" {
                "amount >= 0"
            } else {
                "amount > 0"
            };
            reply(
                socket,
                200,
                json!({"id":body["id"],"result":{"isError":false,"structuredContent":{"project_id":"project","repository_id":"repo","snapshot":snapshot,"source":source}}}),
            );
        }
    });
    let mut current = RegisteredRepository::new(
        port,
        Zeroizing::new("fixture-key".into()),
        "project".into(),
        "repo".into(),
        Some("current".into()),
    )
    .unwrap();
    let mut baseline = RegisteredRepository::new(
        port,
        Zeroizing::new("fixture-key".into()),
        "project".into(),
        "repo".into(),
        Some("baseline".into()),
    )
    .unwrap();
    let result = current
        .compare_with(
            &mut baseline,
            json!({"path":"payment.rs","start_line":1,"line_count":20}),
            Duration::from_secs(2),
        )
        .unwrap();
    assert_eq!(result["before"]["snapshot"], "baseline");
    assert_eq!(result["after"]["snapshot"], "current");
    assert_eq!(result["authorship"], "not_independently_verified");
    task.join().unwrap();
}
#[test]
fn comparison_never_reads_current_after_baseline_access_is_denied() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let task = std::thread::spawn(move || {
        let (socket, _, body) = receive(&listener);
        assert_eq!(body["params"]["arguments"]["snapshot"], "baseline");
        reply(socket, 403, json!({"denied":true}));
    });
    let mut current = RegisteredRepository::new(
        port,
        Zeroizing::new("fixture-key".into()),
        "project".into(),
        "repo".into(),
        Some("current".into()),
    )
    .unwrap();
    let mut baseline = RegisteredRepository::new(
        port,
        Zeroizing::new("fixture-key".into()),
        "project".into(),
        "repo".into(),
        Some("baseline".into()),
    )
    .unwrap();
    assert!(matches!(
        current.compare_with(
            &mut baseline,
            json!({"path":"payment.rs","start_line":1,"line_count":20}),
            Duration::from_secs(2)
        ),
        Err(ReasoningError::Http(403))
    ));
    task.join().unwrap();
}
#[test]
fn comparison_rejects_same_snapshot_and_other_repository_without_network() {
    let make = |repo: &str, snap: &str| {
        RegisteredRepository::new(
            1,
            Zeroizing::new("fixture-key".into()),
            "project".into(),
            repo.into(),
            Some(snap.into()),
        )
        .unwrap()
    };
    let mut current = make("repo", "current");
    for mut invalid in [make("repo", "current"), make("other", "baseline")] {
        assert!(matches!(
            current.compare_with(
                &mut invalid,
                json!({"path":"payment.rs","start_line":1,"line_count":20}),
                Duration::from_secs(1)
            ),
            Err(ReasoningError::Repository)
        ));
    }
}
