use clap::Parser;
use nexora_api::ApiArguments;

#[test]
fn worker_is_available_as_an_operator_command() {
    assert!(
        ApiArguments::try_parse_from([
            "nexora-api",
            "worker",
            "--settings",
            "/operator/worker.json",
            "--git-policy",
            "/operator/git.json",
            "--once",
        ])
        .is_ok()
    );
}
