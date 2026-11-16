#[path = "../../nexora-platform-store/tests/support/local_snapshot_fixture.rs"]
mod local_snapshot_fixture;
#[path = "../../nexora-platform-store/tests/support/work_item_fixture.rs"]
mod work_item_fixture;
use local_snapshot_fixture::LocalSnapshotFixture;
use nexora_platform::{Capability, KeyScope};
use nexora_platform_store::SecretGenerator;
use serde_json::Value;
use std::{path::Path, process::Command};
use zeroize::Zeroizing;

#[test]
#[ignore = "requires local databases; operator CLI uses private human credentials, preserves IDs, closes owned sessions and rejects unsafe credential files"]
fn operator_cli_adopts_revokes_and_reauthorizes_without_exposing_credentials() {
    let executable = env!("CARGO_BIN_EXE_nexora-api");
    let help = Command::new(executable).arg("--help").output().unwrap();
    let help = String::from_utf8(help.stdout).unwrap();
    for name in ["adopt-local", "revoke-local", "reauthorize-local"] {
        assert!(help.contains(name), "missing operator command {name}");
    }
    let mut f = LocalSnapshotFixture::new();
    let password = Zeroizing::new(SecretGenerator::token("fixture_password").unwrap());
    let operator = f
        .work
        .store
        .create_user(&f.work.ctx, "operator", &password, "Local fixture operator")
        .unwrap();
    f.work
        .store
        .grant(
            &f.work.ctx,
            &operator.id,
            Capability::InstallationManage,
            None,
        )
        .unwrap();
    f.work
        .store
        .grant(
            &f.work.ctx,
            &operator.id,
            Capability::RepositoriesWrite,
            Some(&f.work.project.id),
        )
        .unwrap();
    let session = f.work.store.login("operator", &password).unwrap();
    let private = std::env::temp_dir()
        .canonicalize()
        .unwrap()
        .join(SecretGenerator::id("nexora-operator-cli").unwrap());
    std::fs::create_dir(&private).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&private, std::fs::Permissions::from_mode(0o700)).unwrap();
    }
    let session_file = private.join("operator-session");
    let password_file = private.join("operator-password");
    for (path, value) in [
        (&session_file, session.access_token.as_str()),
        (&password_file, password.as_str()),
    ] {
        std::fs::write(path, value).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).unwrap();
        }
    }
    let connections = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.nexora/local/connections.json")
        .canonicalize()
        .unwrap();
    let run = |args: &[&str]| {
        Command::new(executable)
            .current_dir(&private)
            .arg("--connections")
            .arg(&connections)
            .arg("--schema")
            .arg(&f.work.namespace)
            .args(args)
            .output()
            .unwrap()
    };
    let root = f.root.to_str().unwrap();
    let session_path = session_file.to_str().unwrap();
    let password_path = password_file.to_str().unwrap();
    let project = &f.work.project.id;
    let adopt = [
        "adopt-local",
        "--project",
        project,
        "--name",
        "Verified CLI fixture",
        "--root",
        root,
        "--runtime-tenant",
        &f.header.tenant_id.0,
        "--snapshot",
        &f.header.id.0,
        "--expected-hash",
        &f.header.payload_hash,
        "--reason",
        "Adopt exact public fixture",
        "--request-id",
        "cli-adoption",
        "--session-file",
        session_path,
    ];
    let output = run(&adopt);
    assert!(
        output.status.success(),
        "operator adoption failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let first: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(first["runtime_repository_id"], f.header.repository_id.0);
    assert_eq!(first["payload_hash"], f.header.payload_hash);
    assert!(first.get("root").is_none());
    let encoded = String::from_utf8(output.stdout).unwrap();
    assert!(!encoded.contains(session.access_token.as_str()));
    let repeated = run(&adopt);
    assert!(repeated.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&repeated.stdout).unwrap(),
        first
    );
    assert!(
        f.work.store.authenticate(&session.access_token).is_ok(),
        "caller-owned session must remain open"
    );
    let bad_hash = "0".repeat(64);
    let mut failed = adopt[..adopt.len() - 2].to_vec();
    let hash_index = failed
        .iter()
        .position(|arg| *arg == "--expected-hash")
        .unwrap()
        + 1;
    let request_index = failed
        .iter()
        .position(|arg| *arg == "--request-id")
        .unwrap()
        + 1;
    failed[hash_index] = &bad_hash;
    failed[request_index] = "failed-password-attestation";
    failed.extend(["--username", "operator", "--password-file", password_path]);
    assert!(
        !run(&failed).status.success(),
        "a failed password-authenticated operation must also close its owned session"
    );
    let key = f
        .work
        .store
        .issue_key(
            &f.work.ctx,
            f.work.ctx.actor_id(),
            "Human key cannot act as operator session",
            KeyScope {
                capabilities: vec![
                    Capability::InstallationManage,
                    Capability::RepositoriesWrite,
                ],
                project_ids: vec![project.clone()],
                all_projects: false,
                installation: true,
            },
            3600,
        )
        .unwrap();
    std::fs::write(&session_file, &key.key).unwrap();
    let rejected = run(&adopt);
    assert!(!rejected.status.success());
    assert!(!String::from_utf8_lossy(&rejected.stderr).contains(key.key.as_str()));
    std::fs::write(&session_file, "x".repeat(4096)).unwrap();
    assert!(!run(&adopt).status.success());
    std::fs::write(&session_file, &session.access_token).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::{PermissionsExt, symlink};
        std::fs::set_permissions(&session_file, std::fs::Permissions::from_mode(0o644)).unwrap();
        let rejected = run(&adopt);
        assert!(!rejected.status.success());
        assert!(!String::from_utf8_lossy(&rejected.stderr).contains(session.access_token.as_str()));
        std::fs::set_permissions(&session_file, std::fs::Permissions::from_mode(0o600)).unwrap();
        let link = private.join("session-link");
        symlink(&session_file, &link).unwrap();
        let mut linked = adopt;
        linked[linked.len() - 1] = link.to_str().unwrap();
        assert!(!run(&linked).status.success());
    }
    let repository = first["repository_id"].as_str().unwrap();
    let revoked = run(&[
        "revoke-local",
        "--project",
        project,
        "--repository",
        repository,
        "--expected-revision",
        "1",
        "--reason",
        "Explicit operator revocation",
        "--request-id",
        "cli-revoke",
        "--username",
        "operator",
        "--password-file",
        password_path,
    ]);
    assert!(
        revoked.status.success(),
        "operator revocation failed: {}",
        String::from_utf8_lossy(&revoked.stderr)
    );
    let revoked: Value = serde_json::from_slice(&revoked.stdout).unwrap();
    assert_eq!(revoked["state"], "revoked");
    let mut reauthorize = adopt.to_vec();
    reauthorize[0] = "reauthorize-local";
    reauthorize.extend(["--repository", repository, "--expected-revision", "2"]);
    let restored = run(&reauthorize);
    assert!(
        restored.status.success(),
        "explicit reauthorization failed: {}",
        String::from_utf8_lossy(&restored.stderr)
    );
    let restored: Value = serde_json::from_slice(&restored.stdout).unwrap();
    assert_eq!(restored["repository_id"], first["repository_id"]);
    assert_eq!(restored["source_id"], first["source_id"]);
    assert_eq!(restored["authorization_revision"], 3);
    let active: i64 = f
        .sql
        .query_one(
            "SELECT count(*) FROM sessions WHERE actor_id=$1 AND revoked_at IS NULL",
            &[&operator.id],
        )
        .unwrap()
        .get(0);
    assert_eq!(active, 1, "CLI-created password session must be closed");
    assert_eq!(
        f.runtime
            .query_header(&f.header.tenant_id, &f.header.repository_id, None)
            .unwrap()
            .payload_hash,
        f.header.payload_hash
    );
    std::fs::remove_dir_all(&private).unwrap();
    std::fs::remove_dir_all(&f.root).unwrap();
}
