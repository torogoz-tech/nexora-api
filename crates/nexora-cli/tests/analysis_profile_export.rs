use std::{fs, process::Command};
#[test]
fn export_catalog_profile_is_portable_and_never_overwrites() {
    let path =
        std::env::temp_dir().join(format!("nexora-profile-export-{}.json", std::process::id()));
    let _ = fs::remove_file(&path);
    let output = Command::new(env!("CARGO_BIN_EXE_nexora"))
        .current_dir("../..")
        .args([
            "languages",
            "export-profile",
            "languages/lua",
            "lua",
            "lua",
            "1.0.0",
            "Lua",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = fs::read(&path).unwrap();
    let p: nexora_language::AnalysisProfilePackage = serde_json::from_slice(&bytes).unwrap();
    p.validate().unwrap();
    assert_eq!(p.language_id, "lua");
    let again = Command::new(env!("CARGO_BIN_EXE_nexora"))
        .current_dir("../..")
        .args([
            "languages",
            "export-profile",
            "languages/lua",
            "lua",
            "lua",
            "1.0.0",
            "Lua",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(!again.status.success());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    fs::remove_file(path).unwrap();
}
