use nexora_api::RepositoryWorkerSettings;
use serde_json::json;

#[path = "../../nexora-source/tests/support/test_directory.rs"]
mod test_directory;

#[test]
fn worker_settings_reject_untrusted_paths_and_unknown_options() {
    let parse = |root: &str| {
        serde_json::from_value::<RepositoryWorkerSettings>(json!({
            "id": "fixture-worker",
            "source_root": root,
            "git_binary": "/usr/bin/git",
            "profiles": {}
        }))
        .unwrap()
    };
    assert!(parse("relative/path").validate().is_err());
    assert!(parse("/").validate().is_err());
    assert!(
        serde_json::from_value::<RepositoryWorkerSettings>(json!({
            "id": "fixture-worker",
            "source_root": "/tmp/worker",
            "git_binary": "/usr/bin/git",
            "profiles": {},
            "client_tenant_id": "untrusted"
        }))
        .is_err()
    );
}

#[cfg(unix)]
#[test]
fn worker_root_must_be_private_physical_directory() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let temporary = test_directory::TestDirectory::new("worker-settings");
    let root = temporary.path().canonicalize().unwrap();
    let parse = |path: &std::path::Path| {
        serde_json::from_value::<RepositoryWorkerSettings>(json!({
            "id": "fixture-worker", "source_root": path,
            "git_binary": "/usr/bin/git", "profiles": {}
        }))
        .unwrap()
    };
    assert!(parse(&root).validate().is_ok());
    let alias = root.join("alias");
    symlink(&root, &alias).unwrap();
    assert!(parse(&alias).validate().is_err());
    std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(parse(&root).validate().is_err());
}
