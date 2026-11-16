use nexora_platform_store::SecretGenerator;
use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Output},
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

#[test]
fn local_mcp_header_helper_reads_private_key_without_databases_or_secret_diagnostics() {
    let directory = std::env::temp_dir().join(format!(
        "nexora_mcp_headers_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&directory).unwrap();
    let path = directory.join("actor.key");
    let secret = Zeroizing::new(SecretGenerator::token("fixture_key").unwrap());
    fs::write(&path, secret.as_bytes()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o700)).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    let invoke = |path: &Path| -> Output {
        Command::new(env!("CARGO_BIN_EXE_nexora-api"))
            .arg("--connections")
            .arg(directory.join("absent-connections.json"))
            .arg("mcp-headers")
            .arg("--api-key-file")
            .arg(path)
            .output()
            .unwrap()
    };
    let output = invoke(&path);
    assert!(
        output.status.success(),
        "MCP credential helper must work without connecting to databases"
    );
    assert!(output.stderr.is_empty());
    let headers: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        headers["Authorization"].as_str() == Some(format!("Bearer {}", secret.as_str()).as_str()),
        "Credential header must preserve the key"
    );
    assert_eq!(headers.as_object().unwrap().len(), 1);
    fs::write(&path, format!("{}\r\nX-Injected: true", secret.as_str())).unwrap();
    let output = invoke(&path);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(
        !output
            .stderr
            .windows(secret.len())
            .any(|window| window == secret.as_bytes()),
        "Invalid key diagnostic must not echo credentials"
    );
    fs::write(&path, secret.as_bytes()).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        let output = invoke(&path);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert!(
            !output
                .stderr
                .windows(secret.len())
                .any(|window| window == secret.as_bytes())
        );
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let linked = directory.join("linked.key");
        std::os::unix::fs::symlink(&path, &linked).unwrap();
        let output = invoke(&linked);
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
    }
    fs::write(&path, vec![b'x'; 8192]).unwrap();
    let output = invoke(&path);
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    let output = invoke(&directory.join("missing.key"));
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    fs::remove_dir_all(&directory).unwrap();
}
